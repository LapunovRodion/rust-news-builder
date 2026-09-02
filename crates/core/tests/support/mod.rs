//! Shared fixture loading for the parity suite.
//!
//! Constitution III asserts parity against files captured from the Python reference, so these
//! helpers do one thing: put the Rust pipeline in the same position the reference was in when
//! the goldens were taken, and hand back what it produced.
//!
//! Regenerate the fixtures with `cargo run -p capture-reference`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use newsbuilder_core::build::{BuildContext, BuildOutput, EmbeddedBytes, build};
use newsbuilder_core::import::{attach_photos, import_document};
use newsbuilder_core::model::item::{NewsItem, SourceFormat};
use newsbuilder_core::model::photo::PhotoSource;
use newsbuilder_core::model::server::Slug;

/// The connection values `tools/capture-reference/capture.py` captured with. Changing either
/// here without changing it there silently breaks every URL comparison.
pub const PUBLIC_BASE_URL: &str = "https://example.org/news/2026/03/";
pub const REMOTE_BASE_PATH: &str = "/var/www/html/news";

/// The repository root, found from this file rather than from the working directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn input_dir(case: &str) -> PathBuf {
    repo_root().join("fixtures/inputs").join(case)
}

pub fn golden_dir(case: &str) -> PathBuf {
    repo_root().join("fixtures/reference").join(case)
}

/// Reads a golden file, failing with a message that says how to regenerate it.
pub fn golden(case: &str, name: &str) -> String {
    let path = golden_dir(case).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}\nregenerate with `cargo run -p capture-reference`",
            path.display()
        )
    })
}

/// Whether a golden file exists.
pub fn has_golden(case: &str, name: &str) -> bool {
    golden_dir(case).join(name).is_file()
}

/// Every fixture case, in a stable order.
pub fn all_cases() -> Vec<String> {
    let mut cases: Vec<String> = std::fs::read_dir(repo_root().join("fixtures/inputs"))
        .expect("fixtures/inputs must exist")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    cases.sort();
    cases
}

/// The input document of a case, with the format its extension implies.
pub fn document(case: &str) -> Option<(Vec<u8>, SourceFormat)> {
    for name in ["input.txt", "input.md", "input.docx", "news.docx"] {
        let path = input_dir(case).join(name);
        if path.is_file() {
            let format = SourceFormat::from_file_name(name)?;
            return Some((std::fs::read(&path).ok()?, format));
        }
    }
    None
}

/// The case's images, as `(file_name, source, bytes)` ready for `attach_photos`.
///
/// Ordering is left to `attach_photos`, which applies the reference's natural sort — reading
/// them in whatever order the filesystem offers is exactly the enumeration-order dependency
/// constitution IV forbids.
pub fn images(case: &str) -> Vec<(String, PhotoSource, Vec<u8>)> {
    let dir = input_dir(case).join("images");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).ok()?;
            Some((
                name,
                PhotoSource::Bytes(Arc::from(bytes.clone().into_boxed_slice())),
                bytes,
            ))
        })
        .collect()
}

/// The folder name the reference used, read back out of the case's `paths.json`.
///
/// A case whose title transliterates to nothing gets the reference's `news-<timestamp>`
/// fallback, which the capture masks. There is no fixed folder to compare against, so those
/// cases return `None` and are left to `parity_slug.rs`, which asserts the *rule* instead.
pub fn golden_folder(case: &str) -> Option<Slug> {
    let raw = std::fs::read_to_string(golden_dir(case).join("paths.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if value
        .get("news_folder_is_timestamped")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return None;
    }
    Slug::parse(value.get("news_folder")?.as_str()?)
}

/// The case's item, imported and with its photos attached — the state the reference was in
/// immediately before it published.
///
/// The slug is taken from the golden rather than re-derived, so a publish test compares against
/// the folder the reference actually used.
pub fn item(case: &str) -> Option<NewsItem> {
    let (bytes, format) = document(case)?;
    let folder = golden_folder(case)?;
    let mut imported = import_document(&bytes, format).ok()?;
    attach_photos(&mut imported.item, images(case));
    imported.item.slug = folder;
    Some(imported.item)
}

/// Builds a case exactly as the reference was invoked: document plus an images folder,
/// published beneath the captured public base URL.
///
/// Returns `None` when the case cannot be compared this way — a document with no images
/// folder, or one the reference itself refused.
pub fn build_case(case: &str) -> Option<BuildOutput> {
    if images(case).is_empty() {
        return None;
    }
    let item = item(case)?;
    let ctx = BuildContext {
        public_base_url: Some(url::Url::parse(PUBLIC_BASE_URL).ok()?),
        slug: item.slug.clone(),
    };
    build(&item, &ctx, &EmbeddedBytes).ok()
}

/// Cases whose fragment can be compared against the reference byte for byte.
///
/// Word cases are excluded on purpose: the port derives placements from the positions of a
/// document's embedded images (FR-003, deviation D-6), a capability the reference does not
/// have — it reads its photos from a folder instead. The two therefore render different, and
/// both correct, fragments. Their *text* goldens are still compared, in `parity_text.rs`.
pub fn fragment_comparable_cases() -> Vec<String> {
    all_cases()
        .into_iter()
        .filter(|case| !case.starts_with("word-"))
        .filter(|case| has_golden(case, "fragment.html"))
        .filter(|case| golden_folder(case).is_some())
        .collect()
}
