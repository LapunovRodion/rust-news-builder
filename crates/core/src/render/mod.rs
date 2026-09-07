//! The inline-style HTML fragment — the product's real deliverable.
//!
//! This is the strictest parity surface in the project: the goldens compare against it byte for
//! byte. Every rule below is the reference's `render_html`, including the ones
//! [contracts/html-output.md] does not mention.
//!
//! Four of those deserve naming, because the contract shows something different and a reader
//! coming from it will think this file is wrong:
//!
//! 1. Every image is wrapped in an `<a href>` opening the full-size photo in a new tab, and the
//!    `<img>` carries `loading="lazy"`. The contract shows a bare `<img>`.
//! 2. A full-width placement's wrapper is a `<p>`, not a `<div>`.
//! 3. `alt` is `"{title} - image {N}"`, not empty.
//! 4. The container style is prefixed with `display: flow-root; width: 100%; box-sizing:
//!    border-box;`, and `__omit__` drops the container entirely.
//!
//! The goldens are the specification of record (html-output.md says so itself), so the
//! reference wins. Deviation D-11 records the contract's errors.
//!
//! One line is *not* the reference's: the fragment opens with [`READMORE`], the marker the CMS
//! cuts the announcement at. It is deliberately outside the container, so the cut falls between
//! two complete elements rather than inside an unclosed `<div>`. The parity tests strip exactly
//! that first line before comparing, which is what keeps every other byte pinned.

use std::collections::BTreeMap;

use crate::model::appearance::StyleSet;
use crate::model::item::{Block, Layout, ParagraphKind};
use crate::model::photo::PhotoId;

/// The CMS's "read more" cut, the first line of every fragment.
///
/// Everything after it is the detail text; the announcement is whatever precedes it, which is
/// nothing — the whole item goes to the detail page, and the CMS builds the announcement from
/// its own fields. Kept as one constant because the parity tests strip this exact string.
pub const READMORE: &str = "<hr id=\"system-readmore\"/>";

/// What the renderer needs to know about one photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPhoto {
    /// The `src` and `href` the fragment carries.
    pub url: String,
    /// The photo's 1-based position in the item, which is what `alt` names.
    pub number: usize,
}

/// Renders the fragment.
///
/// Photos the map does not hold are left out; the caller has already warned about them
/// (FR-034), and a placement whose photos have all gone is dropped rather than rendered empty.
#[must_use]
pub fn render(
    title: &str,
    body: &[Block],
    styles: &StyleSet,
    photos: &BTreeMap<PhotoId, RenderedPhoto>,
) -> String {
    let container = styles.container_style();
    let mut lines: Vec<String> = vec![READMORE.to_owned()];

    if let Some(ref style) = container {
        lines.push(format!("<div style=\"{style}\">"));
    }
    lines.push(format!(
        "  <h1 style=\"{}\">{}</h1>",
        styles.title,
        escape_text(title)
    ));

    let mut active_float = false;

    for block in body {
        match block {
            // A paragraph deliberately does *not* clear the float: wrapping text around the
            // photo is the whole purpose of a float. Only the next image closes the wrap.
            Block::Paragraph { text, kind } => {
                let style = match kind {
                    ParagraphKind::Lead => &styles.lead,
                    ParagraphKind::Body => &styles.paragraph,
                };
                lines.push(format!("  <p style=\"{style}\">{}</p>", escape_text(text)));
            }

            Block::Placement {
                photos: ids,
                layout,
            } => {
                let resolved: Vec<&RenderedPhoto> =
                    ids.iter().filter_map(|id| photos.get(id)).collect();
                if resolved.is_empty() {
                    continue;
                }

                // A float's text wrap is closed by the next non-floating *placement*, or by
                // the end of the fragment — never by the paragraphs meant to wrap it.
                if active_float && !layout.floats() {
                    lines.push(format!("  <div style=\"{}\"></div>", styles.clear));
                    active_float = false;
                }

                match layout {
                    Layout::FullWidth => {
                        lines.push(format!("  <p style=\"{}\">", styles.image_wrapper));
                        lines.extend(anchor(resolved[0], title, &styles.image, "    "));
                        lines.push("  </p>".to_owned());
                    }
                    Layout::Row => {
                        lines.push(format!("  <div style=\"{}\">", styles.row_wrapper));
                        for photo in resolved {
                            lines.push(format!("    <div style=\"{}\">", styles.row_item));
                            lines.extend(anchor(photo, title, &styles.row_image, "      "));
                            lines.push("    </div>".to_owned());
                        }
                        lines.push("  </div>".to_owned());
                    }
                    Layout::FloatLeft | Layout::FloatRight => {
                        let style = if *layout == Layout::FloatLeft {
                            &styles.float_left
                        } else {
                            &styles.float_right
                        };
                        lines.extend(anchor(resolved[0], title, style, "  "));
                        active_float = true;
                    }
                }
            }
        }
    }

    if active_float {
        lines.push(format!("  <div style=\"{}\"></div>", styles.clear));
    }
    if container.is_some() {
        lines.push("</div>".to_owned());
    }

    let mut fragment = lines.join("\n");
    fragment.push('\n');
    fragment
}

