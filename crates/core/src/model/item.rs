//! The aggregate root and the flat body it holds.
//!
//! `body` is a single ordered sequence of paragraphs and placements, mirroring the reference's
//! `ParagraphBlock` / `ImageLayoutBlock` split. Keeping it flat is what makes rendering one
//! pass and makes "the placement sits at this point in the text" a position rather than a
//! relationship to maintain.

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::{Error, Result, Warning};
use crate::model::appearance::Appearance;
use crate::model::photo::{Adjustments, NaturalKey, Photo, PhotoId, PhotoOrigin, PhotoSource};
use crate::model::server::{ServerConfigRef, Slug};

/// A position in [`NewsItem::body`].
pub type BlockIndex = usize;

/// Which style slot a paragraph renders with.
///
/// The reference decides this positionally — the first paragraph it emits uses `lead`, every
/// later one uses `paragraph` — so [`NewsItem::normalise_paragraph_kinds`] keeps the stored
/// value agreeing with that rule after any edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphKind {
    /// The first paragraph of the body.
    Lead,
    /// Every later paragraph.
    Body,
}

/// How a placement's photos are laid out. One-to-one with the frozen marker language (FR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// `[image:N]` — one photo, full container width.
    FullWidth,
    /// `[images:N,M,...]` — photos side by side as equal flex children.
    Row,
    /// `[image-left:N]` — one photo floated left, text wrapping around it.
    FloatLeft,
    /// `[image-right:N]` — one photo floated right.
    FloatRight,
}

impl Layout {
    /// The marker keyword the reference uses for this layout.
    #[must_use]
    pub fn marker_keyword(self) -> &'static str {
        match self {
            Self::FullWidth => "image",
            Self::Row => "images",
            Self::FloatLeft => "image-left",
            Self::FloatRight => "image-right",
        }
    }

    /// Whether the layout floats, and so needs a clearing element after it.
    #[must_use]
    pub fn floats(self) -> bool {
        matches!(self, Self::FloatLeft | Self::FloatRight)
    }

    /// Whether the layout admits more than one photo.
    #[must_use]
    pub fn holds_many(self) -> bool {
        matches!(self, Self::Row)
    }
}

/// One element of the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// Running text.
    Paragraph {
        /// The paragraph's text, already normalised.
        text: String,
        /// Which style slot it renders with.
        kind: ParagraphKind,
    },
    /// Photos placed at this point in the text.
    Placement {
        /// The photos, in the order they appear.
        photos: Vec<PhotoId>,
        /// How they are laid out.
        layout: Layout,
    },
}

/// A placement on its own, apart from the body it sits in.
///
/// [`Block::Placement`] is a position; this is the value at it. Separating the two is what lets
/// [`crate::arrange::set_placement`] replace one card's contents without the caller having to
/// rebuild the surrounding body (FR-021).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The photos, in the order they appear.
    pub photos: Vec<PhotoId>,
    /// How they are laid out.
    pub layout: Layout,
}

impl Block {
    /// A body paragraph. [`NewsItem::normalise_paragraph_kinds`] promotes the first to a lead.
    #[must_use]
    pub fn paragraph(text: impl Into<String>) -> Self {
        Self::Paragraph {
            text: text.into(),
            kind: ParagraphKind::Body,
        }
    }

    /// A placement.
    #[must_use]
    pub fn placement(photos: Vec<PhotoId>, layout: Layout) -> Self {
        Self::Placement { photos, layout }
    }

    /// Whether this is a placement.
    #[must_use]
    pub fn is_placement(&self) -> bool {
        matches!(self, Self::Placement { .. })
    }
}

/// The format an item was imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    /// A Word document.
    Docx,
    /// Plain text.
    Text,
    /// Markdown.
    Markdown,
}

impl SourceFormat {
    /// Infers the format from a file name's extension.
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        let ext = name.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "docx" => Some(Self::Docx),
            "txt" => Some(Self::Text),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// An image lifted out of a Word package, with the position that becomes a placement (FR-003).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedMedia {
    /// The `r:embed` relationship id it was resolved through.
    pub rel_id: String,
    /// The package part it came from, e.g. `word/media/image1.png`.
    pub part_name: String,
    /// The image bytes.
    pub bytes: Arc<[u8]>,
    /// How many paragraphs preceded it in the document.
    pub after_paragraph: usize,
}

