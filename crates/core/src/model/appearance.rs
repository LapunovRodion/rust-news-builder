//! Rendering and photo-budget settings.
//!
//! One appearance ships built in (FR-025). Its values are the reference's `DEFAULT_STYLES` and
//! `DEFAULT_CONFIG`, character for character — that identity is what lets the parity fixtures
//! compare like with like and what keeps the existing `style-presets/*.json` files loading
//! unmodified (constitution: Technology and Output Constraints).

/// One inline `style` attribute per rendering slot.
///
/// The field names match the reference's `DEFAULT_STYLES` keys exactly, which is what makes
/// preset files portable between the two implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSet {
    /// The wrapping `div`.
    pub container: String,
    /// The `h1`.
    pub title: String,
    /// Every paragraph after the first.
    pub paragraph: String,
    /// The first paragraph.
    pub lead: String,
    /// The full-width placement's wrapper.
    pub image_wrapper: String,
    /// The full-width placement's image.
    pub image: String,
    /// A row placement's wrapper.
    pub row_wrapper: String,
    /// One cell of a row.
    pub row_item: String,
    /// The image inside a row cell.
    pub row_image: String,
    /// A left-floated image.
    pub float_left: String,
    /// A right-floated image.
    pub float_right: String,
    /// The element that closes a float's text wrap.
    pub clear: String,
}

/// The sentinel the reference accepts in `container` to emit no wrapping `div` at all.
pub const CONTAINER_OMIT: &str = "__omit__";

/// The prefix the reference prepends to every non-omitted container style.
///
/// `build_container_style` in the reference emits this before the configured value, so it is
/// part of the rendered bytes even though it appears in no preset file.
pub const CONTAINER_BASE_STYLE: &str = "display: flow-root; width: 100%; box-sizing: border-box;";

impl StyleSet {
    /// The reference's `DEFAULT_STYLES` — the warm editorial look.
    #[must_use]
    pub fn built_in() -> Self {
        Self {
            container: concat!(
                "max-width: 920px; margin: 0 auto; padding: 34px 38px; ",
                "font-family: Georgia, 'Times New Roman', serif; color: #1d1d1d; line-height: 1.78; ",
                "background: linear-gradient(180deg, #fffdf9 0%, #f7f1e7 100%); ",
                "border: 1px solid #e7dcc8; border-radius: 26px; ",
                "box-shadow: 0 20px 50px rgba(76, 54, 28, 0.10);"
            )
            .to_owned(),
            title: concat!(
                "margin: 0 0 26px; font-size: 40px; line-height: 1.12; font-weight: 700; ",
                "letter-spacing: -0.03em; color: #24180d;"
            )
            .to_owned(),
            paragraph: "margin: 0 0 18px; font-size: 19px; color: #2c241c; text-indent: 1.8em;"
                .to_owned(),
            lead: concat!(
                "margin: 0 0 22px; font-size: 21px; color: #3f2f22; font-weight: 500; ",
                "text-indent: 0;"
            )
            .to_owned(),
            image_wrapper: "margin: 26px 0;".to_owned(),
            image: concat!(
                "display: block; width: 100%; height: auto; border-radius: 12px; ",
                "border: 1px solid rgba(91, 62, 33, 0.10); ",
                "box-shadow: 0 12px 28px rgba(53, 34, 16, 0.16);"
            )
            .to_owned(),
            row_wrapper: "margin: 28px 0; display: flex; gap: 14px; align-items: stretch;"
                .to_owned(),
            row_item: "flex: 1 1 0; min-width: 0;".to_owned(),
            row_image: concat!(
                "display: block; width: 100%; height: auto; border-radius: 12px; ",
                "border: 1px solid rgba(91, 62, 33, 0.10); ",
                "box-shadow: 0 10px 24px rgba(53, 34, 16, 0.14);"
            )
            .to_owned(),
            float_left: concat!(
                "float: left; width: 42%; max-width: 360px; margin: 8px 22px 14px 0; display: block; ",
                "border-radius: 16px; border: 1px solid rgba(91, 62, 33, 0.10); ",
                "box-shadow: 0 12px 28px rgba(53, 34, 16, 0.16);"
            )
            .to_owned(),
            float_right: concat!(
                "float: right; width: 42%; max-width: 360px; margin: 8px 0 14px 22px; display: block; ",
                "border-radius: 16px; border: 1px solid rgba(91, 62, 33, 0.10); ",
                "box-shadow: 0 12px 28px rgba(53, 34, 16, 0.16);"
            )
            .to_owned(),
            clear: "clear: both; height: 0; overflow: hidden;".to_owned(),
        }
    }

