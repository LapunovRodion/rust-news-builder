//! The open item, and the view of it the frontend gets (T042).
//!
//! The Rust side owns the [`NewsItem`]; the frontend holds a projection it never mutates on its
//! own. Every command applies a change here and hands back a fresh [`ItemView`], so there is
//! exactly one copy of the truth and no reconciliation to get wrong
//! (contracts/desktop-commands.md).
//!
//! Two things this module exists to prevent:
//!
//! - **Full-size photo bytes crossing the IPC boundary.** A thirty-photo item is tens of
//!   megabytes; serialising that into JSON on every keystroke is how SC-008's one-second
//!   rebuild becomes ten. Photos leave as `thumbUrl`s — small JPEGs written to the application
//!   cache and served over Tauri's asset protocol.
//! - **The frontend composing any part of the news page.** The view carries structure, never
//!   HTML and never marker text (deviation D-9).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use newsbuilder_core::build::{
    BuildContext, BuildOutput, PhotoBytesSource, ProcessCache, build_with_cache,
};
use newsbuilder_core::error::{Error, Result, Warning};
use newsbuilder_core::model::item::{Block, Layout, NewsItem, ParagraphKind};
use newsbuilder_core::model::photo::{CropRect, Photo, PhotoId, PhotoSource};
use newsbuilder_core::photo::ThumbnailCache;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{CommandError, CommandResult};

// =============================================================================================
// Views — what crosses the boundary
// =============================================================================================

/// The whole item, as the frontend sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub title: String,
    pub blocks: Vec<BlockView>,
    pub photos: Vec<PhotoView>,
    pub slug: String,
    /// Whether there are changes the editor has not exported or published.
    pub dirty: bool,
}

/// One body block.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlockView {
    Paragraph {
        text: String,
        /// Whether this is the lead paragraph, which renders differently.
        lead: bool,
    },
    Placement {
        #[serde(rename = "photoIds")]
        photo_ids: Vec<String>,
        layout: Layout,
    },
}

/// One photo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoView {
    pub id: String,
    pub file_name: String,
    pub width: u32,
    pub height: u32,
    /// Whether any placement references it (FR-010).
    pub used: bool,
    /// An asset-protocol URL for the cached thumbnail. Empty when it could not be rendered.
    pub thumb_url: String,
    pub crop: Option<CropRectView>,
    /// Quarter-turns clockwise, 0–3.
    pub rotation: u8,
}

/// A crop, in source pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRectView {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl From<CropRect> for CropRectView {
    fn from(crop: CropRect) -> Self {
        Self {
            x: crop.x,
            y: crop.y,
            width: crop.width,
            height: crop.height,
        }
    }
}

impl From<CropRectView> for CropRect {
    fn from(view: CropRectView) -> Self {
        Self {
            x: view.x,
            y: view.y,
            width: view.width,
            height: view.height,
        }
    }
}

/// One body block on the way *in* (contracts/desktop-commands.md).
///
/// Structure, never a string containing markers: the editor works on placement cards and never
/// types or sees marker text (deviation D-9).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlockInput {
    Paragraph {
        text: String,
    },
    Placement {
        #[serde(rename = "photoIds")]
        photo_ids: Vec<String>,
        layout: Layout,
    },
}

/// A warning, as the frontend sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningView {
    pub kind: String,
    pub detail: String,
}

impl From<&Warning> for WarningView {
    fn from(warning: &Warning) -> Self {
        let kind = match warning {
            Warning::MissingPhoto { .. } => "missing_photo",
            Warning::MalformedMarker { .. } => "malformed_marker",
            Warning::UnusedPhoto { .. } => "unused_photo",
            Warning::PhotoSkipped { .. } => "photo_skipped",
            Warning::UnsupportedDocumentFeature { .. } => "unsupported_document_feature",
            Warning::SizeBudgetUnreachable { .. } => "size_budget_unreachable",
            Warning::SlugSuffixed { .. } => "slug_suffixed",
            Warning::UnknownAppearanceKey { .. } => "unknown_appearance_key",
            _ => "warning",
        };
        Self {
            kind: kind.to_owned(),
            detail: warning.to_string(),
        }
    }
}