/// The reference's `openable_image_tag`: three lines, at the given indent.
fn anchor(photo: &RenderedPhoto, title: &str, image_style: &str, indent: &str) -> Vec<String> {
    let url = escape_attribute(&photo.url);
    let alt = escape_attribute(&format!("{title} - image {}", photo.number));
    vec![
        format!(
            "{indent}<a href=\"{url}\" target=\"_blank\" rel=\"noopener noreferrer\" \
             style=\"text-decoration: none;\">"
        ),
        format!(
            "{indent}  <img src=\"{url}\" alt=\"{alt}\" style=\"{image_style}\" \
             loading=\"lazy\" />"
        ),
        format!("{indent}</a>"),
    ]
}

/// Python's `html.escape(value, quote=False)`: the three structural characters only.
#[must_use]
pub fn escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Python's `html.escape(value, quote=True)`, which also escapes both quote characters.
///
/// The apostrophe becomes `&#x27;`, not `&apos;` — Python's spelling, and the goldens'.
#[must_use]
pub fn escape_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{READMORE, RenderedPhoto, escape_attribute, escape_text, render};
    use crate::model::appearance::StyleSet;
    use crate::model::item::{Block, Layout};
    use crate::model::photo::PhotoId;
    use std::collections::BTreeMap;

    #[test]
    fn every_fragment_opens_with_the_cms_cut_marker() {
        // Outside the container, so cutting there leaves both halves well-formed.
        let out = render("День знаний", &[], &StyleSet::built_in(), &BTreeMap::new());
        let mut lines = out.lines();
        assert_eq!(lines.next(), Some(READMORE));
        assert!(
            lines
                .next()
                .is_some_and(|line| line.starts_with("<div style=")),
            "the marker is inside the container:\n{out}"
        );
    }

    fn photos(count: usize) -> BTreeMap<PhotoId, RenderedPhoto> {
        (1..=count)
            .map(|n| {
                (
                    PhotoId(n as u64),
                    RenderedPhoto {
                        url: format!("https://e.test/x-{n:02}.jpg"),
                        number: n,
                    },
                )
            })
            .collect()
    }

    fn body_with(layout: Layout, ids: &[u64]) -> Vec<Block> {
        let mut body = vec![Block::paragraph("Before.")];
        body.push(Block::placement(
            ids.iter().map(|n| PhotoId(*n)).collect(),
            layout,
        ));
        body.push(Block::paragraph("After."));
        let mut item = crate::model::item::NewsItem::new();
        item.body = body;
        item.normalise_paragraph_kinds();
        item.body
    }

    #[test]
    fn escaping_follows_pythons_two_modes() {
        assert_eq!(
            escape_text("a & b < c > d \" e ' f"),
            "a &amp; b &lt; c &gt; d \" e ' f"
        );
        assert_eq!(
            escape_attribute("a & b < c > d \" e ' f"),
            "a &amp; b &lt; c &gt; d &quot; e &#x27; f"
        );
    }

    #[test]
    fn the_fragment_carries_no_forbidden_construct() {
        // FR-023 and the html-output contract's hard rules.
        let fragment = render(
            "Title",
            &body_with(Layout::Row, &[1, 2]),
            &StyleSet::built_in(),
            &photos(2),
        );
        for forbidden in [
            "<style",
            "</style",
            "<link",
            "<script",
            "class=",
            "<!doctype",
            "<html",
            "<body",
        ] {
            assert!(
                !fragment.to_ascii_lowercase().contains(forbidden),
                "fragment contains {forbidden}"
            );
        }
    }

    #[test]
    fn every_image_is_wrapped_in_an_opening_anchor() {
        // Not in the contract, but the reference does it and the goldens pin it.
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[1]),
            &StyleSet::built_in(),
            &photos(1),
        );
        assert!(fragment.contains(
            r#"<a href="https://e.test/x-01.jpg" target="_blank" rel="noopener noreferrer" style="text-decoration: none;">"#
        ));
        assert!(fragment.contains(r#"loading="lazy" />"#));
    }

    #[test]
    fn a_full_width_placement_is_wrapped_in_a_paragraph() {
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[1]),
            &StyleSet::built_in(),
            &photos(1),
        );
        assert!(fragment.contains(&format!(
            "  <p style=\"{}\">\n",
            StyleSet::built_in().image_wrapper
        )));
    }

    #[test]
    fn alt_text_names_the_title_and_the_photo_number() {
        let fragment = render(
            "Марш",
            &body_with(Layout::Row, &[1, 2]),
            &StyleSet::built_in(),
            &photos(2),
        );
        assert!(fragment.contains(r#"alt="Марш - image 1""#));
        assert!(fragment.contains(r#"alt="Марш - image 2""#));
    }

    #[test]
    fn a_paragraph_does_not_clear_a_float() {
        // Wrapping text around the photo is the point of a float, so the paragraph after one
        // must keep wrapping. The reference only checks for a clear in its image branch.
        let fragment = render(
            "T",
            &body_with(Layout::FloatLeft, &[1]),
            &StyleSet::built_in(),
            &photos(1),
        );
        let clear = format!("  <div style=\"{}\"></div>", StyleSet::built_in().clear);
        assert_eq!(
            fragment.matches(&clear).count(),
            1,
            "only the closing clear"
        );
        let clear_at = fragment.find(&clear).expect("clear present");
        let after_at = fragment.find("After.").expect("trailing paragraph present");
        assert!(after_at < clear_at, "the wrap must survive the paragraph");
    }

    #[test]
    fn the_next_image_clears_the_float() {
        // Pinned by fixtures/reference/markers: the clear sits between the wrapped paragraph
        // and the full-width photo that follows it.
        let mut item = crate::model::item::NewsItem::new();
        item.body = vec![
            Block::placement(vec![PhotoId(1)], Layout::FloatRight),
            Block::paragraph("Wrapped."),
            Block::placement(vec![PhotoId(2)], Layout::FullWidth),
        ];
        item.normalise_paragraph_kinds();
        let styles = StyleSet::built_in();
        let fragment = render("T", &item.body, &styles, &photos(2));
        let clear = format!("  <div style=\"{}\"></div>", styles.clear);
        assert_eq!(
            fragment.matches(&clear).count(),
            1,
            "cleared once, not twice"
        );
        let clear_at = fragment.find(&clear).expect("clear present");
        let wrapped_at = fragment.find("Wrapped.").expect("paragraph present");
        let image_wrapper_at = fragment
            .find(&format!("  <p style=\"{}\">", styles.image_wrapper))
            .expect("the full-width placement present");
        assert!(
            wrapped_at < clear_at,
            "the paragraph wraps before the clear"
        );
        assert!(
            clear_at < image_wrapper_at,
            "the clear precedes the next image"
        );
    }

    #[test]
    fn a_trailing_float_is_cleared_at_the_end() {
        let mut item = crate::model::item::NewsItem::new();
        item.body = vec![
            Block::paragraph("Text."),
            Block::placement(vec![PhotoId(1)], Layout::FloatRight),
        ];
        item.normalise_paragraph_kinds();
        let fragment = render("T", &item.body, &StyleSet::built_in(), &photos(1));
        let clear = format!("  <div style=\"{}\"></div>", StyleSet::built_in().clear);
        assert_eq!(fragment.matches(&clear).count(), 1);
        assert!(fragment.trim_end().ends_with("</div>"));
    }

    #[test]
    fn consecutive_floats_are_not_cleared_between_each_other() {
        let mut item = crate::model::item::NewsItem::new();
        item.body = vec![
            Block::placement(vec![PhotoId(1)], Layout::FloatLeft),
            Block::placement(vec![PhotoId(2)], Layout::FloatRight),
        ];
        let fragment = render("T", &item.body, &StyleSet::built_in(), &photos(2));
        let clear = format!("  <div style=\"{}\"></div>", StyleSet::built_in().clear);
        assert_eq!(
            fragment.matches(&clear).count(),
            1,
            "only the closing clear"
        );
    }

    #[test]
    fn the_first_paragraph_uses_the_lead_style() {
        let styles = StyleSet::built_in();
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[1]),
            &styles,
            &photos(1),
        );
        let lead_at = fragment.find(&styles.lead).expect("lead style used");
        let body_at = fragment.find(&styles.paragraph).expect("body style used");
        assert!(lead_at < body_at);
    }

    #[test]
    fn the_omit_sentinel_removes_the_wrapping_div() {
        let mut styles = StyleSet::built_in();
        styles.container = "__omit__".to_owned();
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[1]),
            &styles,
            &photos(1),
        );
        assert!(
            fragment.starts_with(&format!("{READMORE}\n  <h1 ")),
            "unexpected start: {fragment:.80}"
        );
        assert!(!fragment.trim_end().ends_with("</div>\n</div>"));
    }

    #[test]
    fn a_placement_whose_photos_are_all_missing_is_dropped() {
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[99]),
            &StyleSet::built_in(),
            &photos(1),
        );
        assert!(!fragment.contains("<img"), "nothing to render: {fragment}");
        assert!(
            fragment.contains("Before."),
            "the surrounding text still renders"
        );
    }

    #[test]
    fn a_row_renders_only_the_photos_that_resolve() {
        let fragment = render(
            "T",
            &body_with(Layout::Row, &[1, 99, 2]),
            &StyleSet::built_in(),
            &photos(2),
        );
        assert_eq!(fragment.matches("<img").count(), 2);
    }

    #[test]
    fn the_fragment_ends_with_exactly_one_newline() {
        let fragment = render(
            "T",
            &body_with(Layout::FullWidth, &[1]),
            &StyleSet::built_in(),
            &photos(1),
        );
        assert!(fragment.ends_with(">\n"));
        assert!(!fragment.ends_with("\n\n"));
    }

    #[test]
    fn rendering_is_deterministic() {
        // Constitution IV: same input, same bytes, every time.
        let body = body_with(Layout::Row, &[1, 2]);
        let first = render("T", &body, &StyleSet::built_in(), &photos(2));
        for _ in 0..5 {
            assert_eq!(render("T", &body, &StyleSet::built_in(), &photos(2)), first);
        }
    }
}
