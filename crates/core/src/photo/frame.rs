//! Crop geometry, and the headroom bias that keeps heads in frame (FR-013, research R5).
//!
//! Pure arithmetic over dimensions: no image is decoded here, so every rule below is
//! unit-testable, instant, and deterministic (constitution IV).
//!
//! The bias exists because editorial photographs of people put the subject's head in the upper
//! third. Centring a crop takes equally from the top and the bottom and decapitates them.
//!
//! Research R5 proposed taking a quarter from the top and three quarters from the bottom. The
//! SC-003 benchmark ([`crates/core/tests/portrait_benchmark.rs`]) shows that rule cutting the
//! head off five of the twenty portraits at 1:1 and ten of them at 3:2, because a head can
//! begin as little as a fifteenth of the way down the frame. R5 anticipated exactly this
//! ("the bias rule failing"), so the vertical rule is now the guaranteeing one: **a vertical
//! crop takes nothing off the top at all**. The whole excess comes off the bottom, which costs
//! floor and cannot cost a head. Horizontal crops still split evenly — nothing about a person
//! makes the left side more important than the right.

use crate::model::photo::CropRect;

/// A target shape, as a ratio rather than a size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AspectRatio {
    /// The width term. Must be non-zero.
    pub width: u32,
    /// The height term. Must be non-zero.
    pub height: u32,
}

impl AspectRatio {
    /// A ratio. Returns `None` if either term is zero, which is not a shape.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        (width > 0 && height > 0).then_some(Self { width, height })
    }

    /// A square.
    pub const SQUARE: Self = Self {
        width: 1,
        height: 1,
    };
    /// The classic landscape frame.
    pub const LANDSCAPE_3_2: Self = Self {
        width: 3,
        height: 2,
    };
    /// A wide banner.
    pub const WIDE_16_9: Self = Self {
        width: 16,
        height: 9,
    };

    /// The same shape turned on its side.
    ///
    /// A photo the editor has rotated is cropped in its *pre-rotation* frame — that is the
    /// space [`CropRect`] is defined in and the space
    /// [`set_crop`](crate::photo::set_crop) validates against — so a landscape target chosen
    /// on the rotated view becomes a portrait target on the stored pixels.
    #[must_use]
    pub fn transposed(self) -> Self {
        Self {
            width: self.height,
            height: self.width,
        }
    }
}

/// How much of the vertical excess comes off the top, as a fraction with this denominator.
///
/// None of it: see the module documentation. Kept as named constants rather than inlined so
/// the rule has one place to change if face detection ever earns its dependency.
const HEADROOM_NUMERATOR: u64 = 0;
const HEADROOM_DENOMINATOR: u64 = 1;

/// The crop that fits `dims` into `target`, or `None` when the image already has that shape.
///
/// The result always lies inside the image (INV-6).
#[must_use]
pub fn default_frame(dims: (u32, u32), target: AspectRatio) -> Option<CropRect> {
    let (width, height) = dims;
    if width == 0 || height == 0 {
        return None;
    }

    let w = u64::from(width);
    let h = u64::from(height);
    let tw = u64::from(target.width);
    let th = u64::from(target.height);

    // Compare w/h against tw/th without leaving the integers, so the decision cannot drift
    // between platforms.
    let image_term = w * th;
    let target_term = h * tw;

    if image_term == target_term {
        return None;
    }

    if image_term > target_term {
        // Wider than the target: take from the sides, evenly.
        let new_width = (h * tw / th).max(1).min(w);
        let excess = w - new_width;
        return Some(CropRect {
            x: (excess / 2) as u32,
            y: 0,
            width: new_width as u32,
            height,
        });
    }

    // Taller than the target: the excess comes off the bottom, so the head survives (FR-013).
    let new_height = (w * th / tw).max(1).min(h);
    let excess = h - new_height;
    let from_top = excess * HEADROOM_NUMERATOR / HEADROOM_DENOMINATOR;
    Some(CropRect {
        x: 0,
        y: from_top as u32,
        width,
        height: new_height as u32,
    })
}

/// Clamps a crop into an image's bounds, so a rectangle from the UI can never violate INV-6.
///
/// Returns `None` when nothing of the rectangle overlaps the image.
#[must_use]
pub fn clamp_to_bounds(crop: CropRect, dims: (u32, u32)) -> Option<CropRect> {
    let (width, height) = dims;
    if width == 0 || height == 0 || crop.width == 0 || crop.height == 0 {
        return None;
    }
    let x = crop.x.min(width.saturating_sub(1));
    let y = crop.y.min(height.saturating_sub(1));
    let w = crop.width.min(width - x);
    let h = crop.height.min(height - y);
    (w > 0 && h > 0).then_some(CropRect {
        x,
        y,
        width: w,
        height: h,
    })
}

