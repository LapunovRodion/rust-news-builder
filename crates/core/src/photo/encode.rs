//! Re-encoding to the byte budget (FR-028).
//!
//! Meeting the budget is a search, not a calculation: encode, measure, step the quality down,
//! repeat. This reproduces the reference's ladder exactly — from `quality` down to
//! `min_quality` in steps of five, stopping at the first result that fits or at the floor.
//!
//! # Parity limit
//!
//! The *bytes* this module produces are not identical to the reference's. Pillow wraps
//! libjpeg-turbo and libwebp with its own defaults; the `image` crate's encoders are a
//! different implementation. No amount of care makes two encoders agree bit for bit.
//!
//! What parity does hold for, and what the goldens assert: the chosen format, the published
//! file name and extension, the output dimensions, the quality ladder, and the rule that
//! decides when to stop. Deviation D-10 records this.

use image::{DynamicImage, ImageEncoder};

use crate::error::{Error, Result};
use crate::model::appearance::ImageBudget;

/// The container a processed photo is written into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Lossy, quality-controlled.
    Jpeg,
    /// Lossless; the quality ladder does not apply.
    Png,
    /// Lossy, quality-controlled. Needs the `webp-lossy` feature.
    Webp,
}

impl OutputFormat {
    /// The extension the reference publishes this format under.
    ///
    /// `processed_extension` in the reference maps JPEG to `.jpg` and WEBP to `.webp`, and
    /// otherwise keeps the source suffix — so a PNG stays `.png`.
    #[must_use]
    pub fn extension(self, source_extension: &str) -> String {
        match self {
            Self::Jpeg => ".jpg".to_owned(),
            Self::Webp => ".webp".to_owned(),
            Self::Png => {
                let lower = source_extension.to_ascii_lowercase();
                if lower.is_empty() {
                    ".png".to_owned()
                } else {
                    lower
                }
            }
        }
    }

    /// Reproduces the reference's format decision.
    ///
    /// JPEG, PNG and WebP sources keep their container. Everything else — GIF, BMP, TIFF —
    /// becomes a JPEG, which is why `photo4.gif` publishes as `formats-04.jpg`.
    #[must_use]
    pub fn for_source(decoded: Option<image::ImageFormat>, source_extension: &str) -> Self {
        let from_decoded = decoded.and_then(|format| match format {
            image::ImageFormat::Jpeg => Some(Self::Jpeg),
            image::ImageFormat::Png => Some(Self::Png),
            image::ImageFormat::WebP => Some(Self::Webp),
            _ => None,
        });
        from_decoded.unwrap_or_else(|| match source_extension.to_ascii_lowercase().as_str() {
            ".jpg" | ".jpeg" => Self::Jpeg,
            ".png" => Self::Png,
            ".webp" => Self::Webp,
            _ => Self::Jpeg,
        })
    }

    /// The starting and floor quality this format uses, or `None` when it is lossless.
    #[must_use]
    pub fn quality_bounds(self, budget: &ImageBudget) -> Option<(u8, u8)> {
        match self {
            Self::Jpeg => Some((budget.jpeg_quality, budget.jpeg_min_quality)),
            Self::Webp => Some((budget.webp_quality, budget.webp_min_quality)),
            Self::Png => None,
        }
    }
}

/// The result of encoding one photo.
#[derive(Debug, Clone)]
pub struct Encoded {
    /// The encoded bytes.
    pub bytes: Vec<u8>,
    /// What they are.
    pub format: OutputFormat,
    /// The quality the search settled on, or `None` for a lossless format.
    pub quality: Option<u8>,
    /// How many encodes it took. The goldens record this, so a ladder that silently changed
    /// shape would be caught.
    pub attempts: usize,
    /// Whether the floor was reached with the photo still over budget (FR-028).
    pub over_budget: bool,
}

/// The reference's quality ladder: from `start` down to `floor` in steps of five.
///
/// `range(85, 49, -5)` yields 85, 80, ... 50. The floor is always attempted, and a floor above
/// the start yields the start alone rather than nothing.
#[must_use]
pub fn quality_ladder(start: u8, floor: u8) -> Vec<u8> {
    if start < floor {
        return vec![start];
    }
    let mut ladder = Vec::new();
    let mut value = start;
    loop {
        ladder.push(value);
        match value.checked_sub(5) {
            Some(next) if next >= floor => value = next,
            _ => break,
        }
    }
    ladder
}

