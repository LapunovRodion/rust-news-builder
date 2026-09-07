//! The photo pipeline: sniff, decode, orient, adjust, scale, encode.
//!
//! The source is only ever read (FR-015). Every editor action is recorded on
//! [`Adjustments`](crate::model::photo::Adjustments) and applied on the way to the output, so
//! reverting is assignment rather than restoration.
//!
//! Order matters: EXIF orientation, then the crop, then the editor's rotation, then scaling,
//! then the quality search. The crop sits before the rotation because a [`CropRect`] is defined
//! in oriented, pre-rotation pixel space — which is also the space
//! [`NewsItem::crops_are_within_bounds`](crate::model::item::NewsItem::crops_are_within_bounds)
//! checks INV-6 in.

pub mod encode;
pub mod frame;
pub mod orient;

use std::collections::HashMap;

use image::DynamicImage;

use crate::error::{Error, Result, Warning};
use crate::model::appearance::ImageBudget;
use crate::model::photo::{Adjustments, CropRect, Orientation, Photo, PhotoId, Quarters};

pub use encode::{Encoded, OutputFormat};
pub use frame::{AspectRatio, clamp_to_bounds, default_frame};

/// The formats FR-011 admits, by the extensions they arrive under.
///
/// Several names for one container is normal and not a special case: `.jfif`, `.jpe` and
/// `.jif` are what Windows, Outlook and browsers call a JPEG when they save one, and a photo
/// that arrives under any of them publishes as `.jpg` like every other JPEG — the container
/// decides the published name, not the name it came in with.
///
/// `pub` because the desktop's file dialog keeps the same list and a test holds the two
/// together; a list maintained twice by hand is how one of them goes stale.
pub const SUPPORTED_EXTENSIONS: [&str; 11] = [
    ".jpg", ".jpeg", ".jfif", ".jpe", ".jif", ".png", ".webp", ".gif", ".bmp", ".tif", ".tiff",
];

/// Image formats this build cannot decode, and what to do about each.
///
/// Kept apart from "not an image at all" because the two need different sentences. A `.heic`
/// from a phone is a photograph; telling its editor it is not an image format sends them
/// looking for a corrupt file rather than to the converter. Reading these needs libheif or
/// libdav1d — C libraries the build does not carry.
const UNDECODABLE_IMAGE_EXTENSIONS: [(&str, &str); 4] = [
    (".heic", "HEIC"),
    (".heif", "HEIF"),
    (".hif", "HEIF"),
    (".avif", "AVIF"),
];

/// What a file's bytes turn out to be, before any work is done on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    /// Dimensions with EXIF orientation already accounted for.
    pub dimensions: (u32, u32),
    /// The orientation the file asks for.
    pub orientation: Orientation,
    /// The container the bytes actually are, which need not match the extension.
    pub format: image::ImageFormat,
}

