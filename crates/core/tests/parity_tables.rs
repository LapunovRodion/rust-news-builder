//! Parity for the tables the reference exposes directly: ordering, transliteration,
//! normalisation, and the built-in appearance.
//!
//! These are captured by calling the reference's own functions rather than by rendering, so a
//! divergence points at one rule instead of at a fragment.

mod support;

use newsbuilder_core::import::plain;
use newsbuilder_core::model::appearance::{Appearance, StyleSet};
use newsbuilder_core::model::photo::NaturalKey;
use newsbuilder_core::publish::slug::slugify;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn table(name: &str) -> Value {
    let raw = support::golden("_tables", name);
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("_tables/{name} is not valid JSON: {e}"))
}

// -------------------------------------------------------------------------------------
// T022 — natural-sort ordering
// -------------------------------------------------------------------------------------

#[test]
fn natural_sort_matches_the_reference() {
    let data = table("natural_sort.json");
    let input: Vec<String> = data["input"]
        .as_array()
        .expect("input is an array")
        .iter()
        .map(|v| v.as_str().expect("a string").to_owned())
        .collect();
    let expected: Vec<String> = data["ordered"]
        .as_array()
        .expect("ordered is an array")
        .iter()
        .map(|v| v.as_str().expect("a string").to_owned())
        .collect();

    let mut ordered = input;
    ordered.sort_by(|a, b| NaturalKey::new(a).cmp(&NaturalKey::new(b)));
    assert_eq!(ordered, expected);
}

#[test]
fn photo_two_precedes_photo_ten() {
    // The rule's whole reason for existing, stated on its own so a failure reads plainly.
    assert!(NaturalKey::new("photo2.jpg") < NaturalKey::new("photo10.jpg"));
}

// -------------------------------------------------------------------------------------
// T047 — slug transliteration
// -------------------------------------------------------------------------------------