/// Encodes an image, stepping quality down until it fits `max_bytes` or the floor is reached.
///
/// Reaching the floor over budget is not a failure: the photo is returned with `over_budget`
/// set, so the rest of the item still builds (FR-034). `name` is carried only so a caller can
/// name the offending photo (SC-007).
pub fn encode_to_budget(
    image: &DynamicImage,
    format: OutputFormat,
    budget: &ImageBudget,
    name: &str,
) -> Result<Encoded> {
    let Some((start, floor)) = format.quality_bounds(budget) else {
        // Lossless: one encode, and the budget is whatever it turns out to be.
        let bytes = encode_once(image, format, None, name)?;
        let over_budget = bytes.len() as u64 > budget.max_bytes;
        return Ok(Encoded {
            bytes,
            format,
            quality: None,
            attempts: 1,
            over_budget,
        });
    };

    let ladder = quality_ladder(start, floor);
    let mut last: Option<(Vec<u8>, u8)> = None;
    for (index, quality) in ladder.iter().copied().enumerate() {
        let bytes = encode_once(image, format, Some(quality), name)?;
        let fits = bytes.len() as u64 <= budget.max_bytes;
        let is_last = index + 1 == ladder.len();
        if fits || is_last {
            return Ok(Encoded {
                over_budget: !fits,
                bytes,
                format,
                quality: Some(quality),
                attempts: index + 1,
            });
        }
        last = Some((bytes, quality));
    }

    // Unreachable: the loop above always returns on its final iteration, because `is_last`
    // is true there. Returning the last encode rather than panicking keeps principle V.
    let (bytes, quality) = last.ok_or_else(|| Error::SizeBudgetUnreachable {
        name: name.to_owned(),
        floor_quality: floor,
        achieved: 0,
    })?;
    let over_budget = bytes.len() as u64 > budget.max_bytes;
    Ok(Encoded {
        bytes,
        format,
        quality: Some(quality),
        attempts: ladder.len(),
        over_budget,
    })
}

fn encode_once(
    image: &DynamicImage,
    format: OutputFormat,
    quality: Option<u8>,
    name: &str,
) -> Result<Vec<u8>> {
    let unreadable = |detail: String| Error::PhotoUnreadable {
        name: name.to_owned(),
        detail,
    };

    match format {
        OutputFormat::Jpeg => {
            // JPEG has no alpha channel; the reference converts to RGB for the same reason.
            let rgb = image.to_rgb8();
            let mut bytes = Vec::new();
            let quality = quality.unwrap_or(85).clamp(1, 100);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| unreadable(format!("JPEG encoding failed: {e}")))?;
            Ok(bytes)
        }
        OutputFormat::Png => {
            let rgba = image.to_rgba8();
            let mut bytes = Vec::new();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut bytes,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Adaptive,
            )
            .write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| unreadable(format!("PNG encoding failed: {e}")))?;
            Ok(bytes)
        }
        OutputFormat::Webp => encode_webp(image, quality.unwrap_or(85), name),
    }
}

#[cfg(feature = "webp-lossy")]
fn encode_webp(image: &DynamicImage, quality: u8, _name: &str) -> Result<Vec<u8>> {
    let rgba = image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    Ok(encoder.encode(f32::from(quality.clamp(1, 100))).to_vec())
}

