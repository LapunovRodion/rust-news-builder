//! Headline detection (FR-004).
//!
//! A title is *detected*, never invented. When no headline can be found the caller is told so
//! and asks the editor, which is why every function here returns an `Option`.

use crate::import::plain::{build_body_from_paragraphs, extract_title_and_body, normalize_title};

/// The title and body of a plain-text or Markdown document.
///
/// In Markdown a leading `# ` on the first non-blank line is stripped; otherwise that line is
/// taken as it stands. Either way the line is removed from the body, so a title never appears
/// twice (FR-004).
#[must_use]
pub fn from_plain_text(raw: &str, is_markdown: bool) -> (Option<String>, String) {
    extract_title_and_body(raw, is_markdown)
}

/// The title and body of a Word document, from its paragraphs in order.
///
/// Reproduces `read_docx_with_python_docx`. Its heading-style branch and its first-paragraph
/// branch have the same effect — both fire on the first non-blank paragraph — so the rule is
/// simply: the first paragraph with text is the title.
///
/// Blank paragraphs become separators, but only where they follow something, and never two in
/// a row. That is what stops a document's leading blank lines from opening the body with a gap.
#[must_use]
pub fn from_docx_paragraphs<S: AsRef<str>>(paragraphs: &[S]) -> (Option<String>, String) {
    let mut title: Option<String> = None;
    let mut body_lines: Vec<String> = Vec::new();

    for paragraph in paragraphs {
        let text = paragraph.as_ref().trim();
        if text.is_empty() {
            if body_lines.last().is_some_and(|last| !last.is_empty()) {
                body_lines.push(String::new());
            }
            continue;
        }

        let Some(found) = title.as_deref() else {
            title = Some(text.to_owned());
            continue;
        };

        // A document that repeats its headline as the first body line drops the repeat.
        if text == found && body_lines.is_empty() {
            continue;
        }
        body_lines.push(text.to_owned());
    }

    (
        title.as_deref().map(normalize_title),
        build_body_from_paragraphs(&body_lines),
    )
}

#[cfg(test)]
mod tests {
    use super::{from_docx_paragraphs, from_plain_text};

    #[test]
    fn a_headline_as_the_first_line_is_the_title() {
        let (title, body) = from_plain_text("Headline\n\nBody.\n", false);
        assert_eq!(title.as_deref(), Some("Headline"));
        assert_eq!(body, "Body.");
    }

    #[test]
    fn a_markdown_heading_is_the_title_without_its_hash() {
        let (title, _) = from_plain_text("# Heading\n\nBody.\n", true);
        assert_eq!(title.as_deref(), Some("Heading"));
    }

    #[test]
    fn a_docx_heading_styled_paragraph_is_the_title() {
        // fixtures/reference/word-with-photos pins this.
        let (title, body) = from_docx_paragraphs(&[
            "Word With Photos",
            "The lead paragraph of a Word document.",
            "A second paragraph.",
        ]);
        assert_eq!(title.as_deref(), Some("Word With Photos"));
        assert_eq!(
            body,
            "The lead paragraph of a Word document.\n\nA second paragraph."
        );
    }

    #[test]
    fn a_docx_with_no_heading_style_uses_its_first_paragraph() {
        // fixtures/reference/word-plain-title pins this.
        let (title, body) = from_docx_paragraphs(&[
            "A Plain First Paragraph As Title",
            "The body starts here.",
            "And continues here.",
        ]);
        assert_eq!(title.as_deref(), Some("A Plain First Paragraph As Title"));
        assert_eq!(body, "The body starts here.\n\nAnd continues here.");
    }

    #[test]
    fn a_document_with_no_text_has_no_title() {
        // FR-004: return None rather than invent one.
        assert_eq!(from_docx_paragraphs::<&str>(&[]).0, None);
        assert_eq!(from_docx_paragraphs(&["", "   ", "\t"]).0, None);
        assert_eq!(from_plain_text("", false).0, None);
    }

    #[test]
    fn leading_blank_paragraphs_do_not_open_the_body_with_a_gap() {
        let (_, body) = from_docx_paragraphs(&["Title", "", "", "First.", "", "Second."]);
        assert_eq!(body, "First.\n\nSecond.");
    }

    #[test]
    fn a_repeated_headline_is_dropped_once() {
        let (title, body) = from_docx_paragraphs(&["Headline", "Headline", "Body."]);
        assert_eq!(title.as_deref(), Some("Headline"));
        assert_eq!(body, "Body.");
    }

    #[test]
    fn a_headline_repeated_later_is_kept() {
        // Only the immediate repeat is a duplicated title; a later one is prose.
        let (_, body) = from_docx_paragraphs(&["Headline", "Body.", "Headline"]);
        assert_eq!(body, "Body.\n\nHeadline");
    }
}