/// Provenance for an imported item; absent for items built from loose photos.
#[derive(Debug, Clone)]
pub struct SourceDocument {
    /// The file the editor opened.
    pub path: PathBuf,
    /// What it was.
    pub format: SourceFormat,
    /// Word only: the images and where they sat.
    pub embedded_media: Vec<EmbeddedMedia>,
}

/// One publishable story.
#[derive(Debug, Clone)]
pub struct NewsItem {
    /// The headline. Never repeated in `body` (FR-004).
    pub title: String,
    /// Paragraphs and placements, in order.
    pub body: Vec<Block>,
    /// The item's photos. Index order is the identity order editors see.
    pub photos: Vec<Photo>,
    /// Derived from `title`, overridable (FR-026).
    pub slug: Slug,
    /// Rendering and photo-budget settings.
    pub appearance: Appearance,
    /// The publishing target, once one is chosen.
    pub server: Option<ServerConfigRef>,
    /// Where the item came from, if it was imported.
    pub source: Option<SourceDocument>,
    /// Hands out the next [`PhotoId`]. Monotonic, so an id is never reused within an item.
    next_photo_id: u64,
}

impl Default for NewsItem {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsItem {
    /// An empty item with the built-in appearance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: String::new(),
            body: Vec::new(),
            photos: Vec::new(),
            slug: Slug::fallback(),
            appearance: Appearance::built_in(),
            server: None,
            source: None,
            next_photo_id: 1,
        }
    }

    /// Reserves the next photo id.
    pub fn mint_photo_id(&mut self) -> PhotoId {
        let id = PhotoId(self.next_photo_id);
        self.next_photo_id += 1;
        id
    }

    /// Ensures future ids come after `id`, for callers that assign one directly.
    ///
    /// Import needs this: a marker's index *is* the photo's id, so the ids are chosen rather
    /// than minted, and the counter has to be told where they got to (INV-3).
    pub fn bump_photo_id_past(&mut self, id: PhotoId) {
        self.next_photo_id = self.next_photo_id.max(id.0 + 1);
    }

    /// Looks a photo up by id.
    #[must_use]
    pub fn photo(&self, id: PhotoId) -> Option<&Photo> {
        self.photos.iter().find(|p| p.id == id)
    }

    /// Looks a photo up by id, mutably.
    pub fn photo_mut(&mut self, id: PhotoId) -> Option<&mut Photo> {
        self.photos.iter_mut().find(|p| p.id == id)
    }

    /// Every photo id any placement references, in body order, without duplicates.
    #[must_use]
    pub fn placed_photo_ids(&self) -> Vec<PhotoId> {
        let mut seen = Vec::new();
        for block in &self.body {
            if let Block::Placement { photos, .. } = block {
                for id in photos {
                    if !seen.contains(id) {
                        seen.push(*id);
                    }
                }
            }
        }
        seen
    }

    /// Photos no placement references. These are not uploaded (FR-034).
    #[must_use]
    pub fn unused_photo_ids(&self) -> Vec<PhotoId> {
        let placed = self.placed_photo_ids();
        self.photos
            .iter()
            .map(|p| p.id)
            .filter(|id| !placed.contains(id))
            .collect()
    }

    /// Restores the reference's positional lead rule: the first paragraph in the body renders
    /// with `lead`, every later one with `paragraph`. Call after any body edit.
    pub fn normalise_paragraph_kinds(&mut self) {
        let mut seen_first = false;
        for block in &mut self.body {
            if let Block::Paragraph { kind, .. } = block {
                *kind = if seen_first {
                    ParagraphKind::Body
                } else {
                    ParagraphKind::Lead
                };
                seen_first = true;
            }
        }
    }

    /// Whether every placement references a photo the item holds (INV-1).
    #[must_use]
    pub fn placements_resolve(&self) -> bool {
        self.body.iter().all(|block| match block {
            Block::Paragraph { .. } => true,
            Block::Placement { photos, .. } => {
                !photos.is_empty() && photos.iter().all(|id| self.photo(*id).is_some())
            }
        })
    }

    /// Whether every photo id is distinct (INV-3).
    #[must_use]
    pub fn photo_ids_are_unique(&self) -> bool {
        let mut seen: Vec<PhotoId> = Vec::with_capacity(self.photos.len());
        for photo in &self.photos {
            if seen.contains(&photo.id) {
                return false;
            }
            seen.push(photo.id);
        }
        true
    }

    /// Whether every file name is distinct (INV-5).
    #[must_use]
    pub fn file_names_are_unique(&self) -> bool {
        let mut seen: Vec<&str> = Vec::with_capacity(self.photos.len());
        for photo in &self.photos {
            if seen.contains(&photo.file_name.as_str()) {
                return false;
            }
            seen.push(&photo.file_name);
        }
        true
    }

    /// Whether every placement's photo count agrees with its layout (INV-4).
    #[must_use]
    pub fn layouts_match_counts(&self) -> bool {
        self.body.iter().all(|block| match block {
            Block::Paragraph { .. } => true,
            Block::Placement { photos, layout } => {
                if layout.holds_many() {
                    !photos.is_empty()
                } else {
                    photos.len() == 1
                }
            }
        })
    }

    /// Whether every recorded crop lies inside its photo's bounds (INV-6).
    #[must_use]
    pub fn crops_are_within_bounds(&self) -> bool {
        self.photos
            .iter()
            .all(|p| p.adjust.crop.is_none_or(|c| c.fits_within(p.dimensions)))
    }
}