/// Whether a file name looks like an image this tool handles.
///
/// This is the by-name rejection FR-011 asks for: a `.pdf` is turned away before anything is
/// decoded, and the editor is told which file and why.
#[must_use]
pub fn extension_is_supported(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    SUPPORTED_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// The lowercased extension of a file name, including the dot, or `""` when it has none.
#[must_use]
pub fn extension_of(file_name: &str) -> String {
    match file_name.rfind('.') {
        Some(index) => file_name[index..].to_ascii_lowercase(),
        None => String::new(),
    }
}

/// Decides whether a file may enter an item, and names the file and the reason when it may not.
///
/// Two gates, in this order and for different reasons (FR-011):
///
/// 1. **The name.** A `.pdf` never reaches the decoder, so dropping a folder of mixed files
///    costs nothing and the refusal can say `notes.pdf` rather than "an unsupported file".
/// 2. **The bytes.** A file wearing a `.jpg` name that is not one is caught here. Names are a
///    hint; the container decides.
///
/// The refusal is a [`Warning`] rather than an [`Error`] because one bad file in a drop of
/// thirty must not cost the other twenty-nine (FR-034).
pub fn accept(file_name: &str, bytes: &[u8]) -> std::result::Result<Probe, Warning> {
    if !extension_is_supported(file_name) {
        let extension = extension_of(file_name);
        if let Some((_, format)) = UNDECODABLE_IMAGE_EXTENSIONS
            .iter()
            .find(|(ext, _)| extension == *ext)
        {
            return Err(Warning::PhotoSkipped {
                name: file_name.to_owned(),
                reason: format!(
                    "{format} is a format this tool cannot read; convert the photo to JPEG and \
                     add it again"
                ),
            });
        }
        let what = if extension.is_empty() {
            "it has no file extension".to_owned()
        } else {
            format!("`{extension}` is not an image format this tool handles")
        };
        return Err(Warning::PhotoSkipped {
            name: file_name.to_owned(),
            reason: format!(
                "{what}; the formats accepted are {}",
                SUPPORTED_EXTENSIONS.join(", ")
            ),
        });
    }

    probe(bytes, file_name).map_err(|error| Warning::PhotoSkipped {
        name: file_name.to_owned(),
        reason: error.to_string(),
    })
}

/// Reads a photo's shape without keeping the decoded pixels.
///
/// The container is sniffed from the bytes rather than trusted from the name: real Word
/// packages carry parts like `word/media/image2.tmp`, and a `.tmp` that is really a JPEG must
/// still publish correctly.
pub fn probe(bytes: &[u8], name: &str) -> Result<Probe> {
    let format = image::guess_format(bytes).map_err(|e| Error::UnsupportedFormat {
        name: name.to_owned(),
        detail: format!("the bytes are not an image this tool can read: {e}"),
    })?;

    let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let dimensions = reader
        .into_dimensions()
        .map_err(|e| Error::PhotoUnreadable {
            name: name.to_owned(),
            detail: format!("the image header could not be read: {e}"),
        })?;

    let orientation = orient::read_orientation(bytes);
    Ok(Probe {
        dimensions: orient::oriented_dimensions(dimensions, orientation),
        orientation,
        format,
    })
}

/// A photo, processed and ready to publish.
#[derive(Debug, Clone)]
pub struct ProcessedPhoto {
    /// Which photo this is.
    pub id: PhotoId,
    /// The name it publishes under, e.g. `den-konstitutsii-01.jpg`.
    pub file_name: String,
    /// The encoded bytes.
    pub bytes: Vec<u8>,
    /// Its final dimensions.
    pub dimensions: (u32, u32),
    /// The quality the search settled on, or `None` for a lossless format.
    pub quality: Option<u8>,
}

/// Decodes, applies every recorded transform, scales down, and encodes to the budget.
///
/// `stem` is the published name without its extension; the extension is decided by the format,
/// which is why a `.gif` comes back as `.jpg`.
pub fn process(
    photo: &Photo,
    source_bytes: &[u8],
    stem: &str,
    budget: &ImageBudget,
    warnings: &mut Vec<Warning>,
) -> Result<ProcessedPhoto> {
    let name = photo.file_name.as_str();
    let format = image::guess_format(source_bytes).map_err(|e| Error::UnsupportedFormat {
        name: name.to_owned(),
        detail: format!("the bytes are not an image this tool can read: {e}"),
    })?;

    let decoded = image::ImageReader::with_format(std::io::Cursor::new(source_bytes), format)
        .decode()
        .map_err(|e| Error::PhotoUnreadable {
            name: name.to_owned(),
            detail: format!("the image could not be decoded: {e}"),
        })?;

    let adjusted = apply_adjustments(decoded, photo.orientation, &photo.adjust);
    let scaled = scale_down(adjusted, budget.max_width);
    let dimensions = (scaled.width(), scaled.height());

    let output_format = OutputFormat::for_source(Some(format), &extension_of(name));
    let file_name = format!("{stem}{}", output_format.extension(&extension_of(name)));
    let encoded = encode::encode_to_budget(&scaled, output_format, budget, &file_name)?;

    if encoded.over_budget {
        let floor_quality = output_format
            .quality_bounds(budget)
            .map_or(0, |(_, floor)| floor);
        warnings.push(Warning::SizeBudgetUnreachable {
            name: file_name.clone(),
            floor_quality,
            achieved: encoded.bytes.len() as u64,
        });
    }

    Ok(ProcessedPhoto {
        id: photo.id,
        file_name,
        bytes: encoded.bytes,
        dimensions,
        quality: encoded.quality,
    })
}

/// EXIF orientation, then the crop, then the editor's quarter-turns.
///
/// The crop is expressed in oriented, pre-rotation space, so it is applied before the rotation —
/// reversing these two would crop a different region than the editor chose.
#[must_use]
pub fn apply_adjustments(
    image: DynamicImage,
    orientation: Orientation,
    adjust: &Adjustments,
) -> DynamicImage {
    let oriented = orient::apply(image, orientation);

    let cropped = match adjust.crop {
        Some(crop) => match clamp_to_bounds(crop, (oriented.width(), oriented.height())) {
            Some(safe) => oriented.crop_imm(safe.x, safe.y, safe.width, safe.height),
            None => oriented,
        },
        None => oriented,
    };

    match adjust.rotate.get() {
        1 => cropped.rotate90(),
        2 => cropped.rotate180(),
        3 => cropped.rotate270(),
        _ => cropped,
    }
}

/// Scales an image down so its **width** is at most `max_width`. Never scales up.
///
/// Width, not the longest edge: the reference tests `image.width > max_width`, so a tall
/// portrait narrower than the limit is left alone however tall it is.
/// contracts/html-output.md says "longest edge", which the reference does not do.
#[must_use]
pub fn scale_down(image: DynamicImage, max_width: u32) -> DynamicImage {
    if max_width == 0 || image.width() <= max_width {
        return image;
    }
    // The reference's arithmetic: int(height * (max_width / width)), floored, minimum 1.
    let height =
        (u64::from(image.height()) * u64::from(max_width) / u64::from(image.width())).max(1) as u32;
    image.resize_exact(max_width, height, image::imageops::FilterType::Lanczos3)
}

/// Records a crop, rejecting one that would fall outside the photo (INV-6).
///
/// `None` restores the full frame, which is how reverting works (FR-014).
pub fn set_crop(photo: &mut Photo, crop: Option<CropRect>) -> Result<()> {
    match crop {
        None => {
            photo.adjust.crop = None;
            Ok(())
        }
        Some(rect) => {
            if !rect.fits_within(photo.dimensions) {
                return Err(Error::UnsupportedFormat {
                    name: photo.file_name.clone(),
                    detail: format!(
                        "the crop {}x{} at ({}, {}) falls outside the {}x{} photo",
                        rect.width,
                        rect.height,
                        rect.x,
                        rect.y,
                        photo.dimensions.0,
                        photo.dimensions.1
                    ),
                });
            }
            photo.adjust.crop = Some(rect);
            Ok(())
        }
    }
}

/// Turns a photo by quarter-turns, in either direction.
///
/// The crop is dropped: it was expressed in the pre-rotation frame and would otherwise select
/// a different region than the editor chose.
pub fn rotate(photo: &mut Photo, quarters: i8) {
    if quarters.rem_euclid(4) == 0 {
        return;
    }
    photo.adjust.rotate = photo.adjust.rotate.turn(quarters);
    photo.adjust.crop = None;
}

/// Restores the photo to how it arrived (FR-014).
pub fn revert(photo: &mut Photo) {
    photo.adjust = Adjustments::NONE;
}

/// The crop that would frame this photo for a target shape, without applying it (FR-013).
///
/// `target` is the shape the editor picked on the view they can see, which is the photo after
/// its quarter-turns. The crop itself lives in the pre-rotation frame — that is what
/// [`CropRect`] means and what [`set_crop`] validates against — so a rotation that swaps the
/// axes transposes the *target*, not the dimensions. Swapping the dimensions instead produces a
/// rectangle shaped for a frame it will never be applied in, and `set_crop` rightly refuses it.
#[must_use]
pub fn suggest_crop(photo: &Photo, target: AspectRatio) -> Option<CropRect> {
    let target = if photo.adjust.rotate.swaps_axes() {
        target.transposed()
    } else {
        target
    };
    default_frame(photo.dimensions, target)
}

/// Thumbnails, keyed so a photo is only ever re-rendered when something about it changed.
///
/// The key is the photo's id plus a hash of everything that affects its appearance. Two edits
/// that cancel out land on the same key and reuse the cached bytes, which is what keeps a
/// thirty-photo rebuild inside the SC-008 budget.
#[derive(Debug, Default)]
pub struct ThumbnailCache {
    entries: HashMap<(PhotoId, u64), Vec<u8>>,
}

/// The longest edge a thumbnail is rendered at.
pub const THUMBNAIL_MAX_EDGE: u32 = 320;

impl ThumbnailCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many thumbnails are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Empties the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the photo's thumbnail, rendering it only if the adjustments changed.
    pub fn get_or_render(&mut self, photo: &Photo, source_bytes: &[u8]) -> Result<&[u8]> {
        let key = (photo.id, adjustment_key(photo));
        // Rendering only on a miss is the whole point of the cache; `entry` would render
        // every time to build the argument.
        if let std::collections::hash_map::Entry::Vacant(slot) = self.entries.entry(key) {
            slot.insert(render_thumbnail(photo, source_bytes)?);
        }
        // The insert above guarantees the entry, and `?` on a lookup keeps principle V.
        self.entries
            .get(&key)
            .map(Vec::as_slice)
            .ok_or_else(|| Error::PhotoUnreadable {
                name: photo.file_name.clone(),
                detail: "the thumbnail cache lost the entry it had just written".to_owned(),
            })
    }
}

