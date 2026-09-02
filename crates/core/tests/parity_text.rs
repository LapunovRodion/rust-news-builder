//! Parity for what the reference read out of each document: the title it detected, the body it
//! normalised, and the blocks it parsed.
//!
//! This is where the Word cases are checked. Their *fragments* cannot be compared — the port
//! derives placements from a document's embedded image positions and the reference reads
//! photos from a folder (FR-003, deviation D-6) — but the text extraction is the same job, and
//! the two real editorial documents in the corpus are the only place it is tested against
//! something Word actually wrote.

mod support;

use newsbuilder_core::import::{plain, title};
use newsbuilder_core::model::item::{Block, SourceFormat};
use pretty_assertions::assert_eq;
use serde_json::Value;

/// The title and body the reference reported for a case.
fn golden_title_and_body(case: &str) -> (Option<String>, String) {
    let raw_title = support::golden(case, "title.txt");
    let detected = support::golden(case, "title_detected.txt");
    let title = if detected.trim() == "none" {
        None
    } else {
        Some(raw_title.trim_end_matches('\n').to_owned())
    };
    let body = support::golden(case, "body.txt");
    (title, body.trim_end_matches('\n').to_owned())
}

/// What the port extracts, by the same route the reference took.
fn extracted(case: &str) -> Option<(Option<String>, String)> {
    let (bytes, format) = support::document(case)?;
    match format {
        SourceFormat::Text => Some(title::from_plain_text(
            &String::from_utf8_lossy(&bytes),
            false,
        )),
        SourceFormat::Markdown => Some(title::from_plain_text(
            &String::from_utf8_lossy(&bytes),
            true,
        )),
        SourceFormat::Docx => {
            let content = newsbuilder_core::import::docx::read(&bytes).ok()?;
            Some(title::from_docx_paragraphs(&content.paragraphs))
        }
    }
}

