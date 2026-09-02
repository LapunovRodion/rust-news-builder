//! EXIF orientation (FR-016).
//!
//! Cameras record which way up they were held rather than rotating the pixels. Applying that
//! tag is the difference between a portrait publishing upright and publishing on its side.
//! The correction is an explicit transform in one place, so it is visible and testable —
//! rather than a side effect of whichever decoder happens to run.

use image::DynamicImage;

use crate::model::photo::Orientation;

/// Reads the EXIF orientation tag from encoded image bytes.
///
/// A file with no EXIF, or EXIF the parser cannot read, is [`Orientation::Normal`]. A missing
/// tag is not an error: most PNGs have none.
#[must_use]
pub fn read_orientation(bytes: &[u8]) -> Orientation {
    let mut cursor = std::io::Cursor::new(bytes);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut cursor) else {
        return Orientation::Normal;
    };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .map_or(Orientation::Normal, |value| {
            Orientation::from_exif(value as u16)
        })
}

/// Applies an orientation, returning an image whose pixels are the right way up.
///
/// [`Orientation::Normal`] returns the image untouched, so the common case costs nothing.
#[must_use]
pub fn apply(image: DynamicImage, orientation: Orientation) -> DynamicImage {
    match orientation {
        Orientation::Normal => image,
        Orientation::FlipHorizontal => image.fliph(),
        Orientation::Rotate180 => image.rotate180(),
        Orientation::FlipVertical => image.flipv(),
        // The four transposing cases are a flip composed with a rotation. Spelling them out
        // beats a table: each line is checkable against the EXIF specification directly.
        Orientation::Transpose => image.rotate90().fliph(),
        Orientation::Rotate90 => image.rotate90(),
        Orientation::Transverse => image.rotate270().fliph(),
        Orientation::Rotate270 => image.rotate270(),
    }
}

/// The dimensions an image has once `orientation` is applied.
#[must_use]
pub fn oriented_dimensions(dims: (u32, u32), orientation: Orientation) -> (u32, u32) {
    if orientation.swaps_axes() {
        (dims.1, dims.0)
    } else {
        dims
    }
}

#[cfg(test)]
mod tests {
    use super::{apply, oriented_dimensions, read_orientation};
    use crate::model::photo::Orientation;
    use image::{DynamicImage, Rgb, RgbImage};

    fn corner_marked_image() -> DynamicImage {
        // A 2x1 image with distinguishable pixels, so a transform's effect is observable.
        let mut image = RgbImage::new(2, 1);
        image.put_pixel(0, 0, Rgb([255, 0, 0]));
        image.put_pixel(1, 0, Rgb([0, 0, 255]));
        DynamicImage::ImageRgb8(image)
    }

    #[test]
    fn every_exif_value_maps() {
        assert_eq!(Orientation::from_exif(1), Orientation::Normal);
        assert_eq!(Orientation::from_exif(2), Orientation::FlipHorizontal);
        assert_eq!(Orientation::from_exif(3), Orientation::Rotate180);
        assert_eq!(Orientation::from_exif(4), Orientation::FlipVertical);
        assert_eq!(Orientation::from_exif(5), Orientation::Transpose);
        assert_eq!(Orientation::from_exif(6), Orientation::Rotate90);
        assert_eq!(Orientation::from_exif(7), Orientation::Transverse);
        assert_eq!(Orientation::from_exif(8), Orientation::Rotate270);
    }

    #[test]
    fn an_out_of_range_value_is_treated_as_upright() {
        assert_eq!(Orientation::from_exif(0), Orientation::Normal);
        assert_eq!(Orientation::from_exif(9), Orientation::Normal);
        assert_eq!(Orientation::from_exif(65535), Orientation::Normal);
    }

    #[test]
    fn bytes_with_no_exif_are_upright() {
        assert_eq!(
            read_orientation(b"not an image at all"),
            Orientation::Normal
        );
        assert_eq!(read_orientation(&[]), Orientation::Normal);
    }

    #[test]
    fn normal_is_the_identity() {
        let image = corner_marked_image();
        let out = apply(image.clone(), Orientation::Normal);
        assert_eq!(out.to_rgb8(), image.to_rgb8());
    }

    #[test]
    fn a_horizontal_flip_swaps_the_ends() {
        let out = apply(corner_marked_image(), Orientation::FlipHorizontal).to_rgb8();
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 255]);
        assert_eq!(out.get_pixel(1, 0).0, [255, 0, 0]);
    }

    #[test]
    fn quarter_turns_swap_the_axes() {
        for orientation in [
            Orientation::Rotate90,
            Orientation::Rotate270,
            Orientation::Transpose,
            Orientation::Transverse,
        ] {
            assert!(orientation.swaps_axes(), "{orientation:?}");
            assert_eq!(oriented_dimensions((4000, 3000), orientation), (3000, 4000));
            let out = apply(corner_marked_image(), orientation);
            assert_eq!((out.width(), out.height()), (1, 2), "{orientation:?}");
        }
    }

    #[test]
    fn half_turns_and_flips_keep_the_axes() {
        for orientation in [
            Orientation::Normal,
            Orientation::Rotate180,
            Orientation::FlipHorizontal,
            Orientation::FlipVertical,
        ] {
            assert!(!orientation.swaps_axes(), "{orientation:?}");
            assert_eq!(oriented_dimensions((4000, 3000), orientation), (4000, 3000));
        }
    }
}