    /// The style the container `div` actually carries, or `None` when the `__omit__` sentinel
    /// says to emit no container at all. Reproduces the reference's `build_container_style`.
    #[must_use]
    pub fn container_style(&self) -> Option<String> {
        let cleaned = self.container.trim();
        if cleaned == CONTAINER_OMIT {
            return None;
        }
        if cleaned.is_empty() {
            return Some(CONTAINER_BASE_STYLE.to_owned());
        }
        let mut value = cleaned.to_owned();
        if !value.ends_with(';') {
            value.push(';');
        }
        Some(format!("{CONTAINER_BASE_STYLE} {value}"))
    }
}

/// Photo-budget settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageBudget {
    /// The longest edge a published photo may have. Photos are never scaled up.
    pub max_width: u32,
    /// The size a published photo must fit into.
    pub max_bytes: u64,
    /// The quality the JPEG search starts at.
    pub jpeg_quality: u8,
    /// The quality the JPEG search will not go below.
    pub jpeg_min_quality: u8,
    /// The quality the WebP search starts at.
    pub webp_quality: u8,
    /// The quality the WebP search will not go below.
    pub webp_min_quality: u8,
}

impl Default for ImageBudget {
    fn default() -> Self {
        Self::built_in()
    }
}

impl ImageBudget {
    /// The reference's `DEFAULT_CONFIG["image"]`. `max_bytes` is `500 * 1024`.
    #[must_use]
    pub fn built_in() -> Self {
        Self {
            max_width: 1600,
            max_bytes: 512_000,
            jpeg_quality: 85,
            jpeg_min_quality: 50,
            webp_quality: 85,
            webp_min_quality: 50,
        }
    }
}

/// Everything that affects what a build produces, other than the item itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appearance {
    /// The inline styles.
    pub styles: StyleSet,
    /// The photo budget.
    pub image: ImageBudget,
}

impl Default for Appearance {
    fn default() -> Self {
        Self::built_in()
    }
}

impl Appearance {
    /// The single shipped appearance (FR-025).
    #[must_use]
    pub fn built_in() -> Self {
        Self {
            styles: StyleSet::built_in(),
            image: ImageBudget::built_in(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Appearance, CONTAINER_BASE_STYLE, StyleSet};

    #[test]
    fn built_in_budget_matches_the_constitution() {
        let budget = Appearance::built_in().image;
        assert_eq!(budget.max_width, 1600);
        assert_eq!(budget.max_bytes, 512_000);
        assert_eq!(budget.jpeg_quality, 85);
        assert_eq!(budget.jpeg_min_quality, 50);
        assert_eq!(budget.webp_quality, 85);
        assert_eq!(budget.webp_min_quality, 50);
    }

    #[test]
    fn container_style_carries_the_reference_prefix() {
        let styles = StyleSet::built_in();
        let rendered = styles
            .container_style()
            .expect("built-in container is not omitted");
        assert!(rendered.starts_with(CONTAINER_BASE_STYLE));
        assert!(rendered.ends_with("box-shadow: 0 20px 50px rgba(76, 54, 28, 0.10);"));
    }

    #[test]
    fn the_omit_sentinel_removes_the_container() {
        let mut styles = StyleSet::built_in();
        styles.container = "  __omit__  ".to_owned();
        assert_eq!(styles.container_style(), None);
    }

    #[test]
    fn an_empty_container_style_still_gets_the_base() {
        let mut styles = StyleSet::built_in();
        styles.container = String::new();
        assert_eq!(
            styles.container_style().as_deref(),
            Some(CONTAINER_BASE_STYLE)
        );
    }

    #[test]
    fn a_container_style_without_a_trailing_semicolon_gains_one() {
        let mut styles = StyleSet::built_in();
        styles.container = "color: red".to_owned();
        assert_eq!(
            styles.container_style().as_deref(),
            Some("display: flow-root; width: 100%; box-sizing: border-box; color: red;")
        );
    }
}