#[test]
fn every_case_detects_the_same_title() {
    let mut mismatches = Vec::new();
    for case in support::all_cases() {
        if !support::has_golden(&case, "title.txt") {
            continue;
        }
        let Some((title, _)) = extracted(&case) else {
            continue;
        };
        let (expected, _) = golden_title_and_body(&case);
        if title != expected {
            mismatches.push(format!("{case}: expected {expected:?}, got {title:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "title detection diverged:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn every_case_normalises_to_the_same_body() {
    let mut mismatches = Vec::new();
    for case in support::all_cases() {
        if !support::has_golden(&case, "body.txt") {
            continue;
        }
        let Some((_, body)) = extracted(&case) else {
            continue;
        };
        let (_, expected) = golden_title_and_body(&case);
        if body != expected {
            mismatches.push(case.clone());
            if mismatches.len() == 1 {
                assert_eq!(body, expected, "case `{case}` normalised differently");
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "body normalisation diverged: {mismatches:?}"
    );
}

#[test]
fn real_editorial_word_documents_read_correctly() {
    // The two cases that came from Word rather than from a generator. If the purpose-built
    // reader (research R3) is going to be fragile anywhere, it is here.
    for case in ["word-brsm", "word-den-znaniy"] {
        let (title, body) = extracted(case).unwrap_or_else(|| panic!("`{case}` should read"));
        let (expected_title, expected_body) = golden_title_and_body(case);
        assert_eq!(title, expected_title, "`{case}` title");
        assert_eq!(body, expected_body, "`{case}` body");
        assert!(!body.is_empty(), "`{case}` should have a body");
    }
}

#[test]
fn a_word_document_yields_its_embedded_images_in_order() {
    // FR-002 and FR-003, on a document Word itself wrote.
    let (bytes, _) = support::document("word-brsm").expect("the case is present");
    let content = newsbuilder_core::import::docx::read(&bytes).expect("it reads");
    assert!(
        content.media.len() >= 4,
        "expected several images, found {}",
        content.media.len()
    );
    let positions: Vec<usize> = content.media.iter().map(|m| m.after_paragraph).collect();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "media must come back in document order");
    for media in &content.media {
        assert!(
            !media.bytes.is_empty(),
            "`{}` came back empty",
            media.part_name
        );
    }
}

#[test]
fn a_word_part_with_a_misleading_extension_still_imports() {
    // word-den-znaniy carries `word/media/image2.tmp`, which is really a JPEG. Trusting the
    // extension would publish it as `.tmp`.
    let (bytes, _) = support::document("word-den-znaniy").expect("the case is present");
    let imported =
        newsbuilder_core::import::import_document(&bytes, SourceFormat::Docx).expect("it imports");
    assert!(
        imported
            .item
            .photos
            .iter()
            .all(|p| !p.file_name.ends_with(".tmp")),
        "a part's extension must not decide the published name: {:?}",
        imported
            .item
            .photos
            .iter()
            .map(|p| &p.file_name)
            .collect::<Vec<_>>()
    );
    assert!(!imported.item.photos.is_empty());
}

#[test]
fn a_table_is_skipped_with_a_warning_rather_than_a_failure() {
    // FR-034. The body golden for this case shows the table's cells absent, matching
    // python-docx's `document.paragraphs`.
    let (bytes, _) = support::document("word-unsupported").expect("the case is present");
    let imported = newsbuilder_core::import::import_document(&bytes, SourceFormat::Docx)
        .expect("a table must not fail the import");
    assert!(
        imported.warnings.iter().any(|w| matches!(
            w,
            newsbuilder_core::Warning::UnsupportedDocumentFeature { what } if what.contains("table")
        )),
        "expected a table warning, got {:?}",
        imported.warnings
    );
    assert!(
        !imported.item.body.is_empty(),
        "the paragraphs around the table still import"
    );
}

#[test]
fn every_case_parses_to_the_same_blocks() {
    let mut mismatches = Vec::new();
    for case in support::all_cases() {
        if !support::has_golden(&case, "blocks.json") {
            continue;
        }
        let Some((_, body)) = extracted(&case) else {
            continue;
        };
        let raw = support::golden(&case, "blocks.json");
        let expected: Vec<Value> = serde_json::from_str(&raw).expect("valid JSON");

        let (blocks, _) = split_like_import(&body);
        let produced: Vec<Value> = blocks
            .iter()
            .map(|block| match block {
                Block::Paragraph { text, .. } => {
                    serde_json::json!({"kind": "paragraph", "text": text})
                }
                Block::Placement { photos, layout } => serde_json::json!({
                    "kind": "placement",
                    "layout": layout.marker_keyword(),
                    "indices": photos.iter().map(|p| p.0).collect::<Vec<_>>(),
                }),
            })
            .collect();

        if produced != expected {
            mismatches.push(case.clone());
            if mismatches.len() == 1 {
                assert_eq!(
                    serde_json::to_string_pretty(&produced).unwrap_or_default(),
                    serde_json::to_string_pretty(&expected).unwrap_or_default(),
                    "case `{case}` parsed differently"
                );
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "block parsing diverged: {mismatches:?}"
    );
}

/// The same split `import` performs, exposed here so the comparison uses the real code path.
fn split_like_import(body: &str) -> (Vec<Block>, Vec<newsbuilder_core::Warning>) {
    use newsbuilder_core::markers::{self, ParsedBlock};
    use newsbuilder_core::model::photo::PhotoId;

    let (parsed, warnings) = markers::parse(body);
    let mut blocks = Vec::new();
    for element in parsed {
        match element {
            ParsedBlock::Text(text) => {
                blocks.extend(
                    plain::split_paragraphs(&text)
                        .into_iter()
                        .map(Block::paragraph),
                );
            }
            ParsedBlock::Marker(marker) => {
                let ids: Vec<PhotoId> = marker
                    .indices
                    .iter()
                    .map(|index| PhotoId(*index as u64))
                    .collect();
                blocks.push(Block::placement(ids, marker.layout));
            }
        }
    }
    (blocks, warnings)
}