#[test]
fn every_captured_slug_matches() {
    let mut mismatches = Vec::new();
    for entry in table("slugs.json").as_array().expect("an array") {
        let input = entry["input"].as_str().expect("a string");
        let expected = entry["slug"].as_str().expect("a string");
        let produced = slugify(input);
        if produced.as_str() != expected {
            mismatches.push(format!("{input:?}: expected {expected:?}, got {produced}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "transliteration diverged:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn the_transliteration_table_itself_matches() {
    // Comparing the table, not just its effects: a missing row would otherwise only show up
    // for the one headline that happens to use that letter.
    let captured = table("transliteration.json");
    let entries = captured.as_object().expect("an object");
    for (cyrillic, latin) in entries {
        let mut chars = cyrillic.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            panic!("the table's keys should be single characters, got {cyrillic:?}");
        };
        let expected = latin.as_str().expect("a string");
        // Slugify a word built around the character so the mapping is observable even for
        // the two signs, which transliterate to nothing.
        let produced = slugify(&format!("a{ch}a"));
        assert_eq!(
            produced.as_str(),
            format!("a{expected}a"),
            "character {cyrillic:?} should transliterate to {expected:?}"
        );
    }
}

// -------------------------------------------------------------------------------------
// Text normalisation
// -------------------------------------------------------------------------------------

#[test]
fn text_normalisation_matches_the_reference() {
    let data = table("normalisation.json");
    for entry in data["normalize_text_content"].as_array().expect("an array") {
        let input = entry["input"].as_str().expect("a string");
        let expected = entry["output"].as_str().expect("a string");
        assert_eq!(
            plain::normalize_text_content(input),
            expected,
            "normalize_text_content({input:?})"
        );
    }
}

#[test]
fn paragraph_splitting_matches_the_reference() {
    let data = table("normalisation.json");
    for entry in data["split_paragraphs"].as_array().expect("an array") {
        let input = entry["input"].as_str().expect("a string");
        let expected: Vec<String> = entry["output"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| v.as_str().expect("a string").to_owned())
            .collect();
        assert_eq!(
            plain::split_paragraphs(input),
            expected,
            "split_paragraphs({input:?})"
        );
    }
}

// -------------------------------------------------------------------------------------
// T016/T019 — the built-in appearance and the shipped presets
// -------------------------------------------------------------------------------------

#[test]
fn the_built_in_appearance_reproduces_the_reference_defaults() {
    let config = table("default_config.json");
    let budget = Appearance::built_in().image;
    let image = &config["image"];
    assert_eq!(
        u64::from(budget.max_width),
        image["max_width"].as_u64().expect("a number")
    );
    assert_eq!(
        budget.max_bytes,
        image["max_bytes"].as_u64().expect("a number")
    );
    assert_eq!(
        u64::from(budget.jpeg_quality),
        image["jpeg_quality"].as_u64().expect("a number")
    );
    assert_eq!(
        u64::from(budget.jpeg_min_quality),
        image["jpeg_min_quality"].as_u64().expect("a number")
    );
    assert_eq!(
        u64::from(budget.webp_quality),
        image["webp_quality"].as_u64().expect("a number")
    );
    assert_eq!(
        u64::from(budget.webp_min_quality),
        image["webp_min_quality"].as_u64().expect("a number")
    );
}

#[test]
fn every_built_in_style_string_matches_character_for_character() {
    let config = table("default_config.json");
    let styles = &config["styles"];
    let built_in = StyleSet::built_in();
    let slots: [(&str, &String); 12] = [
        ("container", &built_in.container),
        ("title", &built_in.title),
        ("paragraph", &built_in.paragraph),
        ("lead", &built_in.lead),
        ("image_wrapper", &built_in.image_wrapper),
        ("image", &built_in.image),
        ("row_wrapper", &built_in.row_wrapper),
        ("row_item", &built_in.row_item),
        ("row_image", &built_in.row_image),
        ("float_left", &built_in.float_left),
        ("float_right", &built_in.float_right),
        ("clear", &built_in.clear),
    ];
    for (name, value) in slots {
        assert_eq!(
            value.as_str(),
            styles[name]
                .as_str()
                .unwrap_or_else(|| panic!("`{name}` is missing from the capture")),
            "style slot `{name}`"
        );
    }
}

#[test]
fn the_container_style_prefix_matches() {
    let captured = table("container_style.json");
    let mut styles = StyleSet::built_in();

    assert_eq!(
        styles.container_style().as_deref(),
        captured["built_in"].as_str(),
        "the built-in container"
    );

    styles.container = "__omit__".to_owned();
    assert!(
        captured["omit"].is_null(),
        "the reference omits the container for __omit__"
    );
    assert_eq!(styles.container_style(), None);

    styles.container = String::new();
    assert_eq!(
        styles.container_style().as_deref(),
        captured["empty"].as_str()
    );

    styles.container = "color: red".to_owned();
    assert_eq!(
        styles.container_style().as_deref(),
        captured["no_trailing_semicolon"].as_str()
    );
}

#[test]
fn every_shipped_preset_loads_unmodified() {
    // The constitution requires the reference's `style-presets/*.json` to keep working.
    let dir = support::golden_dir("_tables").join("style-presets");
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} should hold the captured presets: {e}", dir.display()));

    let mut loaded = 0usize;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let json = std::fs::read_to_string(&path).expect("a readable preset");
        let result = newsbuilder_core::model::appearance_config::load(&json);
        assert!(
            result.is_ok(),
            "preset {} failed to load: {:?}",
            path.display(),
            result.err()
        );
        loaded += 1;
    }
    assert!(
        loaded >= 5,
        "expected the reference's presets to be captured, found {loaded}"
    );
}

#[test]
fn the_warm_editorial_preset_is_the_built_in_appearance() {
    // It is the one that ships, so the parity fixtures compare like with like.
    let path = support::golden_dir("_tables").join("style-presets/warm-editorial.json");
    let json = std::fs::read_to_string(&path).expect("the preset is captured");
    let loaded = newsbuilder_core::model::appearance_config::load(&json).expect("it loads");
    assert_eq!(loaded.appearance.styles, StyleSet::built_in());
}
