//! Plain-text and Markdown import, and the text normalisation every format goes through.
//!
//! The normalisation rules decide where paragraphs begin and end, so they are load-bearing for
//! parity: two implementations that disagree about a non-breaking space produce different
//! fragments. Each function here reproduces one of the reference's, and each is pinned by
//! `fixtures/reference/_tables/normalisation.json`.

/// Reproduces the reference's `normalize_text_content`.
///
/// In order: line endings to `\n`, non-breaking space to a plain space, zero-width space and
/// byte-order mark removed, runs of spaces and tabs collapsed to one space, runs of three or
/// more newlines collapsed to two, then trimmed.
#[must_use]
pub fn normalize_text_content(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    // Pass one: line endings and the invisible characters Word leaves behind.
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\u{00a0}' => out.push(' '),
            '\u{200b}' | '\u{feff}' => {}
            other => out.push(other),
        }
    }

    // Pass two: `[ \t]+` -> " ".
    let mut collapsed = String::with_capacity(out.len());
    let mut in_run = false;
    for ch in out.chars() {
        if ch == ' ' || ch == '\t' {
            if !in_run {
                collapsed.push(' ');
                in_run = true;
            }
        } else {
            in_run = false;
            collapsed.push(ch);
        }
    }

    // Pass three: `\n{3,}` -> "\n\n".
    let mut result = String::with_capacity(collapsed.len());
    let mut newline_run = 0usize;
    for ch in collapsed.chars() {
        if ch == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(ch);
            }
        } else {
            newline_run = 0;
            result.push(ch);
        }
    }

    result.trim().to_owned()
}

/// Reproduces the reference's `normalize_title`: normalised, then flattened to one line.
#[must_use]
pub fn normalize_title(value: &str) -> String {
    normalize_text_content(value)
        .replace('\n', " ")
        .trim()
        .to_owned()
}

/// Reproduces the reference's `normalize_body`.
#[must_use]
pub fn normalize_body(value: &str) -> String {
    normalize_text_content(value)
}

/// Reproduces the reference's `split_paragraphs`.
///
/// Splits on a blank line — `\n\s*\n`, greedily — then joins the surviving lines of each group
/// with a single space. That join is why a hard-wrapped Word paragraph does not become several
/// paragraphs in the output.
#[must_use]
pub fn split_paragraphs(chunk: &str) -> Vec<String> {
    let normalized = chunk.replace("\r\n", "\n");
    let mut paragraphs = Vec::new();
    for group in split_on_blank_lines(&normalized) {
        let lines: Vec<&str> = group
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if !lines.is_empty() {
            paragraphs.push(lines.join(" "));
        }
    }
    paragraphs
}

/// The `\n\s*\n` split, greedy, so a run of blank lines is one separator rather than several.
fn split_on_blank_lines(value: &str) -> Vec<String> {
    let chars: Vec<char> = value.chars().collect();
    let mut groups = Vec::new();
    let mut group_start = 0usize;
    let mut cursor = 0usize;

    while cursor < chars.len() {
        if chars[cursor] != '\n' {
            cursor += 1;
            continue;
        }
        // A separator is a newline, any run of whitespace, and a final newline. Take the
        // longest run available, matching Python's greedy quantifier.
        let mut scan = cursor + 1;
        let mut last_newline = None;
        while scan < chars.len() && chars[scan].is_whitespace() {
            if chars[scan] == '\n' {
                last_newline = Some(scan);
            }
            scan += 1;
        }
        match last_newline {
            Some(end) => {
                groups.push(chars[group_start..cursor].iter().collect::<String>());
                group_start = end + 1;
                cursor = end + 1;
            }
            None => cursor += 1,
        }
    }

    groups.push(chars[group_start..].iter().collect::<String>());
    groups
}