/// Renders a warning list for the frontend.
pub fn warning_views(warnings: &[Warning]) -> Vec<WarningView> {
    warnings.iter().map(WarningView::from).collect()
}

// =============================================================================================
// The session
// =============================================================================================

/// Everything one window is editing.
#[derive(Debug)]
pub struct SessionState {
    /// The item. There is exactly one.
    pub item: NewsItem,
    /// Original bytes, by photo, so a rebuild never re-reads the disk.
    ///
    /// A photo that came from a path is read once, on the way in, and kept here. This is what
    /// makes FR-015 structural as well as intended: after intake the source file is never
    /// opened again, so nothing the editor does can write to it.
    pub bytes: HashMap<PhotoId, Vec<u8>>,
    /// Rendered thumbnails, keyed by photo and adjustment.
    pub thumbnails: ThumbnailCache,
    /// Photos already encoded to the publishing budget, so a rebuild after a text edit
    /// re-encodes nothing (SC-008, T102).
    ///
    /// The preview rebuilds on every keystroke and a thirty-photo item is thirty decodes and
    /// thirty quality-ladder searches; paying that per keystroke is the difference between
    /// SC-008's 150 ms and several seconds. Cleared in [`SessionState::replace`], because the
    /// cache is keyed by `PhotoId` and a new item starts its ids over.
    pub processed: ProcessCache,
    /// Where thumbnails are written for the asset protocol.
    pub thumbnail_dir: PathBuf,
    /// Where the preview's photos are written for the asset protocol.
    ///
    /// The preview fragment is `core::build`'s own string and the frontend may not touch it, so
    /// the photos have to be reachable at the URLs the build already put in it. Handing the
    /// build an asset-protocol base and then putting the files where it says is what makes that
    /// true — and it is the same mechanism on every platform, which a custom URI scheme would
    /// not be (Tauri serves those as `scheme://` on Linux and `http://scheme.localhost` on
    /// Windows).
    pub preview_dir: PathBuf,
    /// The asset-protocol URL of [`preview_dir`](Self::preview_dir), which every preview build
    /// is given as its public base.
    pub preview_base: Url,
    /// What is already on disk under `preview_dir`, by path and content.
    ///
    /// The build names photos after the title, so a re-crop reuses a name; without this the
    /// preview would keep showing the pixels the name had last time.
    preview_written: HashMap<PathBuf, u64>,
    /// Whether there are unexported changes (edge case: closing with unsaved work).
    pub dirty: bool,
    /// A credential entered for this session only, when the OS store is unreachable (FR-041).
    ///
    /// Held in memory and written nowhere. Keyed by server name.
    pub session_credentials: HashMap<String, newsbuilder_core::Secret>,
}

/// The managed state Tauri hands to every command.
#[derive(Debug)]
pub struct Session(pub Mutex<SessionState>);

impl SessionState {
    /// A session holding an empty item, with its caches beneath `cache`.
    ///
    /// # Panics
    ///
    /// Only if the cache directory's path cannot be made into a URL, which percent-encoding it
    /// rules out. There is no window to report it in at that point.
    pub fn new(cache: &Path) -> Self {
        let preview_dir = preview_dir(cache);
        Self {
            item: NewsItem::new(),
            bytes: HashMap::new(),
            thumbnails: ThumbnailCache::new(),
            processed: ProcessCache::new(),
            thumbnail_dir: thumbnail_dir(cache),
            preview_base: Url::parse(&asset_url(&preview_dir))
                .expect("a percent-encoded path makes a well-formed asset URL"),
            preview_dir,
            preview_written: HashMap::new(),
            dirty: false,
            session_credentials: HashMap::new(),
        }
    }

