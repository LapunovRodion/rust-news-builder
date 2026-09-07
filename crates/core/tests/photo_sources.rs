//! T082 — a photo from a path and a photo from bytes are the same photo.
//!
//! Three intake routes exist — dropped, pasted, picked — and FR-006 to FR-008 say they are the
//! same feature seen from three angles. `add_photos` is the single door, and the only thing
//! that distinguishes the routes is the [`PhotoSource`] and the [`PhotoOrigin`] they carry.
//! Everything downstream — dimensions, orientation, naming, placement, the published bytes —
//! must be indistinguishable, or the "no folder required" promise of US4 would come with a
//! second-class path.

mod support;

use std::path::{Path, PathBuf};

use newsbuilder_core::build::{BuildContext, EmbeddedBytes, FileStoreBytes, build};
use newsbuilder_core::model::item::{Block, Layout, NewsItem, PhotoIntake, add_photos};
use newsbuilder_core::model::photo::{PhotoOrigin, PhotoSource};
use newsbuilder_core::ports::FileStore;

use support::jpeg_bytes;

/// A file store holding exactly one file, so the path route has something to read.
#[derive(Debug)]
struct OneFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl FileStore for OneFile {
    fn read(&self, path: &Path) -> newsbuilder_core::Result<Vec<u8>> {
        if path == self.path {
            Ok(self.bytes.clone())
        } else {
            Err(newsbuilder_core::Error::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
            })
        }
    }

    fn write(&self, path: &Path, _bytes: &[u8]) -> newsbuilder_core::Result<()> {
        panic!("FR-015: {} must not be written", path.display())
    }

    fn exists(&self, path: &Path) -> newsbuilder_core::Result<bool> {
        Ok(path == self.path)
    }
}

fn one_photo_item(intake: PhotoIntake) -> NewsItem {
    let mut item = NewsItem::new();
    item.title = "Photo Sources".to_owned();
    item.slug = newsbuilder_core::publish::slug::slugify(&item.title);
    let added = add_photos(&mut item, vec![intake]);
    assert_eq!(added.ids.len(), 1, "{:?}", added.warnings);
    item.body = vec![Block::placement(added.ids, Layout::FullWidth)];
    item
}

fn context(item: &NewsItem) -> BuildContext {
    BuildContext {
        public_base_url: Some(
            url::Url::parse("https://example.org/news/2026/03/").expect("a valid url"),
        ),
        slug: item.slug.clone(),
    }
}

#[test]
fn a_path_and_bytes_produce_the_same_photo_record() {
    let bytes = jpeg_bytes(400, 300, 7);
    let path = PathBuf::from("/photos/holiday.jpg");

    let from_bytes = one_photo_item(PhotoIntake::from_bytes(
        "holiday.jpg".to_owned(),
        bytes.clone(),
        PhotoOrigin::Pasted,
    ));
    let from_path = one_photo_item(PhotoIntake::from_path(
        path.clone(),
        bytes.clone(),
        PhotoOrigin::Picked { path: path.clone() },
    ));

    let a = &from_bytes.photos[0];
    let b = &from_path.photos[0];
    assert_eq!(
        a.file_name, b.file_name,
        "the name comes from the path's last component"
    );
    assert_eq!(a.dimensions, b.dimensions);
    assert_eq!(a.orientation, b.orientation);
    assert_eq!(a.adjust, b.adjust);
    assert_eq!(a.natural_key, b.natural_key);
    assert_eq!(a.id, b.id);
}

#[test]
fn only_the_source_and_the_origin_differ() {
    let bytes = jpeg_bytes(400, 300, 7);
    let path = PathBuf::from("/photos/holiday.jpg");

    let from_bytes = one_photo_item(PhotoIntake::from_bytes(
        "holiday.jpg".to_owned(),
        bytes.clone(),
        PhotoOrigin::Pasted,
    ));
    let from_path = one_photo_item(PhotoIntake::from_path(
        path.clone(),
        bytes,
        PhotoOrigin::Picked { path: path.clone() },
    ));

    assert!(matches!(from_bytes.photos[0].source, PhotoSource::Bytes(_)));
    assert_eq!(from_path.photos[0].source, PhotoSource::Path(path));
    assert_eq!(from_bytes.photos[0].origin, PhotoOrigin::Pasted);
    assert!(matches!(
        from_path.photos[0].origin,
        PhotoOrigin::Picked { .. }
    ));
}

#[test]
fn both_routes_build_byte_identical_output() {
    // The claim that matters: whichever way the photo arrived, the CMS gets the same thing.
    let bytes = jpeg_bytes(400, 300, 7);
    let path = PathBuf::from("/photos/holiday.jpg");

    let from_bytes = one_photo_item(PhotoIntake::from_bytes(
        "holiday.jpg".to_owned(),
        bytes.clone(),
        PhotoOrigin::Pasted,
    ));
    let from_path = one_photo_item(PhotoIntake::from_path(
        path.clone(),
        bytes.clone(),
        PhotoOrigin::Picked { path: path.clone() },
    ));

    let store = OneFile { path, bytes };
    let a = build(&from_bytes, &context(&from_bytes), &EmbeddedBytes).expect("builds");
    let b = build(&from_path, &context(&from_path), &FileStoreBytes(&store)).expect("builds");

    assert_eq!(a.fragment, b.fragment);
    assert_eq!(a.processed.len(), 1);
    assert_eq!(a.processed[0].file_name, b.processed[0].file_name);
    assert_eq!(a.processed[0].dimensions, b.processed[0].dimensions);
    assert_eq!(a.processed[0].bytes, b.processed[0].bytes);
}

#[test]
fn a_path_intake_takes_its_name_from_the_last_component() {
    let bytes = jpeg_bytes(60, 40, 1);
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        vec![PhotoIntake::from_path(
            PathBuf::from("/home/editor/Pictures/2026/opening.jpg"),
            bytes,
            PhotoOrigin::Dropped,
        )],
    );
    assert_eq!(item.photos[0].file_name, "opening.jpg");
}

#[test]
fn the_routes_share_the_rejection_rules_too() {
    // A `.pdf` is a `.pdf` whether it was dropped or pasted.
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![
            PhotoIntake::from_path(
                PathBuf::from("/tmp/notes.pdf"),
                b"%PDF".to_vec(),
                PhotoOrigin::Dropped,
            ),
            PhotoIntake::from_bytes(
                "notes.pdf".to_owned(),
                b"%PDF".to_vec(),
                PhotoOrigin::Pasted,
            ),
        ],
    );
    assert!(added.ids.is_empty());
    assert_eq!(added.warnings.len(), 2);
}

#[test]
fn a_mixed_drop_keeps_its_order() {
    // Order is what the editor sees in the photo list and what decides the published numbering,
    // so it follows the order the intake arrived in — not the source kind.
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        vec![
            PhotoIntake::from_bytes(
                "b.jpg".to_owned(),
                jpeg_bytes(60, 40, 1),
                PhotoOrigin::Pasted,
            ),
            PhotoIntake::from_path(
                PathBuf::from("/tmp/a.jpg"),
                jpeg_bytes(60, 40, 2),
                PhotoOrigin::Dropped,
            ),
        ],
    );
    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(names, vec!["b.jpg", "a.jpg"]);
}
