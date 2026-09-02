//! The aggregate root and the flat body it holds.
//!
//! `body` is a single ordered sequence of paragraphs and placements, mirroring the reference's
//! `ParagraphBlock` / `ImageLayoutBlock` split. Keeping it flat is what makes rendering one
//! pass and makes "the placement sits at this point in the text" a position rather than a
//! relationship to maintain.

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::appearance::Appearance;
use crate::model::photo::{Photo, PhotoId};
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