// =============================================================================================
// Photo management (T083, FR-006 – FR-011)
// =============================================================================================

/// One photo offered to an item.
///
/// The three intake routes FR-006 to FR-008 describe — dropped, pasted, picked — differ only in
/// the [`PhotoSource`] and [`PhotoOrigin`] they construct. `bytes` is what the item is probed
/// with; a photo that lives on disk keeps a [`PhotoSource::Path`] so the build can re-read it
/// rather than holding a copy of every full-size image in memory.
#[derive(Debug, Clone)]
pub struct PhotoIntake {
    /// The name the photo arrives under, before uniqueness is enforced.
    pub file_name: String,
    /// Where its bytes come from from here on.
    pub source: PhotoSource,
    /// The bytes, for probing.
    pub bytes: Arc<[u8]>,
    /// How it entered the item.
    pub origin: PhotoOrigin,
}

impl PhotoIntake {
    /// A photo that arrived as bytes — from the clipboard, or out of a Word package.
    #[must_use]
    pub fn from_bytes(file_name: impl Into<String>, bytes: Vec<u8>, origin: PhotoOrigin) -> Self {
        let bytes: Arc<[u8]> = Arc::from(bytes.into_boxed_slice());
        Self {
            file_name: file_name.into(),
            source: PhotoSource::Bytes(Arc::clone(&bytes)),
            bytes,
            origin,
        }
    }

    /// A photo that lives on disk. The name is the path's last component.
    #[must_use]
    pub fn from_path(path: PathBuf, bytes: Vec<u8>, origin: PhotoOrigin) -> Self {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            file_name,
            source: PhotoSource::Path(path),
            bytes: Arc::from(bytes.into_boxed_slice()),
            origin,
        }
    }
}

/// What [`add_photos`] accepted, and what it turned away.
#[derive(Debug, Clone, Default)]
pub struct Added {
    /// The photos that entered the item, in the order they were offered.
    pub ids: Vec<PhotoId>,
    /// One per file that could not be used (FR-034).
    pub warnings: Vec<Warning>,
}

/// The single door every photo enters an item through (FR-006 – FR-008).
///
/// A file that is not an image is refused by name, and one whose bytes will not decode is
/// refused when it is probed — either way with a warning that names it, and without consuming
/// an id, so the published numbering carries no gap nobody can explain.
pub fn add_photos(item: &mut NewsItem, intake: Vec<PhotoIntake>) -> Added {
    let mut added = Added::default();
    for one in intake {
        let probe = match crate::photo::accept(&one.file_name, &one.bytes) {
            Ok(probe) => probe,
            Err(warning) => {
                added.warnings.push(warning);
                continue;
            }
        };

        let file_name = unique_file_name(item, one.file_name);
        let id = item.mint_photo_id();
        item.photos.push(Photo {
            id,
            origin: one.origin,
            source: one.source,
            natural_key: NaturalKey::new(&file_name),
            file_name,
            dimensions: probe.dimensions,
            orientation: probe.orientation,
            adjust: Adjustments::NONE,
        });
        added.ids.push(id);
    }
    added
}

