//! Byte-for-byte fragment parity against the Python reference (constitution I and III).
//!
//! This is the gate the whole project turns on. Every case here was rendered by
//! `news_builder.py` at the pinned commit, and the bytes it produced are in
//! `fixtures/reference/<case>/fragment.html`.

mod support;

use pretty_assertions::assert_eq;

#[test]
fn every_comparable_case_renders_byte_for_byte() {
    let cases = support::fragment_comparable_cases();
    assert!(
        !cases.is_empty(),
        "the fixture corpus is missing; run `cargo run -p capture-reference`"
    );

    let mut failed = Vec::new();
    for case in &cases {
        let Some(output) = support::build_case(case) else {
            failed.push(format!("{case}: could not be built"));
            continue;
        };
        let expected = support::golden(case, "fragment.html");
        let rendered = support::without_readmore(&output.fragment);
        if rendered != expected {
            failed.push(case.clone());
            // Show the first difference for the first failure only; a wall of diffs for
            // twenty cases helps nobody.
            if failed.len() == 1 {
                assert_eq!(
                    rendered, expected,
                    "case `{case}` diverged from the reference"
                );
            }
        }
    }

    assert!(
        failed.is_empty(),
        "cases diverged from the reference: {failed:?}"
    );
}

#[test]
fn each_of_the_four_layouts_matches_on_its_own() {
    // The contract asks for a golden per marker form, isolated from the others.
    for case in [
        "marker-full-width",
        "marker-row",
        "marker-float-left",
        "marker-float-right",
    ] {
        let output =
            support::build_case(case).unwrap_or_else(|| panic!("case `{case}` should build"));
        assert_eq!(
            support::without_readmore(&output.fragment),
            support::golden(case, "fragment.html"),
            "layout case `{case}`"
        );
    }
}

#[test]
fn a_row_of_one_photo_stays_a_row() {
    // data-model.md's INV-4 says a single-photo row is normalised to full width "matching
    // reference behaviour". The reference does no such thing, and this golden proves it.
    let output = support::build_case("row-of-one").expect("row-of-one should build");
    let expected = support::golden("row-of-one", "fragment.html");
    assert_eq!(support::without_readmore(&output.fragment), expected);
    assert!(
        expected.contains("display: flex"),
        "the reference rendered a row wrapper, not a full-width placement"
    );
}

#[test]
fn the_fragment_carries_inline_styles_only() {
    // FR-023, over every case rather than a constructed example.
    for case in support::fragment_comparable_cases() {
        let output = support::build_case(&case).unwrap_or_else(|| panic!("`{case}` should build"));
        let lowered = output.fragment.to_ascii_lowercase();
        for forbidden in [
            "<style",
            "<link",
            "<script",
            "class=",
            "<!doctype",
            "<html",
            "<body",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "`{case}` contains {forbidden}"
            );
        }
    }
}

#[test]
fn rendering_the_same_case_twice_gives_the_same_bytes() {
    // FR-024 and constitution IV, asserted on real inputs rather than a synthetic item.
    for case in support::fragment_comparable_cases() {
        let first = support::build_case(&case).unwrap_or_else(|| panic!("`{case}` should build"));
        let second = support::build_case(&case).unwrap_or_else(|| panic!("`{case}` should build"));
        assert_eq!(
            first.fragment, second.fragment,
            "case `{case}` is not deterministic"
        );
    }
}