/// A hash of everything that changes how a photo looks.
fn adjustment_key(photo: &Photo) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    photo.adjust.rotate.get().hash(&mut hasher);
    match photo.adjust.crop {
        Some(crop) => (crop.x, crop.y, crop.width, crop.height).hash(&mut hasher),
        None => u32::MAX.hash(&mut hasher),
    }
    (photo.orientation as u8).hash(&mut hasher);
    hasher.finish()
}

fn render_thumbnail(photo: &Photo, source_bytes: &[u8]) -> Result<Vec<u8>> {
    let name = photo.file_name.as_str();
    let format = image::guess_format(source_bytes).map_err(|e| Error::UnsupportedFormat {
        name: name.to_owned(),
        detail: format!("the bytes are not an image this tool can read: {e}"),
    })?;
    let decoded = image::ImageReader::with_format(std::io::Cursor::new(source_bytes), format)
        .decode()
        .map_err(|e| Error::PhotoUnreadable {
            name: name.to_owned(),
            detail: format!("the image could not be decoded: {e}"),
        })?;
    let adjusted = apply_adjustments(decoded, photo.orientation, &photo.adjust);
    let thumbnail = adjusted.thumbnail(THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);
    let mut bytes = Vec::new();
    let rgb = thumbnail.to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| Error::PhotoUnreadable {
            name: name.to_owned(),
            detail: format!("the thumbnail could not be encoded: {e}"),
        })?;
    Ok(bytes)
}