/// Takes a photo out of the item, and out of whatever placement held it (FR-009).
///
/// A placement left holding one photo stops being a row; a placement left holding none is
/// dropped, with a warning naming the photo that took it (INV-1, INV-4).
pub fn remove_photo(item: &mut NewsItem, id: PhotoId) -> Vec<Warning> {
    let Some(position) = item.photos.iter().position(|photo| photo.id == id) else {
        return Vec::new();
    };
    let removed = item.photos.remove(position);

    let mut emptied = false;
    for block in &mut item.body {
        if let Block::Placement { photos, layout } = block {
            let before = photos.len();
            photos.retain(|held| *held != id);
            if photos.len() == before {
                continue;
            }
            if photos.is_empty() {
                emptied = true;
            } else if photos.len() == 1 && layout.holds_many() {
                *layout = Layout::FullWidth;
            }
        }
    }

    let mut warnings = Vec::new();
    if emptied {
        item.body.retain(|block| match block {
            Block::Placement { photos, .. } => !photos.is_empty(),
            Block::Paragraph { .. } => true,
        });
        warnings.push(Warning::PhotoSkipped {
            name: removed.file_name.clone(),
            reason: "removing it left its placement empty, so the placement went too".to_owned(),
        });
    }

    item.normalise_paragraph_kinds();
    warnings
}

/// Reorders the photo list (FR-009).
///
/// Placements bind to ids, so they follow without being touched — which is the whole reason
/// they hold an id rather than a position. Ids the item does not hold are ignored, and photos
/// the order does not mention keep their relative places behind the ones it does, so the
/// desktop list can hand back a selection rather than the whole list.
pub fn reorder_photos(item: &mut NewsItem, order: &[PhotoId]) {
    let mut remaining = std::mem::take(&mut item.photos);
    let mut ordered = Vec::with_capacity(remaining.len());
    for id in order {
        if let Some(position) = remaining.iter().position(|photo| photo.id == *id) {
            ordered.push(remaining.remove(position));
        }
    }
    ordered.append(&mut remaining);
    item.photos = ordered;
}

/// Renames a photo, refusing a name that is taken, empty, or not an image name.
///
/// The identity is the [`PhotoId`], so a rename cannot disturb a placement (INV-1).
pub fn rename_photo(item: &mut NewsItem, id: PhotoId, name: &str) -> Result<()> {
    let wanted = name.trim();

    if item.photo(id).is_none() {
        return Err(Error::PhotoNameUnusable {
            name: wanted.to_owned(),
            detail: format!("the item holds no photo {id}"),
        });
    }
    if wanted.is_empty() {
        return Err(Error::PhotoNameUnusable {
            name: String::new(),
            detail: "a photo needs a name".to_owned(),
        });
    }
    if wanted.contains('/') || wanted.contains('\\') {
        return Err(Error::PhotoNameUnusable {
            name: wanted.to_owned(),
            detail: "a photo name is a file name, not a path".to_owned(),
        });
    }
    if !crate::photo::extension_is_supported(wanted) {
        return Err(Error::PhotoNameUnusable {
            name: wanted.to_owned(),
            detail: "it does not end in one of the image extensions this tool handles".to_owned(),
        });
    }
    if item
        .photos
        .iter()
        .any(|photo| photo.id != id && photo.file_name == wanted)
    {
        return Err(Error::PhotoNameUnusable {
            name: wanted.to_owned(),
            detail: "another photo in this item already has that name".to_owned(),
        });
    }

    if let Some(photo) = item.photo_mut(id) {
        photo.file_name = wanted.to_owned();
        photo.natural_key = NaturalKey::new(wanted);
    }
    Ok(())
}

/// Appends a numeric suffix until the name is free (INV-5).
pub(crate) fn unique_file_name(item: &NewsItem, wanted: String) -> String {
    if !item.photos.iter().any(|photo| photo.file_name == wanted) {
        return wanted;
    }
    let (stem, extension) = match wanted.rsplit_once('.') {
        Some((stem, extension)) => (stem.to_owned(), format!(".{extension}")),
        None => (wanted.clone(), String::new()),
    };
    for n in 2u32.. {
        let candidate = format!("{stem}-{n}{extension}");
        if !item.photos.iter().any(|photo| photo.file_name == candidate) {
            return candidate;
        }
    }
    wanted
}

// =============================================================================================
// Placement editing (T113, FR-018a – FR-018d)
// =============================================================================================

