//! Photos and the adjustments recorded against them.
//!
//! Adjustments are never applied to the source (FR-015): a [`Photo`] holds a
//! [`PhotoSource`] it only ever reads, and an [`Adjustments`] describing what the editor asked
//! for. Reverting is assignment of [`Adjustments::NONE`] — the original bytes survive because
//! they were never overwritten.

use std::path::PathBuf;
use std::sync::Arc;

/// A photo's identity within an item. Stable across reordering and renaming.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct PhotoId(pub u64);

impl std::fmt::Display for PhotoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// How a photo entered the item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotoOrigin {
    /// Extracted from a Word document, at the given position in document order (FR-002).
    Embedded {
        /// Zero-based position among the document's images.
        doc_order: usize,
    },
    /// Dropped onto the window (FR-006).
    Dropped,
    /// Pasted from the clipboard (FR-007).
    Pasted,
    /// Chosen through a file dialog (FR-008).
    Picked {
        /// Where it was chosen from.
        path: PathBuf,
    },
}

/// Where a photo's bytes come from.
///
/// Clipboard images have no path, which is why this is not simply a `PathBuf`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotoSource {
    /// A file on disk, opened read-only and never written (FR-015).
    Path(PathBuf),
    /// Bytes held in memory.
    Bytes(Arc<[u8]>),
}

/// EXIF orientation (FR-016), as the eight values the tag defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    /// No transform.
    #[default]
    Normal,
    /// Mirrored horizontally.
    FlipHorizontal,
    /// Rotated 180°.
    Rotate180,
    /// Mirrored vertically.
    FlipVertical,
    /// Transposed.
    Transpose,
    /// Rotated 90° clockwise.
    Rotate90,
    /// Transversed.
    Transverse,
    /// Rotated 270° clockwise.
    Rotate270,
}

impl Orientation {
    /// Reads the value the EXIF tag carries. Anything outside 1..=8 is [`Self::Normal`].
    #[must_use]
    pub fn from_exif(value: u16) -> Self {
        match value {
            2 => Self::FlipHorizontal,
            3 => Self::Rotate180,
            4 => Self::FlipVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => Self::Normal,
        }
    }

    /// Whether applying this orientation swaps width and height.
    #[must_use]
    pub fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270
        )
    }
}

/// A crop rectangle in oriented-image pixel space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CropRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width in pixels; always at least 1.
    pub width: u32,
    /// Height in pixels; always at least 1.
    pub height: u32,
}

impl CropRect {
    /// Whether the rectangle lies wholly inside an image of the given size (INV-6).
    #[must_use]
    pub fn fits_within(&self, dims: (u32, u32)) -> bool {
        let (w, h) = dims;
        self.width > 0
            && self.height > 0
            && self.x.saturating_add(self.width) <= w
            && self.y.saturating_add(self.height) <= h
    }
}

/// Quarter-turns applied by the editor, normalised to 0..=3 (FR-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Quarters(u8);

impl Quarters {
    /// No rotation.
    pub const NONE: Self = Self(0);

    /// Normalises any number of quarter-turns, in either direction, into 0..=3.
    #[must_use]
    pub fn new(turns: i8) -> Self {
        Self((turns.rem_euclid(4)) as u8)
    }

    /// The number of clockwise quarter-turns, 0..=3.
    #[must_use]
    pub fn get(self) -> u8 {
        self.0
    }

    /// Adds quarter-turns, wrapping.
    #[must_use]
    pub fn turn(self, by: i8) -> Self {
        Self::new(self.0 as i8 + by)
    }

    /// Whether applying this rotation swaps width and height.
    #[must_use]
    pub fn swaps_axes(self) -> bool {
        self.0 % 2 == 1
    }
}

/// What the editor changed about a photo. Reversible by construction (FR-014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Adjustments {
    /// The crop, in oriented-image pixel space. `None` is the full frame.
    pub crop: Option<CropRect>,
    /// Editor-applied quarter-turns, on top of EXIF orientation.
    pub rotate: Quarters,
}

impl Adjustments {
    /// The photo publishes as it arrived.
    pub const NONE: Self = Self {
        crop: None,
        rotate: Quarters::NONE,
    };

    /// Whether anything at all was changed.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

/// One component of a natural-sort key.
///
/// Numbers sort before text at the same position. Python's reference implementation raises a
/// `TypeError` on that comparison; defining it keeps the ordering total and locale-independent
/// as constitution principle IV requires, and it is unreachable for the names the reference
/// actually orders.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NaturalPart {
    /// A run of digits, compared numerically.
    Number(u128),
    /// Anything else, lowercased and compared bytewise.
    Text(String),
}

/// A precomputed natural-sort key, reproducing the reference's `natural_sort_key`.
///
/// The reference splits the file name on runs of digits, keeping the separators:
/// `re.split(r"(\d+)", name)`. Digit runs become integers, everything else is lowercased.
/// That is what makes `photo2.jpg` precede `photo10.jpg`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NaturalKey(pub Vec<NaturalPart>);