/// The published stem for the photo at `index` (1-based) of an item titled with `slug`.
///
/// The reference builds `f"{title_slug}-{idx:02d}"`, where `title_slug` comes from the
/// **title**, not from any `--news-slug` override. An override changes the folder, not the
/// file names.
#[must_use]
pub fn published_stem(title_slug: &str, index: usize) -> String {
    format!("{title_slug}-{index:02}")
}

/// Normalises quarter-turns coming from a frontend into the model's representation.
#[must_use]
pub fn quarters(turns: i8) -> Quarters {
    Quarters::new(turns)
}

#[cfg(test)]
mod tests {
    use super::{extension_is_supported, extension_of, published_stem, scale_down, set_crop};
    use crate::model::photo::{
        Adjustments, CropRect, NaturalKey, Orientation, Photo, PhotoId, PhotoOrigin, PhotoSource,
    };
    use image::{DynamicImage, RgbImage};
    use std::sync::Arc;

    fn photo(dims: (u32, u32)) -> Photo {
        Photo {
            id: PhotoId(1),
            origin: PhotoOrigin::Dropped,
            source: PhotoSource::Bytes(Arc::from(Vec::new().into_boxed_slice())),
            file_name: "photo1.jpg".to_owned(),
            natural_key: NaturalKey::new("photo1.jpg"),
            dimensions: dims,
            orientation: Orientation::Normal,
            adjust: Adjustments::NONE,
        }
    }

