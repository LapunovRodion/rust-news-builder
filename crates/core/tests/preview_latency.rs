//! T102 — the SC-008 measurements, and the property they rest on.
//!
//! SC-008 asks for two numbers: *"a text edit is reflected within 150 milliseconds, and a
//! thirty-photo rebuild finishes in under a second."* The preview rebuilds on every keystroke
//! through `SessionState::build_preview`, which is `core::build_with_cache`, so this is where
//! both are measured.
//!
//! The file is in two halves, because a wall-clock assertion and a correctness assertion fail
//! for very different reasons and should not be able to mask each other:
//!
//! - **The property** (always runs). A rebuild after a text-only edit re-encodes *no* photo,
//!   and produces bytes identical to an uncached build. This is deterministic, so it belongs in
//!   the merge gate: it is what makes the numbers achievable, and it is what a future change
//!   would break first.
//! - **The numbers** (`#[ignore]`). Timings mean nothing in a debug build — the image decoder
//!   runs an order of magnitude slower — and a wall-clock assertion in the merge gate is a
//!   flake waiting for a loaded CI box. Run them with `just bench-preview`, which builds
//!   optimised, and record what they say.
//!
//! Before the cache, every rebuild re-encoded every placed photo: thirty decodes and thirty
//! quality-ladder searches per keystroke. `retitling_costs_no_re_encoding_either` is the reason
//! the published stem is deliberately not part of the cache key.

use std::time::{Duration, Instant};

use newsbuilder_core::build::{BuildContext, EmbeddedBytes, ProcessCache, build, build_with_cache};
use newsbuilder_core::model::item::{Block, Layout, NewsItem};
use newsbuilder_core::model::photo::{
    Adjustments, CropRect, NaturalKey, Orientation, Photo, PhotoOrigin, PhotoSource,
};
use newsbuilder_core::publish::slug::slugify;

/// SC-008's first number: a text edit is on screen within this.
const TEXT_EDIT_BUDGET: Duration = Duration::from_millis(150);

/// SC-008's second number: a thirty-photo item rebuilds within this from cold.
const THIRTY_PHOTO_BUDGET: Duration = Duration::from_secs(1);

/// The item size SC-008 names.
const PHOTO_COUNT: usize = 30;

/// A photograph-shaped JPEG.
///
/// Smooth gradients with a little structure, rather than the seeded noise the other fixtures
/// use: noise is incompressible, which would send every photo to the bottom of the quality
/// ladder and measure a case no editor ever publishes. This compresses roughly the way a
/// photograph does, so the ladder settles after a step or two.
fn photograph(width: u32, height: u32, seed: u32) -> Vec<u8> {
    use image::{ImageEncoder, Rgb, RgbImage};

    let mut image = RgbImage::new(width, height);
    let offset = seed.wrapping_mul(37) % 255;
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let fx = x as f32 / width as f32;
        let fy = y as f32 / height as f32;
        // Two low-frequency waves and a slow ramp: enough detail that the encoder has work to
        // do, little enough that it behaves like a photograph rather than like static.
        let a = (fx * 6.0).sin() * 40.0 + (fy * 4.0).cos() * 30.0;
        let b = (fx * 3.0 + fy * 5.0).sin() * 35.0;
        let base = 120.0 + a + b + offset as f32 * 0.2;
        *pixel = Rgb([
            base.clamp(0.0, 255.0) as u8,
            (base * 0.85).clamp(0.0, 255.0) as u8,
            (base * 0.7).clamp(0.0, 255.0) as u8,
        ]);
    }

    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92)
        .write_image(
            image.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .expect("encoding a generated image cannot fail");
    bytes
}

/// A thirty-photo item with text between the placements, which is the shape SC-008 is about.
fn thirty_photo_item() -> NewsItem {
    let mut item = NewsItem::new();
    item.title = "День Конституции".to_owned();
    item.slug = slugify(&item.title);

    let mut body = vec![Block::paragraph(
        "Мероприятие прошло в главном корпусе университета.",
    )];
    for n in 0..PHOTO_COUNT {
        let id = item.mint_photo_id();
        let name = format!("IMG_{:04}.jpg", n + 1);
        // Full HD sources, above the 1600px budget, so every build does a real resize too.
        let (width, height) = if n % 3 == 0 {
            (1600, 2400)
        } else {
            (2400, 1600)
        };
        item.photos.push(Photo {
            id,
            origin: PhotoOrigin::Dropped,
            source: PhotoSource::Bytes(photograph(width, height, n as u32).into()),
            natural_key: NaturalKey::new(&name),
            file_name: name,
            dimensions: (width, height),
            orientation: Orientation::Normal,
            adjust: Adjustments::NONE,
        });
        body.push(Block::placement(vec![id], Layout::FullWidth));
        body.push(Block::paragraph(format!(
            "Подпись к снимку номер {}.",
            n + 1
        )));
    }

    item.body = body;
    item.normalise_paragraph_kinds();
    item
}