    /// Replaces the open item, dropping everything that belonged to the old one.
    pub fn replace(&mut self, item: NewsItem, bytes: HashMap<PhotoId, Vec<u8>>) {
        self.item = item;
        self.bytes = bytes;
        self.thumbnails.clear();
        // Both caches are keyed by `PhotoId`, and the new item's ids start over — keeping
        // either would serve the old item's photo under the new item's id.
        self.processed.clear();
        self.dirty = false;
        // Thumbnails on disk belong to the item that is gone; leaving them would let a stale
        // image show under a re-used id. The preview's copies go for the same reason: they are
        // named after the old title.
        let _ = std::fs::remove_dir_all(&self.thumbnail_dir);
        let _ = std::fs::remove_dir_all(&self.preview_dir);
        self.preview_written.clear();
    }

    /// The item as the frontend sees it, rendering any thumbnail that is missing or stale.
    pub fn view(&mut self) -> ItemView {
        let placed = self.item.placed_photo_ids();
        let blocks = self.item.body.iter().map(BlockView::from).collect();

        // Collected first because rendering borrows `self` mutably while the item is borrowed.
        let photos: Vec<Photo> = self.item.photos.clone();
        let mut views = Vec::with_capacity(photos.len());
        for photo in &photos {
            let thumb_url = self.thumbnail_url(photo).unwrap_or_default();
            let (width, height) = photo.effective_dimensions();
            views.push(PhotoView {
                id: photo.id.0.to_string(),
                file_name: photo.file_name.clone(),
                width,
                height,
                used: placed.contains(&photo.id),
                thumb_url,
                crop: photo.adjust.crop.map(CropRectView::from),
                rotation: photo.adjust.rotate.get(),
            });
        }

        ItemView {
            title: self.item.title.clone(),
            blocks,
            photos: views,
            slug: self.item.slug.to_string(),
            dirty: self.dirty,
        }
    }

    /// Writes the photo's thumbnail to the cache directory and returns a path for the asset
    /// protocol.
    ///
    /// A failure here is not worth failing a command over — a photo with no thumbnail still
    /// lists, still places, and still publishes. The view carries an empty URL and the UI shows
    /// a placeholder.
    fn thumbnail_url(&mut self, photo: &Photo) -> Option<String> {
        // Every exit here is logged. Swallowing the failure keeps the command working, which is
        // the intent; swallowing the *reason* leaves a photo with no picture and no way to find
        // out why, which is not.
        let Some(source) = self.bytes.get(&photo.id).cloned() else {
            tracing::warn!(
                photo = %photo.file_name,
                "the session holds no bytes for the photo, so it gets no thumbnail"
            );
            return None;
        };
        let bytes = match self.thumbnails.get_or_render(photo, &source) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(photo = %photo.file_name, %error, "the thumbnail could not be rendered");
                return None;
            }
        };

        // The adjustment hash is in the name, so a crop produces a new file rather than
        // overwriting one the webview may still be showing from its own cache.
        let name = format!("{}-{:016x}.jpg", photo.id.0, fingerprint(bytes));
        let path = self.thumbnail_dir.join(name);
        if !path.exists() {
            if let Err(error) = std::fs::create_dir_all(&self.thumbnail_dir)
                .and_then(|()| std::fs::write(&path, bytes))
            {
                tracing::warn!(
                    photo = %photo.file_name,
                    path = %path.display(),
                    %error,
                    "the thumbnail could not be written to the cache"
                );
                return None;
            }
        }
        Some(path.to_string_lossy().into_owned())
    }

    /// Looks a photo up from the string id the frontend uses.
    pub fn photo_id(&self, id: &str) -> CommandResult<PhotoId> {
        let parsed = id
            .parse::<u64>()
            .map(PhotoId)
            .map_err(|_| CommandError::new("invalid_photo", format!("`{id}` is not a photo id")))?;
        if self.item.photo(parsed).is_none() {
            return Err(CommandError::new(
                "invalid_photo",
                format!("the item holds no photo {parsed}"),
            ));
        }
        Ok(parsed)
    }

    /// Builds the item for preview, with the bytes this session already holds.
    ///
    /// `&mut` for the cache alone: the item is not changed here. What the editor sees is
    /// byte-identical to what [`build`](newsbuilder_core::build::build) would have produced,
    /// which is what keeps FR-022's "the preview is the export" true through the optimisation.
    pub fn build_preview(&mut self) -> Result<BuildOutput> {
        let context = BuildContext {
            public_base_url: Some(self.preview_base.clone()),
            slug: self.item.slug.clone(),
        };
        let output = build_with_cache(
            &self.item,
            &context,
            &SessionBytes(&self.bytes),
            &mut self.processed,
        )?;
        self.write_preview_photos(&output);
        Ok(output)
    }

    /// Puts the built photos where the fragment's `<img>` tags will look for them.
    ///
    /// Only what changed is written: [`ProcessCache`] keeps a photo's bytes identical across a
    /// rebuild, so after the first build a keystroke touches the disk not at all, which is what
    /// SC-008's budget leaves no room for.
    ///
    /// A failure here costs that photo its place in the preview and nothing else — the item
    /// still builds, still exports and still publishes, so it is not worth failing a command
    /// over. It is worth saying out loud.
    fn write_preview_photos(&mut self, output: &BuildOutput) {
        let folder = self.preview_dir.join(self.item.slug.to_string());
        for photo in &output.processed {
            let path = folder.join(&photo.file_name);
            let mark = fingerprint(&photo.bytes);
            if self.preview_written.get(&path) == Some(&mark) {
                continue;
            }
            match std::fs::create_dir_all(&folder)
                .and_then(|()| std::fs::write(&path, &photo.bytes))
            {
                Ok(()) => {
                    self.preview_written.insert(path, mark);
                }
                Err(error) => tracing::warn!(
                    photo = %photo.file_name,
                    path = %path.display(),
                    %error,
                    "the preview copy of a photo could not be written, so it will not show"
                ),
            }
        }
    }
}

