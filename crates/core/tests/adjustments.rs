//! T070 — reverting an adjustment, and how a rotation composes with EXIF orientation.
//!
//! Two properties, both of which the editor feels directly:
//!
//! - `set_crop(None)` restores the full frame (FR-014). Reverting is assignment, because the
//!   source was never written in the first place.
//! - The editor's quarter-turns compose *on top of* the camera's orientation tag (FR-016,
//!   FR-017), in that order, so a photo shot sideways and then turned by the editor ends up
//!   where the editor put it rather than where the camera thought it was.

use image::{DynamicImage, Rgb, RgbImage};
use newsbuilder_core::model::photo::{
    Adjustments, CropRect, NaturalKey, Orientation, Photo, PhotoId, PhotoOrigin, PhotoSource,
    Quarters,
};
use newsbuilder_core::photo::{self, AspectRatio};
use std::sync::Arc;

/// A photo record over bytes that are never read: these tests are about the recorded
/// adjustments and the transform, not about decoding.
fn photo(dims: (u32, u32)) -> Photo {
    Photo {
        id: PhotoId(1),
        origin: PhotoOrigin::Dropped,
        source: PhotoSource::Bytes(Arc::from(Vec::new().into_boxed_slice())),
        file_name: "portrait.jpg".to_owned(),
        natural_key: NaturalKey::new("portrait.jpg"),
        dimensions: dims,
        orientation: Orientation::Normal,
        adjust: Adjustments::NONE,
    }
}

/// A 2x2 image whose four pixels are all different, so any transform is observable.
///
/// ```text
/// R G
/// B W
/// ```
fn quadrant_image() -> DynamicImage {
    let mut image = RgbImage::new(2, 2);
    image.put_pixel(0, 0, Rgb([255, 0, 0]));
    image.put_pixel(1, 0, Rgb([0, 255, 0]));
    image.put_pixel(0, 1, Rgb([0, 0, 255]));
    image.put_pixel(1, 1, Rgb([255, 255, 255]));
    DynamicImage::ImageRgb8(image)
}

fn pixels(image: &DynamicImage) -> Vec<[u8; 3]> {
    let rgb = image.to_rgb8();
    rgb.pixels().map(|p| p.0).collect()
}

// -------------------------------------------------------------------------------------------
// Reverting
// -------------------------------------------------------------------------------------------

#[test]
fn clearing_the_crop_restores_the_full_frame() {
    let mut subject = photo((600, 900));
    photo::set_crop(
        &mut subject,
        Some(CropRect {
            x: 40,
            y: 60,
            width: 200,
            height: 300,
        }),
    )
    .expect("a crop inside the photo is accepted");
    assert_eq!(subject.effective_dimensions(), (200, 300));

    photo::set_crop(&mut subject, None).expect("clearing always works");
    assert_eq!(subject.adjust.crop, None);
    assert_eq!(
        subject.effective_dimensions(),
        (600, 900),
        "the full frame is back"
    );
}

#[test]
fn clearing_the_crop_leaves_a_rotation_alone() {
    // FR-014 says an edit can be reverted, not that reverting one throws away the others.
    let mut subject = photo((600, 900));
    photo::rotate(&mut subject, 1);
    photo::set_crop(
        &mut subject,
        Some(CropRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        }),
    )
    .expect("accepted");

    photo::set_crop(&mut subject, None).expect("clearing always works");
    assert_eq!(subject.adjust.crop, None);
    assert_eq!(subject.adjust.rotate, Quarters::new(1));
}

#[test]
fn reverting_restores_everything_at_once() {
    let mut subject = photo((600, 900));
    photo::rotate(&mut subject, 2);
    photo::set_crop(
        &mut subject,
        Some(CropRect {
            x: 10,
            y: 10,
            width: 100,
            height: 100,
        }),
    )
    .expect("accepted");

    photo::revert(&mut subject);
    assert_eq!(subject.adjust, Adjustments::NONE);
    assert_eq!(subject.effective_dimensions(), (600, 900));
}

#[test]
fn a_crop_and_a_revert_never_touch_the_source() {
    // FR-015 in miniature: the bytes the photo points at are the bytes it arrived with.
    let original: Arc<[u8]> = Arc::from(vec![1u8, 2, 3, 4].into_boxed_slice());
    let mut subject = photo((600, 900));
    subject.source = PhotoSource::Bytes(Arc::clone(&original));

    photo::set_crop(
        &mut subject,
        Some(CropRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }),
    )
    .expect("accepted");
    photo::rotate(&mut subject, 1);
    photo::revert(&mut subject);

    match &subject.source {
        PhotoSource::Bytes(bytes) => assert_eq!(&bytes[..], &original[..]),
        PhotoSource::Path(path) => panic!("the source changed shape: {}", path.display()),
    }
}

// -------------------------------------------------------------------------------------------
// Composition with EXIF orientation
// -------------------------------------------------------------------------------------------