    #[test]
    fn the_formats_fr_011_lists_are_accepted() {
        for name in [
            "a.jpg", "a.JPEG", "a.jfif", "a.JFIF", "a.jpe", "a.jif", "a.png", "a.webp", "a.gif",
            "a.bmp", "a.tif", "a.tiff",
        ] {
            assert!(extension_is_supported(name), "{name}");
        }
    }

    #[test]
    fn other_files_are_rejected_by_name() {
        for name in [
            "notes.pdf",
            "video.mp4",
            "archive.zip",
            "photo",
            "photo.jpg.txt",
            "IMG_0009.heic",
            "shot.avif",
        ] {
            assert!(!extension_is_supported(name), "{name}");
        }
    }

    #[test]
    fn extensions_are_lowercased() {
        assert_eq!(extension_of("PHOTO.JPG"), ".jpg");
        assert_eq!(extension_of("no-extension"), "");
        assert_eq!(extension_of("a.b.c"), ".c");
    }

    #[test]
    fn published_stems_are_zero_padded_to_two_digits() {
        assert_eq!(published_stem("den-konstitutsii", 1), "den-konstitutsii-01");
        assert_eq!(
            published_stem("den-konstitutsii", 12),
            "den-konstitutsii-12"
        );
        // Past ninety-nine the reference simply stops padding rather than truncating.
        assert_eq!(published_stem("a", 100), "a-100");
    }

    #[test]
    fn scaling_never_enlarges() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(400, 300));
        let out = scale_down(image, 1600);
        assert_eq!((out.width(), out.height()), (400, 300));
    }

    #[test]
    fn scaling_uses_width_not_the_longest_edge() {
        // A tall portrait narrower than the limit is left alone, matching the reference.
        let image = DynamicImage::ImageRgb8(RgbImage::new(1000, 4000));
        let out = scale_down(image, 1600);
        assert_eq!((out.width(), out.height()), (1000, 4000));
    }

    #[test]
    fn scaling_down_keeps_the_aspect_ratio() {
        let image = DynamicImage::ImageRgb8(RgbImage::new(3200, 2400));
        let out = scale_down(image, 1600);
        assert_eq!((out.width(), out.height()), (1600, 1200));
    }

    #[test]
    fn a_crop_outside_the_photo_is_refused() {
        let mut subject = photo((600, 400));
        let result = set_crop(
            &mut subject,
            Some(CropRect {
                x: 500,
                y: 0,
                width: 200,
                height: 100,
            }),
        );
        assert!(result.is_err(), "INV-6 must not be violated by assignment");
        assert_eq!(
            subject.adjust.crop, None,
            "a refused crop must not be recorded"
        );
    }

    #[test]
    fn setting_no_crop_restores_the_full_frame() {
        let mut subject = photo((600, 400));
        set_crop(
            &mut subject,
            Some(CropRect {
                x: 10,
                y: 10,
                width: 100,
                height: 100,
            }),
        )
        .expect("a crop inside the photo is accepted");
        set_crop(&mut subject, None).expect("clearing always works");
        assert_eq!(subject.adjust, Adjustments::NONE);
    }

    #[test]
    fn rotating_drops_a_crop_taken_in_the_old_frame() {
        let mut subject = photo((600, 400));
        set_crop(
            &mut subject,
            Some(CropRect {
                x: 10,
                y: 10,
                width: 100,
                height: 100,
            }),
        )
        .expect("accepted");
        super::rotate(&mut subject, 1);
        assert_eq!(subject.adjust.crop, None);
        assert_eq!(subject.adjust.rotate.get(), 1);
    }

    #[test]
    fn rotating_by_a_full_turn_changes_nothing() {
        let mut subject = photo((600, 400));
        set_crop(
            &mut subject,
            Some(CropRect {
                x: 10,
                y: 10,
                width: 100,
                height: 100,
            }),
        )
        .expect("accepted");
        let before = subject.adjust;
        super::rotate(&mut subject, 4);
        assert_eq!(subject.adjust, before);
    }
}