#[cfg(not(feature = "webp-lossy"))]
fn encode_webp(_image: &DynamicImage, _quality: u8, name: &str) -> Result<Vec<u8>> {
    // The pure-Rust WebP encoder is lossless only, so it cannot meet a byte budget. Refusing
    // clearly beats publishing a photo that silently ignores the budget (research R4).
    Err(Error::UnsupportedFormat {
        name: name.to_owned(),
        detail: "lossy WebP needs the `webp-lossy` cargo feature, which is off in this build"
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{OutputFormat, encode_to_budget, quality_ladder};
    use crate::model::appearance::ImageBudget;
    use image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn the_ladder_matches_the_reference_range() {
        // range(85, 49, -5)
        assert_eq!(quality_ladder(85, 50), vec![85, 80, 75, 70, 65, 60, 55, 50]);
    }

    #[test]
    fn a_ladder_that_does_not_divide_evenly_still_stops_at_or_above_the_floor() {
        // range(85, 51, -5) -> 85..55; 52 is never reached, matching Python.
        assert_eq!(quality_ladder(85, 52), vec![85, 80, 75, 70, 65, 60, 55]);
    }

    #[test]
    fn equal_bounds_give_one_rung() {
        assert_eq!(quality_ladder(85, 85), vec![85]);
    }

    #[test]
    fn a_floor_above_the_start_still_yields_an_attempt() {
        assert_eq!(quality_ladder(50, 85), vec![50]);
    }

    fn noisy(width: u32, height: u32) -> DynamicImage {
        // Noise so the encoder cannot compress its way under any budget for free.
        let mut image = RgbImage::new(width, height);
        let mut state: u32 = 0x1234_5678;
        for pixel in image.pixels_mut() {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *pixel = Rgb([(state >> 16) as u8, (state >> 8) as u8, state as u8]);
        }
        DynamicImage::ImageRgb8(image)
    }

    #[test]
    fn a_small_photo_encodes_once_at_the_starting_quality() {
        let budget = ImageBudget::built_in();
        let encoded = encode_to_budget(&noisy(60, 40), OutputFormat::Jpeg, &budget, "small.jpg")
            .expect("a tiny image encodes");
        assert_eq!(encoded.attempts, 1);
        assert_eq!(encoded.quality, Some(85));
        assert!(!encoded.over_budget);
    }

    #[test]
    fn an_oversized_photo_steps_down_the_ladder() {
        let budget = ImageBudget {
            max_bytes: 20_000,
            ..ImageBudget::built_in()
        };
        let encoded = encode_to_budget(&noisy(900, 700), OutputFormat::Jpeg, &budget, "big.jpg")
            .expect("encodes");
        assert!(
            encoded.attempts > 1,
            "expected the ladder to be walked: {encoded:?}"
        );
        assert!(
            encoded.quality.is_some_and(|q| q < 85),
            "expected a reduced quality: {:?}",
            encoded.quality
        );
    }

    #[test]
    fn reaching_the_floor_over_budget_is_reported_not_raised() {
        // FR-028 and FR-034: the photo still comes back, flagged.
        let budget = ImageBudget {
            max_bytes: 1,
            ..ImageBudget::built_in()
        };
        let encoded = encode_to_budget(
            &noisy(400, 300),
            OutputFormat::Jpeg,
            &budget,
            "hopeless.jpg",
        )
        .expect("still returns a photo");
        assert!(encoded.over_budget);
        assert_eq!(encoded.quality, Some(budget.jpeg_min_quality));
        assert_eq!(encoded.attempts, quality_ladder(85, 50).len());
        assert!(!encoded.bytes.is_empty());
    }

    #[test]
    fn a_lossless_format_does_not_walk_the_ladder() {
        let budget = ImageBudget {
            max_bytes: 1,
            ..ImageBudget::built_in()
        };
        let encoded =
            encode_to_budget(&noisy(40, 40), OutputFormat::Png, &budget, "a.png").expect("encodes");
        assert_eq!(encoded.attempts, 1);
        assert_eq!(encoded.quality, None);
        assert!(encoded.over_budget);
    }

    #[test]
    fn extensions_follow_the_reference() {
        assert_eq!(OutputFormat::Jpeg.extension(".gif"), ".jpg");
        assert_eq!(OutputFormat::Webp.extension(".webp"), ".webp");
        assert_eq!(OutputFormat::Png.extension(".png"), ".png");
    }

    #[test]
    fn unhandled_containers_become_jpeg() {
        // fixtures/reference/formats pins this: photo4.gif publishes as formats-04.jpg.
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::Gif), ".gif"),
            OutputFormat::Jpeg
        );
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::Bmp), ".bmp"),
            OutputFormat::Jpeg
        );
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::Tiff), ".tiff"),
            OutputFormat::Jpeg
        );
    }

    #[test]
    fn the_three_kept_containers_are_kept() {
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::Jpeg), ".jpg"),
            OutputFormat::Jpeg
        );
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::Png), ".png"),
            OutputFormat::Png
        );
        assert_eq!(
            OutputFormat::for_source(Some(image::ImageFormat::WebP), ".webp"),
            OutputFormat::Webp
        );
    }
}