fn preview_context(item: &NewsItem) -> BuildContext {
    BuildContext::preview(item.slug.clone())
}

/// Edits a paragraph, the way typing into the editor does.
fn edit_some_text(item: &mut NewsItem, text: &str) {
    let first = item
        .body
        .iter_mut()
        .find(|block| matches!(block, Block::Paragraph { .. }))
        .expect("the fixture opens with a paragraph");
    *first = Block::paragraph(text);
    item.normalise_paragraph_kinds();
}

// =============================================================================================
// The property the numbers rest on — deterministic, and part of the merge gate
// =============================================================================================

#[test]
fn a_text_edit_re_encodes_nothing() {
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();

    let first = build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("the item builds");
    assert_eq!(first.processed.len(), PHOTO_COUNT);
    assert_eq!(
        cache.encoded_last_build(),
        PHOTO_COUNT,
        "the first build has nothing to reuse"
    );

    edit_some_text(&mut item, "Мероприятие прошло в актовом зале университета.");

    let second = build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("the item rebuilds");
    assert_eq!(
        cache.encoded_last_build(),
        0,
        "SC-008: a text edit re-encoded {} photo(s)",
        cache.encoded_last_build()
    );
    assert_eq!(cache.reused_last_build(), PHOTO_COUNT);
    assert!(
        second.fragment.contains("в актовом зале"),
        "the edit did not reach the fragment"
    );
}

#[test]
fn a_cached_rebuild_is_byte_identical_to_an_uncached_one() {
    // The cache is an optimisation and nothing else. If it could change the output, FR-024's
    // determinism would be a matter of which path a caller took, and the parity suite — which
    // goes through `build` — would never notice.
    let item = thirty_photo_item();
    let ctx = preview_context(&item);

    let plain = build(&item, &ctx, &EmbeddedBytes).expect("the item builds");

    let mut cache = ProcessCache::new();
    let cold = build_with_cache(&item, &ctx, &EmbeddedBytes, &mut cache).expect("builds");
    let warm = build_with_cache(&item, &ctx, &EmbeddedBytes, &mut cache).expect("rebuilds");

    assert_eq!(plain.fragment, cold.fragment);
    assert_eq!(plain.fragment, warm.fragment);
    assert_eq!(plain.warnings, cold.warnings);
    assert_eq!(plain.warnings, warm.warnings);

    for ((expected, from_cold), from_warm) in plain
        .processed
        .iter()
        .zip(&cold.processed)
        .zip(&warm.processed)
    {
        assert_eq!(expected.file_name, from_cold.file_name);
        assert_eq!(expected.file_name, from_warm.file_name);
        assert_eq!(expected.quality, from_warm.quality);
        assert_eq!(expected.dimensions, from_warm.dimensions);
        assert_eq!(
            expected.bytes, from_warm.bytes,
            "{} came back from the cache with different bytes",
            expected.file_name
        );
    }
}

#[test]
fn cropping_one_photo_re_encodes_that_photo_and_no_other() {
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache).expect("builds");

    item.photos[4].adjust.crop = Some(CropRect {
        x: 100,
        y: 80,
        width: 1200,
        height: 800,
    });

    let out = build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("rebuilds");
    assert_eq!(
        cache.encoded_last_build(),
        1,
        "only the cropped photo needed re-encoding"
    );
    assert_eq!(cache.reused_last_build(), PHOTO_COUNT - 1);
    assert_eq!(out.processed.len(), PHOTO_COUNT);

    // And the crop actually reached the output rather than being served stale from the cache.
    let cropped = &out.processed[4];
    assert_eq!(
        cropped.dimensions.1 * 1200,
        cropped.dimensions.0 * 800,
        "{} is not the cropped shape: {:?}",
        cropped.file_name,
        cropped.dimensions
    );
}

#[test]
fn rotating_one_photo_re_encodes_that_photo_and_no_other() {
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache).expect("builds");

    item.photos[9].adjust.rotate = item.photos[9].adjust.rotate.turn(1);

    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache).expect("rebuilds");
    assert_eq!(cache.encoded_last_build(), 1);
    assert_eq!(cache.reused_last_build(), PHOTO_COUNT - 1);
}

#[test]
fn retitling_costs_no_re_encoding_either() {
    // A title is text, and SC-008 does not carve it out. The title decides the published *name*
    // of every photo but none of their bytes, which is why the stem is kept out of the cache
    // key — otherwise editing a headline would re-encode all thirty.
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    let before = build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("builds");
    assert!(
        before.processed[0]
            .file_name
            .starts_with("den-konstitutsii")
    );

    item.title = "День знаний".to_owned();
    item.slug = slugify(&item.title);

    let after = build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("rebuilds");
    assert_eq!(
        cache.encoded_last_build(),
        0,
        "retitling re-encoded {} photo(s)",
        cache.encoded_last_build()
    );

    // The names followed the new title even though nothing was re-encoded.
    assert!(
        after.processed[0].file_name.starts_with("den-znanii"),
        "{}",
        after.processed[0].file_name
    );
    assert_eq!(before.processed[0].bytes, after.processed[0].bytes);
}