impl NaturalKey {
    /// Builds the key for a file name.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let mut parts = Vec::new();
        let chars: Vec<char> = name.chars().collect();
        let mut idx = 0usize;
        // `re.split` with a capturing group alternates non-matching and matching pieces,
        // emitting empty strings at the boundaries. Reproduce that exactly, empties included:
        // it is what keeps positions aligned between two keys being compared.
        loop {
            let start = idx;
            while idx < chars.len() && !chars[idx].is_ascii_digit() {
                idx += 1;
            }
            parts.push(NaturalPart::Text(
                chars[start..idx].iter().collect::<String>().to_lowercase(),
            ));
            if idx >= chars.len() {
                break;
            }
            let digit_start = idx;
            while idx < chars.len() && chars[idx].is_ascii_digit() {
                idx += 1;
            }
            let digits: String = chars[digit_start..idx].iter().collect();
            // Python integers are unbounded; u128 covers every realistic file name, and a run
            // too long to parse falls back to text rather than panicking.
            match digits.parse::<u128>() {
                Ok(value) => parts.push(NaturalPart::Number(value)),
                Err(_) => parts.push(NaturalPart::Text(digits)),
            }
        }
        Self(parts)
    }
}

/// One image belonging to an item.
#[derive(Debug, Clone)]
pub struct Photo {
    /// Stable identity for the item's lifetime.
    pub id: PhotoId,
    /// How it entered the item.
    pub origin: PhotoOrigin,
    /// Where its bytes come from.
    pub source: PhotoSource,
    /// The name it was added under; editable (FR-009). Unique within the item (INV-5).
    pub file_name: String,
    /// Precomputed ordering key.
    pub natural_key: NaturalKey,
    /// Dimensions after EXIF orientation is applied.
    pub dimensions: (u32, u32),
    /// The orientation read from EXIF (FR-016).
    pub orientation: Orientation,
    /// What the editor changed (FR-014).
    pub adjust: Adjustments,
}

impl Photo {
    /// The dimensions the photo publishes at, before scaling: oriented, rotated, and cropped.
    #[must_use]
    pub fn effective_dimensions(&self) -> (u32, u32) {
        let (w, h) = self
            .adjust
            .crop
            .map_or(self.dimensions, |c| (c.width, c.height));
        if self.adjust.rotate.swaps_axes() {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// Whether the photo is taller than it is wide, after every recorded transform.
    #[must_use]
    pub fn is_portrait(&self) -> bool {
        let (w, h) = self.effective_dimensions();
        h > w
    }
}

#[cfg(test)]
mod tests {
    use super::{NaturalKey, NaturalPart, Quarters};

    fn key(name: &str) -> NaturalKey {
        NaturalKey::new(name)
    }

    #[test]
    fn digits_split_out_and_compare_numerically() {
        // The whole point of the rule: photo2 precedes photo10, which bytewise it would not.
        assert!(key("photo2.jpg") < key("photo10.jpg"));
        assert!(key("photo9.jpg") < key("photo10.jpg"));
    }

    #[test]
    fn shape_matches_the_reference_split() {
        // re.split(r"(\d+)", "photo10.jpg") == ['photo', '10', '.jpg']
        assert_eq!(
            key("photo10.jpg").0,
            vec![
                NaturalPart::Text("photo".into()),
                NaturalPart::Number(10),
                NaturalPart::Text(".jpg".into()),
            ]
        );
    }

    #[test]
    fn a_leading_digit_run_produces_an_empty_first_part() {
        // re.split(r"(\d+)", "10.jpg") == ['', '10', '.jpg']
        assert_eq!(
            key("10.jpg").0,
            vec![
                NaturalPart::Text(String::new()),
                NaturalPart::Number(10),
                NaturalPart::Text(".jpg".into()),
            ]
        );
    }

    #[test]
    fn comparison_is_case_insensitive_like_the_reference() {
        assert_eq!(key("Photo1.JPG"), key("photo1.jpg"));
    }

    #[test]
    fn ordering_is_total_even_where_python_would_raise() {
        // 'a1' vs 'aa': Number vs Text at position 1. Python raises; we define it.
        let a = key("a1");
        let b = key("aa");
        assert!(a < b || b < a);
    }

    #[test]
    fn quarters_normalise_in_both_directions() {
        assert_eq!(Quarters::new(-1).get(), 3);
        assert_eq!(Quarters::new(5).get(), 1);
        assert_eq!(Quarters::new(4).get(), 0);
        assert_eq!(Quarters::NONE.turn(-1).get(), 3);
    }

    #[test]
    fn odd_quarter_turns_swap_axes() {
        assert!(Quarters::new(1).swaps_axes());
        assert!(!Quarters::new(2).swaps_axes());
    }
}