/// A short stable name for a byte string, so a changed thumbnail gets a changed file name.
fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    // Hashing the whole image would cost more than rendering it; the ends plus the length
    // separate two thumbnails of the same photo under different adjustments, which is all this
    // has to do.
    bytes.iter().take(256).for_each(|b| b.hash(&mut hasher));
    bytes
        .iter()
        .rev()
        .take(256)
        .for_each(|b| b.hash(&mut hasher));
    hasher.finish()
}

/// Serves photo bytes from the session rather than from disk.
///
/// This is what keeps a rebuild off the filesystem entirely: the bytes were read once, when the
/// photo entered the item.
pub struct SessionBytes<'a>(pub &'a HashMap<PhotoId, Vec<u8>>);

impl PhotoBytesSource for SessionBytes<'_> {
    fn read(&self, photo: &Photo) -> Result<Vec<u8>> {
        if let Some(bytes) = self.0.get(&photo.id) {
            return Ok(bytes.clone());
        }
        match &photo.source {
            PhotoSource::Bytes(bytes) => Ok(bytes.to_vec()),
            PhotoSource::Path(path) => Err(Error::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "this photo's bytes are not in the session",
                ),
            }),
        }
    }
}

impl From<&Block> for BlockView {
    fn from(block: &Block) -> Self {
        match block {
            Block::Paragraph { text, kind } => Self::Paragraph {
                text: text.clone(),
                lead: *kind == ParagraphKind::Lead,
            },
            Block::Placement { photos, layout } => Self::Placement {
                photo_ids: photos.iter().map(|id| id.0.to_string()).collect(),
                layout: *layout,
            },
        }
    }
}

