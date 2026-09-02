//! The frozen marker language (FR-005), parsed.
//!
//! This module reproduces the reference's `MARKER_PATTERN` and `parse_marker_block` semantics:
//!
//! ```text
//! \[(image|images|image-left|image-right):([0-9,\s]+)\]
//! ```
//!
//! **Parse only.** There is deliberately no emitter. Marker typing was dropped in favour of
//! inline placement cards (deviation D-9), so the editor works on structure and never sees
//! marker text. Nothing downstream of import produces or consumes it.
//!
//! Where the reference raises, this module warns and keeps the marker's text as literal prose
//! (FR-034). No input is ever silently dropped.

use crate::error::Warning;
use crate::model::item::Layout;

/// One element of a parsed body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedBlock {
    /// A run of prose, not yet split into paragraphs.
    Text(String),
    /// A marker that parsed cleanly.
    Marker(Marker),
}

/// A marker the reference would accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// The layout the keyword names.
    pub layout: Layout,
    /// The photo numbers, 1-based, in the order written. Duplicates are kept: the reference
    /// accepts `[images:1,1]` and renders the photo twice.
    pub indices: Vec<usize>,
    /// The marker exactly as it appeared, for warning text (SC-007).
    pub raw: String,
}

/// The four keywords, ordered so that a longer keyword is tried before a prefix of it.
///
/// Python's alternation is leftmost-first, so `image|images|...` only matches `images` after
/// `image` fails to be followed by a colon. Trying the longest first is equivalent and does
/// not need backtracking.
const KEYWORDS: [(&str, Layout); 4] = [
    ("image-left", Layout::FloatLeft),
    ("image-right", Layout::FloatRight),
    ("images", Layout::Row),
    ("image", Layout::FullWidth),
];

/// Splits a body into prose and markers, in order.
///
/// Returns the blocks alongside any marker the reference would have rejected. A rejected
/// marker stays in the prose exactly as written, so re-importing is lossless.
#[must_use]
pub fn parse(body: &str) -> (Vec<ParsedBlock>, Vec<Warning>) {
    let chars: Vec<char> = body.chars().collect();
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut prose_start = 0usize;
    let mut cursor = 0usize;

    while cursor < chars.len() {
        if chars[cursor] != '[' {
            cursor += 1;
            continue;
        }
        let Some((end, keyword_layout, payload)) = match_marker(&chars, cursor) else {
            cursor += 1;
            continue;
        };
        let raw: String = chars[cursor..end].iter().collect();

        match indices_from(&payload, keyword_layout) {
            Ok(indices) => {
                push_text(&mut blocks, &chars[prose_start..cursor]);
                blocks.push(ParsedBlock::Marker(Marker {
                    layout: keyword_layout,
                    indices,
                    raw,
                }));
                prose_start = end;
            }
            Err(reason) => {
                // The reference raises here and abandons the document. Keeping the text and
                // warning is what FR-034 asks for, and it means an editor sees the problem
                // without losing the rest of their work.
                warnings.push(Warning::MalformedMarker {
                    marker: raw,
                    reason,
                });
            }
        }
        cursor = end;
    }

    push_text(&mut blocks, &chars[prose_start..]);
    (blocks, warnings)
}

fn push_text(blocks: &mut Vec<ParsedBlock>, chars: &[char]) {
    if !chars.is_empty() {
        blocks.push(ParsedBlock::Text(chars.iter().collect()));
    }
}

/// Matches `[keyword:payload]` starting at `start`, which is known to be `[`.
///
/// Returns the index one past the closing bracket, the layout, and the raw payload.
fn match_marker(chars: &[char], start: usize) -> Option<(usize, Layout, String)> {
    let after_bracket = start + 1;
    let (layout, mut cursor) = KEYWORDS.iter().find_map(|(keyword, layout)| {
        let end = after_bracket + keyword.chars().count();
        let matches = chars.len() > end
            && chars[after_bracket..end]
                .iter()
                .copied()
                .eq(keyword.chars())
            && chars[end] == ':';
        matches.then_some((*layout, end + 1))
    })?;

    // `[0-9,\s]+` — at least one payload character.
    let payload_start = cursor;
    while cursor < chars.len()
        && (chars[cursor].is_ascii_digit() || chars[cursor] == ',' || chars[cursor].is_whitespace())
    {
        cursor += 1;
    }
    if cursor == payload_start || cursor >= chars.len() || chars[cursor] != ']' {
        return None;
    }

    Some((
        cursor + 1,
        layout,
        chars[payload_start..cursor].iter().collect(),
    ))
}

/// Reproduces `parse_marker_block`: split on commas, drop blank parts, parse the rest.
fn indices_from(payload: &str, layout: Layout) -> Result<Vec<usize>, String> {
    let mut indices = Vec::new();
    for part in payload.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed
            .parse::<usize>()
            .map_err(|_| format!("`{trimmed}` is not a photo number"))?;
        indices.push(value);
    }

    if indices.is_empty() {
        return Err("it names no photo numbers".to_owned());
    }
    if !layout.holds_many() && indices.len() != 1 {
        return Err(format!(
            "[{}:N] takes exactly one photo number, but {} were given",
            layout.marker_keyword(),
            indices.len()
        ));
    }
    Ok(indices)
}