#[test]
fn no_orientation_and_no_rotation_is_the_identity() {
    let out = photo::apply_adjustments(quadrant_image(), Orientation::Normal, &Adjustments::NONE);
    assert_eq!(pixels(&out), pixels(&quadrant_image()));
}

#[test]
fn the_editors_rotation_is_applied_on_top_of_the_cameras() {
    // A camera that recorded "turn me 90° clockwise", plus an editor who turned it 90° more,
    // must land on 180° — not on 90°, and not on 270°.
    let both = photo::apply_adjustments(
        quadrant_image(),
        Orientation::Rotate90,
        &Adjustments {
            crop: None,
            rotate: Quarters::new(1),
        },
    );
    let half_turn =
        photo::apply_adjustments(quadrant_image(), Orientation::Rotate180, &Adjustments::NONE);
    assert_eq!(pixels(&both), pixels(&half_turn));
}

#[test]
fn four_editor_quarter_turns_leave_the_camera_orientation_showing() {
    for orientation in [
        Orientation::Normal,
        Orientation::Rotate90,
        Orientation::Rotate180,
        Orientation::Rotate270,
        Orientation::FlipHorizontal,
        Orientation::FlipVertical,
        Orientation::Transpose,
        Orientation::Transverse,
    ] {
        let turned = photo::apply_adjustments(
            quadrant_image(),
            orientation,
            &Adjustments {
                crop: None,
                // Quarters normalise, so four turns is no turn.
                rotate: Quarters::new(4),
            },
        );
        let untouched = photo::apply_adjustments(quadrant_image(), orientation, &Adjustments::NONE);
        assert_eq!(pixels(&turned), pixels(&untouched), "{orientation:?}");
    }
}

#[test]
fn the_crop_is_taken_before_the_rotation() {
    // The crop is expressed in oriented, pre-rotation space, so cropping the top-left pixel of
    // a photo the editor then turns yields that pixel — turned — and not whichever pixel
    // happens to be top-left after the turn.
    let out = photo::apply_adjustments(
        quadrant_image(),
        Orientation::Normal,
        &Adjustments {
            crop: Some(CropRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }),
            rotate: Quarters::new(1),
        },
    );
    assert_eq!((out.width(), out.height()), (1, 1));
    assert_eq!(pixels(&out), vec![[255, 0, 0]], "the red top-left pixel");
}

#[test]
fn recorded_dimensions_agree_with_what_the_pipeline_produces() {
    // `effective_dimensions` is what the arrangement rules and the UI reason about; if it
    // disagreed with the pixels, every layout decision would be taken on a lie.
    let mut subject = photo((2, 2));
    // Turn first: rotating drops a crop taken in the old frame, so the crop goes on after.
    photo::rotate(&mut subject, 1);
    photo::set_crop(
        &mut subject,
        Some(CropRect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        }),
    )
    .expect("accepted");

    let out = photo::apply_adjustments(quadrant_image(), subject.orientation, &subject.adjust);
    assert_eq!(
        (out.width(), out.height()),
        subject.effective_dimensions(),
        "a 2x1 crop turned a quarter is 1x2"
    );
}

// -------------------------------------------------------------------------------------------
// Suggested crops survive the round trip
// -------------------------------------------------------------------------------------------

#[test]
fn a_suggested_crop_is_always_one_set_crop_accepts() {
    // The suggestion and the validation must agree about which frame they are talking about
    // (INV-6). A rotated photo is the case where they can drift apart.
    for dims in [(600u32, 900u32), (900, 600), (1000, 1000), (600, 1220)] {
        for quarters in 0i8..4 {
            for target in [
                AspectRatio::SQUARE,
                AspectRatio::LANDSCAPE_3_2,
                AspectRatio::WIDE_16_9,
            ] {
                let mut subject = photo(dims);
                subject.adjust.rotate = Quarters::new(quarters);
                let Some(crop) = photo::suggest_crop(&subject, target) else {
                    continue;
                };
                photo::set_crop(&mut subject, Some(crop)).unwrap_or_else(|e| {
                    panic!("{dims:?} turned {quarters} into {target:?} suggested {crop:?}: {e}")
                });
            }
        }
    }
}

#[test]
fn a_suggestion_for_a_turned_photo_produces_the_shape_the_editor_picked() {
    // The editor sees the rotated photo and asks for 3:2 of *that*. A quarter-turn of a 600x1220
    // portrait shows as 1220x600; the suggestion has to be the pre-rotation rectangle that
    // becomes 3:2 once it is turned.
    let mut subject = photo((600, 1220));
    subject.adjust.rotate = Quarters::new(1);
    let crop = photo::suggest_crop(&subject, AspectRatio::LANDSCAPE_3_2).expect("a crop is needed");
    photo::set_crop(&mut subject, Some(crop)).expect("the suggestion is in bounds");

    let (width, height) = subject.effective_dimensions();
    assert_eq!(
        (width * 2) / height,
        3,
        "the displayed result is 3:2, got {width}x{height}"
    );
}