/// Reproduces the reference's `build_body_from_paragraphs`: normalise each paragraph, drop the
/// ones that normalise to nothing, and join with a blank line.
#[must_use]
pub fn build_body_from_paragraphs<I, S>(paragraphs: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    paragraphs
        .into_iter()
        .map(|value| normalize_text_content(value.as_ref()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Reproduces the reference's `extract_title_from_plain_text`.
///
/// The first non-blank line becomes the title and is removed from the body. In Markdown a
/// leading `# ` is stripped from it. A document with no non-blank line has neither a title nor
/// a body — the title is `None` and is never invented (FR-004).
#[must_use]
pub fn extract_title_and_body(raw: &str, is_markdown: bool) -> (Option<String>, String) {
    let lines: Vec<&str> = raw.lines().collect();
    let mut title = None;
    let mut title_index = None;

    for (idx, line) in lines.iter().enumerate() {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if is_markdown && stripped.starts_with("# ") {
            title = Some(stripped[2..].trim().to_owned());
        } else {
            title = Some(stripped.to_owned());
        }
        title_index = Some(idx);
        break;
    }

    let Some(title_index) = title_index else {
        return (None, String::new());
    };

    let body: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != title_index)
        .map(|(_, l)| *l)
        .collect();
    let body = body.join("\n");
    let body = body.trim();

    (title.as_deref().map(normalize_title), normalize_body(body))
}

#[cfg(test)]
mod tests {
    use super::{
        build_body_from_paragraphs, extract_title_and_body, normalize_text_content,
        normalize_title, split_paragraphs,
    };

    #[test]
    fn word_spacing_oddities_are_cleaned() {
        // Pinned by the reference's own test suite:
        // "Title  \n\n\nBody​   text" -> "Title \n\nBody text"
        assert_eq!(
            normalize_text_content("Title\u{00a0} \n\n\nBody\u{200b}   text"),
            "Title \n\nBody text"
        );
    }

    #[test]
    fn carriage_returns_become_newlines() {
        assert_eq!(normalize_text_content("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn a_byte_order_mark_is_removed() {
        assert_eq!(normalize_text_content("\u{feff}text"), "text");
    }

    #[test]
    fn long_blank_runs_collapse_to_one_blank_line() {
        assert_eq!(normalize_text_content("one\n\n\n\n\ntwo"), "one\n\ntwo");
    }

    #[test]
    fn a_title_is_flattened_to_one_line() {
        assert_eq!(normalize_title("Two\nLines"), "Two Lines");
    }

    #[test]
    fn paragraphs_split_on_blank_lines_and_join_their_wrapped_lines() {
        assert_eq!(
            split_paragraphs("a\nb\n\nc\nd"),
            vec!["a b".to_owned(), "c d".to_owned()]
        );
    }

    #[test]
    fn a_run_of_blank_lines_is_one_separator() {
        assert_eq!(
            split_paragraphs("a\n\n\n\nb"),
            vec!["a".to_owned(), "b".to_owned()]
        );
    }

    #[test]
    fn whitespace_only_input_yields_no_paragraphs() {
        assert!(split_paragraphs("\n\n\n").is_empty());
    }

    #[test]
    fn docx_paragraph_breaks_survive() {
        // Pinned by the reference's own test suite.
        assert_eq!(
            build_body_from_paragraphs(["Первый абзац", "Второй абзац", "", "Третий абзац"]),
            "Первый абзац\n\nВторой абзац\n\nТретий абзац"
        );
    }

    #[test]
    fn a_markdown_heading_becomes_the_title() {
        // Pinned by the reference's own test suite.
        let (title, body) =
            extract_title_and_body("# Hello\n\nParagraph one.\n\n[image:1]\n", true);
        assert_eq!(title.as_deref(), Some("Hello"));
        assert_eq!(body, "Paragraph one.\n\n[image:1]");
    }

    #[test]
    fn plain_text_takes_its_first_non_blank_line() {
        let (title, body) = extract_title_and_body("\n\n  Headline  \n\nBody.\n", false);
        assert_eq!(title.as_deref(), Some("Headline"));
        assert_eq!(body, "Body.");
    }

    #[test]
    fn a_hash_is_only_a_heading_in_markdown() {
        let (title, _) = extract_title_and_body("# Hello\n\nBody.\n", false);
        assert_eq!(title.as_deref(), Some("# Hello"));
    }

    #[test]
    fn an_empty_document_has_no_title() {
        // FR-004: never invent one.
        let (title, body) = extract_title_and_body("\n \n\t\n", false);
        assert_eq!(title, None);
        assert!(body.is_empty());
    }
}