#[cfg(test)]
mod tests {
    use super::{Marker, ParsedBlock, parse};
    use crate::error::Warning;
    use crate::model::item::Layout;

    fn markers(body: &str) -> Vec<Marker> {
        parse(body)
            .0
            .into_iter()
            .filter_map(|block| match block {
                ParsedBlock::Marker(marker) => Some(marker),
                ParsedBlock::Text(_) => None,
            })
            .collect()
    }

    fn text(body: &str) -> Vec<String> {
        parse(body)
            .0
            .into_iter()
            .filter_map(|block| match block {
                ParsedBlock::Text(value) => Some(value),
                ParsedBlock::Marker(_) => None,
            })
            .collect()
    }

    #[test]
    fn all_four_forms_parse() {
        assert_eq!(markers("[image:1]")[0].layout, Layout::FullWidth);
        assert_eq!(markers("[images:1,2]")[0].layout, Layout::Row);
        assert_eq!(markers("[image-left:1]")[0].layout, Layout::FloatLeft);
        assert_eq!(markers("[image-right:1]")[0].layout, Layout::FloatRight);
    }

    #[test]
    fn a_longer_keyword_wins_over_its_prefix() {
        // `image` is a prefix of `images` and of `image-left`; the colon is what decides.
        assert_eq!(markers("[images:1,2]")[0].indices, vec![1, 2]);
        assert_eq!(markers("[image-right:3]")[0].layout, Layout::FloatRight);
    }

    #[test]
    fn whitespace_inside_the_payload_is_allowed() {
        // Pinned by fixtures/reference/malformed-markers: `[images: 1 , 1 ]` is valid and
        // keeps the duplicate.
        assert_eq!(markers("[images: 1 , 1 ]")[0].indices, vec![1, 1]);
    }

    #[test]
    fn a_row_of_one_is_not_normalised() {
        // fixtures/reference/row-of-one pins this: the reference leaves `[images:1]` a row.
        // data-model.md's INV-4 claims it is normalised to full width; the golden disagrees.
        let marker = &markers("[images:1]")[0];
        assert_eq!(marker.layout, Layout::Row);
        assert_eq!(marker.indices, vec![1]);
    }

    #[test]
    fn marker_shaped_text_that_does_not_match_stays_prose() {
        // All four are pinned by fixtures/reference/malformed-markers.
        let body = "[image:] and [picture:1] and [image 1] and [IMAGE:1]";
        assert!(markers(body).is_empty());
        assert_eq!(text(body), vec![body.to_owned()]);
    }

    #[test]
    fn positions_are_preserved() {
        let (blocks, warnings) = parse("First.\n\n[image:1]\n\nSecond.");
        assert!(warnings.is_empty());
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[0], ParsedBlock::Text(_)));
        assert!(matches!(blocks[1], ParsedBlock::Marker(_)));
        assert!(matches!(blocks[2], ParsedBlock::Text(_)));
    }

    #[test]
    fn a_single_photo_layout_given_several_warns_and_keeps_the_text() {
        // The reference raises ValueError and abandons the document; FR-034 says warn.
        let (blocks, warnings) = parse("Before [image:1,2] after");
        assert_eq!(
            blocks,
            vec![ParsedBlock::Text("Before [image:1,2] after".to_owned())]
        );
        assert!(matches!(
            &warnings[..],
            [Warning::MalformedMarker { marker, .. }] if marker == "[image:1,2]"
        ));
    }

    #[test]
    fn a_payload_of_only_separators_warns() {
        let (_, warnings) = parse("[image: , ]");
        assert!(matches!(&warnings[..], [Warning::MalformedMarker { .. }]));
    }

    #[test]
    fn the_warning_names_the_marker() {
        // SC-007: an editor must be able to find the thing that is wrong.
        let (_, warnings) = parse("[image-left:1,2]");
        let Some(Warning::MalformedMarker { marker, reason }) = warnings.first() else {
            unreachable!("expected exactly one malformed-marker warning, got {warnings:?}")
        };
        assert_eq!(marker, "[image-left:1,2]");
        assert!(
            reason.contains("image-left"),
            "reason should name the layout: {reason}"
        );
    }

    #[test]
    fn an_unterminated_marker_is_prose() {
        assert!(markers("[image:1").is_empty());
        assert_eq!(text("[image:1"), vec!["[image:1".to_owned()]);
    }

    #[test]
    fn consecutive_markers_leave_no_empty_text_block() {
        let (blocks, _) = parse("[image:1][image:2]");
        assert_eq!(
            blocks.len(),
            2,
            "no empty Text block between them: {blocks:?}"
        );
    }
}