#[test]
fn a_photo_added_to_a_warm_cache_is_the_only_one_encoded() {
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache).expect("builds");

    let id = item.mint_photo_id();
    item.photos.push(Photo {
        id,
        origin: PhotoOrigin::Pasted,
        source: PhotoSource::Bytes(photograph(1800, 1200, 99).into()),
        natural_key: NaturalKey::new("pasted.jpg"),
        file_name: "pasted.jpg".to_owned(),
        dimensions: (1800, 1200),
        orientation: Orientation::Normal,
        adjust: Adjustments::NONE,
    });
    item.body
        .push(Block::placement(vec![id], Layout::FullWidth));

    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache).expect("rebuilds");
    assert_eq!(cache.encoded_last_build(), 1);
    assert_eq!(cache.reused_last_build(), PHOTO_COUNT);
}

// =============================================================================================
// The numbers — run optimised, with `just bench-preview`
// =============================================================================================

/// Reports a measurement so the run leaves a record, not just a pass.
fn report(what: &str, elapsed: Duration, budget: Duration) {
    println!(
        "SC-008 {what}: {:?} (budget {:?}, {:.0}% of it)",
        elapsed,
        budget,
        elapsed.as_secs_f64() / budget.as_secs_f64() * 100.0
    );
}

#[test]
#[ignore = "wall-clock measurement; meaningless unoptimised. Run with `just bench-preview`"]
fn a_thirty_photo_item_rebuilds_from_cold_within_a_second() {
    let item = thirty_photo_item();
    let ctx = preview_context(&item);
    let mut cache = ProcessCache::new();

    let started = Instant::now();
    let out = build_with_cache(&item, &ctx, &EmbeddedBytes, &mut cache).expect("the item builds");
    let elapsed = started.elapsed();

    assert_eq!(out.processed.len(), PHOTO_COUNT);
    report("cold 30-photo rebuild", elapsed, THIRTY_PHOTO_BUDGET);
    assert!(
        elapsed < THIRTY_PHOTO_BUDGET,
        "SC-008: a cold {PHOTO_COUNT}-photo rebuild took {elapsed:?}, over the \
         {THIRTY_PHOTO_BUDGET:?} budget"
    );
}

#[test]
#[ignore = "wall-clock measurement; meaningless unoptimised. Run with `just bench-preview`"]
fn a_text_edit_is_reflected_within_a_hundred_and_fifty_milliseconds() {
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("the item builds");

    // Ten edits, and the slowest one is what SC-008 is about: the editor experiences the worst
    // keystroke, not the average of them.
    let mut worst = Duration::ZERO;
    for n in 0..10 {
        edit_some_text(&mut item, &format!("Мероприятие прошло в зале номер {n}."));
        let ctx = preview_context(&item);
        let started = Instant::now();
        let out = build_with_cache(&item, &ctx, &EmbeddedBytes, &mut cache).expect("rebuilds");
        worst = worst.max(started.elapsed());
        assert!(out.fragment.contains(&format!("зале номер {n}")));
    }

    report("worst of ten text edits", worst, TEXT_EDIT_BUDGET);
    assert_eq!(
        cache.encoded_last_build(),
        0,
        "a text edit must re-encode nothing"
    );
    assert!(
        worst < TEXT_EDIT_BUDGET,
        "SC-008: the slowest rebuild after a text edit took {worst:?}, over the \
         {TEXT_EDIT_BUDGET:?} budget"
    );
}

#[test]
#[ignore = "wall-clock measurement; meaningless unoptimised. Run with `just bench-preview`"]
fn cropping_one_photo_is_reflected_within_the_text_edit_budget_too() {
    // The crop control updates the preview live (FR-014), so a drag has to keep up with the
    // same budget even though it does re-encode one photo.
    let mut item = thirty_photo_item();
    let mut cache = ProcessCache::new();
    build_with_cache(&item, &preview_context(&item), &EmbeddedBytes, &mut cache)
        .expect("the item builds");

    let mut worst = Duration::ZERO;
    for step in 1..=5u32 {
        item.photos[0].adjust.crop = Some(CropRect {
            x: step * 10,
            y: step * 10,
            width: 1400 - step * 20,
            height: 900 - step * 20,
        });
        let ctx = preview_context(&item);
        let started = Instant::now();
        build_with_cache(&item, &ctx, &EmbeddedBytes, &mut cache).expect("rebuilds");
        worst = worst.max(started.elapsed());
        assert_eq!(cache.encoded_last_build(), 1);
    }

    report("worst of five crop drags", worst, TEXT_EDIT_BUDGET);
    assert!(
        worst < TEXT_EDIT_BUDGET,
        "SC-008: the slowest rebuild while dragging a crop took {worst:?}, over the \
         {TEXT_EDIT_BUDGET:?} budget"
    );
}