/// Puts a placement at a point in the body — the cursor, or where a drag was dropped (FR-018a).
///
/// Several photos under a single-photo layout become a row rather than a refusal: a
/// multi-selection dropped into the text plainly means a row (INV-4). A row of one is left
/// alone, because the reference admits one too (deviation D-12).
pub fn insert_placement(
    item: &mut NewsItem,
    at: BlockIndex,
    photos: Vec<PhotoId>,
    layout: Layout,
) -> Result<()> {
    if at > item.body.len() {
        return Err(Error::InvalidPlacement {
            at,
            detail: format!(
                "the body holds {} blocks, so {at} is not a point in it",
                item.body.len()
            ),
        });
    }

    let photos = resolved_photos(item, at, photos)?;
    let layout = if photos.len() > 1 && !layout.holds_many() {
        Layout::Row
    } else {
        layout
    };

    item.body.insert(at, Block::Placement { photos, layout });
    item.normalise_paragraph_kinds();
    Ok(())
}

/// Moves a placement to another point in the text, photos and layout unchanged (FR-018d).
///
/// `to` is the position in the body the card should end up at, counted after it has left the
/// place it was — which is what dragging a card feels like.
pub fn move_placement(item: &mut NewsItem, from: BlockIndex, to: BlockIndex) -> Result<()> {
    expect_placement(item, from)?;
    if to >= item.body.len() {
        return Err(Error::InvalidPlacement {
            at: to,
            detail: format!(
                "the body holds {} blocks, so {to} is not a point to move to",
                item.body.len()
            ),
        });
    }

    let block = item.body.remove(from);
    item.body.insert(to, block);
    item.normalise_paragraph_kinds();
    Ok(())
}

/// Takes a placement out of the body. The photos stay in the item (FR-018b).
pub fn remove_placement(item: &mut NewsItem, at: BlockIndex) -> Result<()> {
    expect_placement(item, at)?;
    item.body.remove(at);
    item.normalise_paragraph_kinds();
    Ok(())
}

/// Drops another photo onto a card, which makes it a row (FR-018c).
pub fn add_to_placement(item: &mut NewsItem, at: BlockIndex, photo: PhotoId) -> Result<()> {
    expect_placement(item, at)?;
    if item.photo(photo).is_none() {
        return Err(Error::InvalidPlacement {
            at,
            detail: format!("the item holds no photo {photo}"),
        });
    }

    if let Some(Block::Placement { photos, layout }) = item.body.get_mut(at) {
        if photos.contains(&photo) {
            return Ok(());
        }
        photos.push(photo);
        if !layout.holds_many() {
            *layout = Layout::Row;
        }
    }
    Ok(())
}

/// Changes a card's layout (FR-018b).
///
/// A card holding several photos cannot take a single-photo layout (INV-4). The control offers
/// only the layouts that fit; this refuses the rest anyway, so a frontend that forgets cannot
/// leave the item in a state the renderer would have to guess about.
pub fn set_layout(item: &mut NewsItem, at: BlockIndex, layout: Layout) -> Result<()> {
    expect_placement(item, at)?;
    let held = match item.body.get(at) {
        Some(Block::Placement { photos, .. }) => photos.len(),
        _ => 0,
    };
    if held > 1 && !layout.holds_many() {
        return Err(Error::InvalidPlacement {
            at,
            detail: format!(
                "it holds {held} photos, and `{}` takes exactly one",
                layout.marker_keyword()
            ),
        });
    }
    if let Some(Block::Placement { layout: slot, .. }) = item.body.get_mut(at) {
        *slot = layout;
    }
    Ok(())
}

/// Refuses a block index that is not a placement.
fn expect_placement(item: &NewsItem, at: BlockIndex) -> Result<()> {
    match item.body.get(at) {
        Some(Block::Placement { .. }) => Ok(()),
        Some(Block::Paragraph { .. }) => Err(Error::InvalidPlacement {
            at,
            detail: "it is a paragraph, not a placement".to_owned(),
        }),
        None => Err(Error::InvalidPlacement {
            at,
            detail: format!("the body holds {} blocks", item.body.len()),
        }),
    }
}

/// Drops repeats and refuses a photo the item does not hold (INV-1).
fn resolved_photos(item: &NewsItem, at: BlockIndex, photos: Vec<PhotoId>) -> Result<Vec<PhotoId>> {
    let mut unique: Vec<PhotoId> = Vec::with_capacity(photos.len());
    for id in photos {
        if item.photo(id).is_none() {
            return Err(Error::InvalidPlacement {
                at,
                detail: format!("the item holds no photo {id}"),
            });
        }
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.is_empty() {
        return Err(Error::InvalidPlacement {
            at,
            detail: "a placement must hold at least one photo".to_owned(),
        });
    }
    Ok(unique)
}