/// Turns the frontend's body back into blocks, refusing an id the item does not hold (INV-1).
pub fn blocks_from_input(item: &NewsItem, input: Vec<BlockInput>) -> CommandResult<Vec<Block>> {
    let mut blocks = Vec::with_capacity(input.len());
    for one in input {
        match one {
            BlockInput::Paragraph { text } => blocks.push(Block::paragraph(text)),
            BlockInput::Placement { photo_ids, layout } => {
                let mut photos = Vec::with_capacity(photo_ids.len());
                for id in &photo_ids {
                    let parsed = id.parse::<u64>().map(PhotoId).map_err(|_| {
                        CommandError::new("invalid_photo", format!("`{id}` is not a photo id"))
                    })?;
                    if item.photo(parsed).is_none() {
                        return Err(CommandError::new(
                            "invalid_photo",
                            format!("the item holds no photo {parsed}"),
                        ));
                    }
                    if !photos.contains(&parsed) {
                        photos.push(parsed);
                    }
                }
                // An emptied card is dropped rather than kept as a placement with no photos,
                // which INV-1 forbids and the renderer would have to guess about.
                if photos.is_empty() {
                    continue;
                }
                let layout = if photos.len() > 1 && !layout.holds_many() {
                    Layout::Row
                } else {
                    layout
                };
                blocks.push(Block::Placement { photos, layout });
            }
        }
    }
    Ok(blocks)
}

/// The directory thumbnails are written to, beneath the application's own cache.
pub fn thumbnail_dir(base: &Path) -> PathBuf {
    base.join("thumbnails")
}

/// The directory the preview's photos are written to, beneath the application's own cache.
pub fn preview_dir(base: &Path) -> PathBuf {
    base.join("preview")
}