#[cfg(test)]
mod tests {
    use super::{AspectRatio, clamp_to_bounds, default_frame};
    use crate::model::photo::CropRect;

    #[test]
    fn an_image_that_already_matches_is_not_cropped() {
        assert_eq!(default_frame((900, 600), AspectRatio::LANDSCAPE_3_2), None);
        assert_eq!(default_frame((600, 600), AspectRatio::SQUARE), None);
        // The ratio is what matters, not the size.
        assert_eq!(default_frame((1920, 1080), AspectRatio::WIDE_16_9), None);
    }

    #[test]
    fn a_portrait_cropped_to_landscape_keeps_the_top() {
        // 600x900 into 3:2 -> 600x400, and all 500px of excess comes off the bottom.
        let crop = default_frame((600, 900), AspectRatio::LANDSCAPE_3_2)
            .expect("a portrait does not already match 3:2");
        assert_eq!(
            crop,
            CropRect {
                x: 0,
                y: 0,
                width: 600,
                height: 400
            }
        );
    }

    #[test]
    fn a_vertical_crop_takes_nothing_off_the_top() {
        // The property that matters, stated independently of any one size: whatever the shape
        // and however much has to go, the top edge of the photo survives — and with it the
        // head (FR-013, SC-003).
        for height in [700u32, 901, 1200, 1333] {
            for target in [
                AspectRatio::SQUARE,
                AspectRatio::LANDSCAPE_3_2,
                AspectRatio::WIDE_16_9,
            ] {
                let Some(crop) = default_frame((600, height), target) else {
                    continue;
                };
                assert_eq!(
                    crop.y, 0,
                    "600x{height} into {target:?} took {} off the top",
                    crop.y
                );
            }
        }
    }

    #[test]
    fn transposing_a_ratio_turns_it_on_its_side() {
        assert_eq!(
            AspectRatio::LANDSCAPE_3_2.transposed(),
            AspectRatio {
                width: 2,
                height: 3
            }
        );
        assert_eq!(AspectRatio::SQUARE.transposed(), AspectRatio::SQUARE);
    }

    #[test]
    fn a_horizontal_crop_is_split_evenly() {
        // 1200x600 into 1:1 -> 600x600, 600px of excess, 300 off each side.
        let crop = default_frame((1200, 600), AspectRatio::SQUARE).expect("wider than square");
        assert_eq!(
            crop,
            CropRect {
                x: 300,
                y: 0,
                width: 600,
                height: 600
            }
        );
    }

    #[test]
    fn the_result_always_lies_inside_the_image() {
        // INV-6, over a spread of shapes and targets.
        let targets = [
            AspectRatio::SQUARE,
            AspectRatio::LANDSCAPE_3_2,
            AspectRatio::WIDE_16_9,
        ];
        for width in [1u32, 7, 100, 601, 1600, 4032] {
            for height in [1u32, 9, 100, 899, 1200, 3024] {
                for target in targets {
                    if let Some(crop) = default_frame((width, height), target) {
                        assert!(
                            crop.fits_within((width, height)),
                            "{width}x{height} into {target:?} gave {crop:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_degenerate_image_is_not_cropped() {
        assert_eq!(default_frame((0, 100), AspectRatio::SQUARE), None);
        assert_eq!(default_frame((100, 0), AspectRatio::SQUARE), None);
    }

    #[test]
    fn a_zero_term_is_not_a_ratio() {
        assert_eq!(AspectRatio::new(0, 1), None);
        assert_eq!(AspectRatio::new(1, 0), None);
    }

    #[test]
    fn clamping_pulls_a_rectangle_back_inside() {
        let crop = CropRect {
            x: 500,
            y: 500,
            width: 400,
            height: 400,
        };
        assert_eq!(
            clamp_to_bounds(crop, (600, 600)),
            Some(CropRect {
                x: 500,
                y: 500,
                width: 100,
                height: 100
            })
        );
    }

    #[test]
    fn clamping_rejects_a_rectangle_with_no_overlap() {
        let crop = CropRect {
            x: 10,
            y: 10,
            width: 0,
            height: 10,
        };
        assert_eq!(clamp_to_bounds(crop, (600, 600)), None);
    }
}
