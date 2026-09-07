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
    /// A session holding an empty item.
    pub fn new(thumbnail_dir: PathBuf) -> Self {
        Self {
            item: NewsItem::new(),
            bytes: HashMap::new(),
            thumbnails: ThumbnailCache::new(),
            processed: ProcessCache::new(),
            thumbnail_dir,
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
        // image show under a re-used id.
        let _ = std::fs::remove_dir_all(&self.thumbnail_dir);
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
        let source = self.bytes.get(&photo.id)?.clone();
        let bytes = self.thumbnails.get_or_render(photo, &source).ok()?;

        // The adjustment hash is in the name, so a crop produces a new file rather than
        // overwriting one the webview may still be showing from its own cache.
        let name = format!("{}-{:016x}.jpg", photo.id.0, fingerprint(bytes));
        let path = self.thumbnail_dir.join(name);
        if !path.exists() {
            std::fs::create_dir_all(&self.thumbnail_dir).ok()?;
            std::fs::write(&path, bytes).ok()?;
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
        let context = BuildContext::preview(self.item.slug.clone());
        build_with_cache(
            &self.item,
            &context,
            &SessionBytes(&self.bytes),
            &mut self.processed,
        )
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