/// The URL Tauri's asset protocol serves `path` at.
///
/// The same string `convertFileSrc` produces in the webview, and produced here for the same
/// reason it exists there: the two platforms disagree about the form, and the fragment carries
/// one fixed string. Everything but `encodeURIComponent`'s unreserved set is escaped, because
/// the protocol handler percent-decodes the whole path back out.
fn asset_url(path: &Path) -> String {
    use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

    const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'!')
        .remove(b'~')
        .remove(b'*')
        .remove(b'\'')
        .remove(b'(')
        .remove(b')');

    let path = path.to_string_lossy();
    let encoded = utf8_percent_encode(&path, COMPONENT);
    if cfg!(any(windows, target_os = "android")) {
        format!("http://asset.localhost/{encoded}")
    } else {
        format!("asset://localhost/{encoded}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    use newsbuilder_core::model::item::{Block, Layout, PhotoIntake, add_photos};
    use newsbuilder_core::model::photo::PhotoOrigin;
    use newsbuilder_core::model::server::Slug;
    use newsbuilder_core::publish::slug::slugify;

    use super::{SessionState, asset_url, preview_dir};

    /// A directory of this test's own, removed when the test that made it passes.
    fn scratch(name: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "newsbuilder-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        dir
    }

    fn fixture_photo() -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/inputs/marker-full-width/images/photo1.jpg");
        std::fs::read(&path).expect("the parity fixtures are checked in")
    }

    /// A session holding one photograph, placed full width so the build processes it.
    fn session_with_one_placed_photo(cache: &Path) -> SessionState {
        let mut state = SessionState::new(cache);
        state.item.title = "День Конституции".to_owned();
        state.item.slug = slugify(&state.item.title);

        let bytes = fixture_photo();
        let added = add_photos(
            &mut state.item,
            vec![PhotoIntake::from_bytes(
                "photo1.jpg",
                bytes.clone(),
                PhotoOrigin::Pasted,
            )],
        );
        let id = *added.ids.first().expect("the fixture is a readable JPEG");
        state.bytes.insert(id, bytes);
        state.item.body.push(Block::Placement {
            photos: vec![id],
            layout: Layout::FullWidth,
        });
        state
    }

    /// The bug this guards: the preview's `<img>` tags pointed at a `newsbuilder-preview://`
    /// scheme nothing served, so the pane was blank however well the build had gone.
    #[test]
    fn the_preview_points_at_photos_that_are_on_disk() {
        let cache = scratch("preview-urls");
        let mut state = session_with_one_placed_photo(&cache);

        let output = state.build_preview().expect("the item builds");

        assert!(
            !output.processed.is_empty(),
            "the placed photo should have been processed"
        );
        let base = format!("{}/{}/", asset_url(&preview_dir(&cache)), state.item.slug);
        for photo in &output.processed {
            let url = format!("{base}{}", photo.file_name);
            assert!(
                output.fragment.contains(&url),
                "the fragment should reference {url}, but holds:\n{}",
                output.fragment
            );
            let path = preview_dir(&cache)
                .join(state.item.slug.to_string())
                .join(&photo.file_name);
            assert!(path.exists(), "{} should have been written", path.display());
            assert_eq!(
                std::fs::read(&path).expect("the file just written"),
                photo.bytes,
                "the file on disk should be the bytes the build produced"
            );
        }

        std::fs::remove_dir_all(&cache).ok();
    }

    /// A crop reuses the photo's published name, so the file has to be rewritten rather than
    /// left as whatever the name meant last time.
    #[test]
    fn a_changed_photo_replaces_the_file_its_name_already_had() {
        let cache = scratch("preview-restale");
        let mut state = session_with_one_placed_photo(&cache);
        let first = state.build_preview().expect("the item builds");

        let name = first.processed[0].file_name.clone();
        let path = preview_dir(&cache)
            .join(state.item.slug.to_string())
            .join(&name);
        std::fs::write(&path, b"stale").expect("the file is there to overwrite");

        // A rebuild alone must not restore it — the bytes have not changed, so nothing is
        // written and the cache is doing its job.
        state.build_preview().expect("the item builds again");
        assert_eq!(std::fs::read(&path).expect("still there"), b"stale");

        // Cropping does change the bytes, and the same name now has to carry them.
        let id = state.item.photos[0].id;
        let photo = state.item.photo_mut(id).expect("the photo is there");
        newsbuilder_core::photo::set_crop(
            photo,
            Some(newsbuilder_core::model::photo::CropRect {
                x: 0,
                y: 0,
                width: 200,
                height: 200,
            }),
        )
        .expect("a crop inside the frame");

        let after = state.build_preview().expect("the cropped item builds");
        let cropped = after
            .processed
            .iter()
            .find(|p| p.file_name == name)
            .expect("the same published name");
        assert_eq!(
            std::fs::read(&path).expect("rewritten"),
            cropped.bytes,
            "the crop should have replaced the file its name already had"
        );

        std::fs::remove_dir_all(&cache).ok();
    }

    /// The bug this guards: `assetProtocol.scope` was `[]`, which is not "everything" but
    /// "nothing" — every thumbnail request was answered 403 and no photo ever appeared.
    #[test]
    fn the_asset_protocol_is_scoped_to_the_directories_the_photos_are_written_to() {
        let config: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"))
                .expect("the app's configuration"),
        )
        .expect("well-formed JSON");

        let scope = config["app"]["security"]["assetProtocol"]["scope"]
            .as_array()
            .expect("assetProtocol.scope is a list");
        let paths: Vec<&str> = scope.iter().filter_map(|entry| entry.as_str()).collect();

        for required in ["$APPCACHE/thumbnails/**", "$APPCACHE/preview/**"] {
            assert!(
                paths.contains(&required),
                "the webview cannot read {required}, so those images render as nothing; \
                 scope is {paths:?}"
            );
        }
    }

    /// The form the protocol handler decodes back into a path.
    #[test]
    fn an_asset_url_percent_encodes_the_whole_path() {
        let url = asset_url(Path::new(
            "/home/a b/.cache/org.newsbuilder.desktop/preview",
        ));
        let expected = if cfg!(any(windows, target_os = "android")) {
            "http://asset.localhost/"
        } else {
            "asset://localhost/"
        };
        assert_eq!(
            url,
            format!("{expected}%2Fhome%2Fa%20b%2F.cache%2Forg.newsbuilder.desktop%2Fpreview")
        );
    }

    /// A slug is a slug; this only guards the assumption the folder layout rests on.
    #[test]
    fn the_preview_folder_is_the_slug_the_publish_would_use() {
        let cache = scratch("preview-folder");
        let mut state = session_with_one_placed_photo(&cache);
        state.item.slug = Slug::parse("den-konstitutsii").expect("well formed");

        state.build_preview().expect("the item builds");

        assert!(preview_dir(&cache).join("den-konstitutsii").is_dir());
        std::fs::remove_dir_all(&cache).ok();
    }
}
