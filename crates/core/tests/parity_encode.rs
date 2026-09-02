//! Photo-processing parity (T049).
//!
//! # What is compared, and what is not
//!
//! The reference encodes through Pillow, which wraps libjpeg-turbo and libwebp; the port
//! encodes through the `image` crate. Two different encoders never agree bit for bit, so the
//! published *bytes* are outside parity — deviation D-10 records that.
//!
//! Everything an editor or the CMS can observe is inside it, and is asserted here: the
//! published file name, the extension the format resolves to, the output dimensions, and the
//! shape of the quality ladder.

mod support;

use newsbuilder_core::model::appearance::ImageBudget;
use newsbuilder_core::photo::encode::{OutputFormat, quality_ladder};
use pretty_assertions::assert_eq;
use serde_json::Value;

fn encode_golden(case: &str) -> Option<Vec<Value>> {
    let raw = std::fs::read_to_string(support::golden_dir(case).join("encode.json")).ok()?;
    serde_json::from_str::<Vec<Value>>(&raw).ok()
}

#[test]
fn published_names_and_dimensions_match() {
    let mut checked = 0usize;
    let mut mismatches = Vec::new();

    for case in support::fragment_comparable_cases() {
        let Some(expected) = encode_golden(&case) else {
            continue;
        };
        let Some(output) = support::build_case(&case) else {
            continue;
        };

        // The reference processes every discovered photo; the port skips the unused ones
        // (they are not uploaded). Compare the ones both produced, by name.
        for processed in &output.processed {
            let Some(reference) = expected
                .iter()
                .find(|entry| entry["file"].as_str() == Some(processed.file_name.as_str()))
            else {
                mismatches.push(format!(
                    "{case}: `{}` has no counterpart",
                    processed.file_name
                ));
                continue;
            };
            let width = reference["width"].as_u64().expect("a number");
            let height = reference["height"].as_u64().expect("a number");
            if (
                u64::from(processed.dimensions.0),
                u64::from(processed.dimensions.1),
            ) != (width, height)
            {
                mismatches.push(format!(
                    "{case}: `{}` is {}x{}, the reference produced {width}x{height}",
                    processed.file_name, processed.dimensions.0, processed.dimensions.1
                ));
            }
            checked += 1;
        }
    }

    assert!(
        checked > 10,
        "expected several photos to compare, checked {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "photo processing diverged:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn a_gif_publishes_as_a_jpeg() {
    // fixtures/reference/formats: photo4.gif becomes formats-04.jpg, which changes the URL.
    // html-output.md's "keeps its source extension" is only true for JPEG, PNG and WebP.
    let expected = encode_golden("formats").expect("the formats case is captured");
    let names: Vec<&str> = expected.iter().filter_map(|e| e["file"].as_str()).collect();
    assert_eq!(
        names,
        vec![
            "formats-01.jpg",
            "formats-02.png",
            "formats-03.webp",
            "formats-04.jpg"
        ]
    );

    let output = support::build_case("formats").expect("formats should build");
    let produced: Vec<&str> = output
        .processed
        .iter()
        .map(|p| p.file_name.as_str())
        .collect();
    assert_eq!(produced, names);
}

#[test]
fn the_quality_ladder_has_the_reference_shape() {
    // The oversized case walks the ladder down from 85. What is comparable across encoders is
    // the ladder itself and the fact that the search steps rather than jumps.
    let expected = encode_golden("oversized").expect("the oversized case is captured");
    let entry = expected.first().expect("one photo");
    let attempts = entry["attempts"].as_u64().expect("a number");
    let quality = entry["quality"].as_u64().expect("a number");

    let budget = ImageBudget::built_in();
    let ladder = quality_ladder(budget.jpeg_quality, budget.jpeg_min_quality);
    assert_eq!(ladder, vec![85, 80, 75, 70, 65, 60, 55, 50]);
    assert!(
        attempts > 1,
        "the reference stepped down, so the ladder is exercised"
    );
    assert_eq!(
        ladder.get(attempts as usize - 1).copied(),
        Some(quality as u8),
        "the reference's {attempts}th rung should be quality {quality}"
    );
}

#[test]
fn an_oversized_photo_is_scaled_to_the_width_limit() {
    // The reference resizes on width alone; the source is 2400 wide and max_width is 1600.
    let expected = encode_golden("oversized").expect("captured");
    let entry = expected.first().expect("one photo");
    assert_eq!(entry["width"].as_u64(), Some(1600));

    let output = support::build_case("oversized").expect("oversized should build");
    assert_eq!(output.processed[0].dimensions.0, 1600);
}

#[test]
fn the_three_kept_containers_survive_the_pipeline() {
    let output = support::build_case("formats").expect("formats should build");
    let extensions: Vec<&str> = output
        .processed
        .iter()
        .filter_map(|p| p.file_name.rsplit_once('.').map(|(_, ext)| ext))
        .collect();
    assert_eq!(extensions, vec!["jpg", "png", "webp", "jpg"]);
    assert_eq!(OutputFormat::Png.extension(".png"), ".png");
}
