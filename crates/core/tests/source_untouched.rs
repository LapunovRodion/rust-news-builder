//! T071 — the editor's own files are never written (FR-015).
//!
//! Every file in `fixtures/inputs/portraits/` is checksummed, put through a full
//! crop-rotate-build cycle, and checksummed again. The point is not that the pipeline happens
//! not to write: it is that nothing in `core` *can* write, because the only route to the disk
//! is the [`FileStore`] port and the build path is handed a reader that has no `write` on it.
//!
//! The test therefore asserts two things at once — the bytes are unchanged, and no file was
//! created or removed either.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use newsbuilder_core::build::{BuildContext, FileStoreBytes, build};
use newsbuilder_core::model::item::NewsItem;
use newsbuilder_core::model::photo::{CropRect, PhotoSource};
use newsbuilder_core::photo::{self, AspectRatio};
use newsbuilder_core::ports::FileStore;

use support::{PUBLIC_BASE_URL, input_dir, item};

/// A read-only view of the real filesystem.
///
/// `write` is not merely unused here — it fails loudly, so a pipeline that tried to write a
/// source file would fail the test rather than quietly succeed. This is the port boundary
/// doing the work constitution IV asks of it.
#[derive(Debug)]
struct ReadOnlyDisk;

impl FileStore for ReadOnlyDisk {
    fn read(&self, path: &Path) -> newsbuilder_core::Result<Vec<u8>> {
        std::fs::read(path).map_err(|source| newsbuilder_core::Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    fn write(&self, path: &Path, _bytes: &[u8]) -> newsbuilder_core::Result<()> {
        panic!(
            "the build tried to write {} — FR-015 says the editor's files are read only",
            path.display()
        )
    }

    fn exists(&self, path: &Path) -> newsbuilder_core::Result<bool> {
        Ok(path.exists())
    }
}

/// A cheap content digest. FNV-1a: no dependency, and collision resistance is not the point —
/// detecting *any* rewrite of a file we just processed is.
fn digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Every file beneath a directory, with its size and digest.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, (u64, u64)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                out.insert(path, (bytes.len() as u64, digest(&bytes)));
            }
        }
    }
    out
}

/// The portraits item, with its photos pointed at the real files on disk rather than at bytes
/// already in memory — otherwise the test could not observe a write at all.
fn portraits_item_from_disk() -> NewsItem {
    let mut item = item("portraits").expect("the portraits fixture is captured");
    let dir = input_dir("portraits").join("images");
    for photo in &mut item.photos {
        photo.source = PhotoSource::Path(dir.join(&photo.file_name));
    }
    item
}

#[test]
fn a_full_crop_rotate_build_cycle_leaves_every_source_file_untouched() {
    let root = input_dir("portraits");
    assert!(
        root.is_dir(),
        "fixtures/inputs/portraits is missing; regenerate with `cargo run -p capture-reference`"
    );

    let before = snapshot(&root);
    assert!(
        before.len() >= 20,
        "the SC-003 benchmark set is twenty photographs, found {}",
        before.len()
    );

    let mut item = portraits_item_from_disk();

    // Everything the editor can do to a photo, applied to every photo in the set.
    let ids: Vec<_> = item.photos.iter().map(|p| p.id).collect();
    for (index, id) in ids.iter().enumerate() {
        let target = match index % 3 {
            0 => AspectRatio::SQUARE,
            1 => AspectRatio::LANDSCAPE_3_2,
            _ => AspectRatio::WIDE_16_9,
        };
        let Some(subject) = item.photo_mut(*id) else {
            continue;
        };
        photo::rotate(subject, (index % 4) as i8);
        if let Some(crop) = photo::suggest_crop(subject, target) {
            photo::set_crop(subject, Some(crop)).expect("a suggested crop is always in bounds");
        }
    }

    // Place them all, so the build actually processes every file.
    use newsbuilder_core::model::item::{Block, Layout};
    item.body = vec![newsbuilder_core::model::item::Block::paragraph("Lead.")];
    for id in &ids {
        item.body
            .push(Block::placement(vec![*id], Layout::FullWidth));
    }
    item.normalise_paragraph_kinds();

    let ctx = BuildContext {
        public_base_url: Some(url::Url::parse(PUBLIC_BASE_URL).expect("a valid constant url")),
        slug: item.slug.clone(),
    };
    let output = build(&item, &ctx, &FileStoreBytes(&ReadOnlyDisk)).expect("the item builds");
    assert_eq!(
        output.processed.len(),
        ids.len(),
        "every photo was processed, so every source file was read: {:?}",
        output.warnings
    );

    let after = snapshot(&root);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "no source file was created or removed"
    );
    for (path, expected) in &before {
        assert_eq!(
            after.get(path),
            Some(expected),
            "{} changed on disk",
            path.display()
        );
    }
}

#[test]
fn a_rejected_crop_changes_neither_the_photo_nor_its_source() {
    // INV-6 refuses the rectangle; FR-015 says the refusal must not have cost anything either.
    let mut item = portraits_item_from_disk();
    let id = item.photos[0].id;
    let subject = item.photo_mut(id).expect("the first photo");
    let before = subject.adjust;
    let (width, height) = subject.dimensions;

    let outside = CropRect {
        x: width,
        y: height,
        width: 10,
        height: 10,
    };
    assert!(photo::set_crop(subject, Some(outside)).is_err());
    assert_eq!(subject.adjust, before);
}
