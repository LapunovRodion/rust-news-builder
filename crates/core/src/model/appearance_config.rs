//! Loading an appearance from the reference's JSON schema.
//!
//! One appearance ships built in (FR-025), but the schema stays the reference's so the
//! existing `style-presets/*.json` files load unmodified — which the constitution's Technology
//! and Output Constraints require even though the product no longer offers a menu of presets.
//!
//! Overrides apply key by key, not wholesale: setting `image.max_width` leaves every style
//! string at its built-in value.

use serde_json::Value;

use crate::error::{Error, Result, Warning};
use crate::model::appearance::{Appearance, ImageBudget, StyleSet};

/// An appearance plus whatever the file said that could not be used.
#[derive(Debug, Clone)]
pub struct LoadedAppearance {
    /// The appearance, with the file's overrides applied over the built-in values.
    pub appearance: Appearance,
    /// Unknown keys, which are ignored so that older and newer preset files still load.
    pub warnings: Vec<Warning>,
}

/// Applies a configuration file's overrides to `base`.
///
/// Rejects — naming the field — a quality pair that is inverted or out of range, a
/// `max_width` below 1, a `max_bytes` below 1024, and any style string that could break out of
/// the `style` attribute it is emitted into.
pub fn apply(base: &Appearance, json: &str) -> Result<LoadedAppearance> {
    let root: Value = serde_json::from_str(json).map_err(|e| Error::InvalidAppearance {
        detail: format!("the file is not valid JSON: {e}"),
    })?;

    let Value::Object(root) = root else {
        return Err(Error::InvalidAppearance {
            detail: "the file's top level must be a JSON object".to_owned(),
        });
    };

    let mut appearance = base.clone();
    let mut warnings = Vec::new();

    for (key, value) in &root {
        match key.as_str() {
            "image" => apply_image(&mut appearance.image, value, &mut warnings)?,
            "styles" => apply_styles(&mut appearance.styles, value, &mut warnings)?,
            other => warnings.push(Warning::UnknownAppearanceKey {
                key: other.to_owned(),
            }),
        }
    }

    validate(&appearance.image)?;
    Ok(LoadedAppearance {
        appearance,
        warnings,
    })
}

/// The built-in appearance with a file's overrides applied.
pub fn load(json: &str) -> Result<LoadedAppearance> {
    apply(&Appearance::built_in(), json)
}

fn apply_image(budget: &mut ImageBudget, value: &Value, warnings: &mut Vec<Warning>) -> Result<()> {
    let Value::Object(fields) = value else {
        return Err(Error::InvalidAppearance {
            detail: "`image` must be a JSON object".to_owned(),
        });
    };

    for (key, raw) in fields {
        match key.as_str() {
            "max_width" => budget.max_width = number(raw, "image.max_width")? as u32,
            "max_bytes" => budget.max_bytes = number(raw, "image.max_bytes")?,
            "jpeg_quality" => budget.jpeg_quality = quality(raw, "image.jpeg_quality")?,
            "jpeg_min_quality" => budget.jpeg_min_quality = quality(raw, "image.jpeg_min_quality")?,
            "webp_quality" => budget.webp_quality = quality(raw, "image.webp_quality")?,
            "webp_min_quality" => budget.webp_min_quality = quality(raw, "image.webp_min_quality")?,
            other => {
                warnings.push(Warning::UnknownAppearanceKey {
                    key: format!("image.{other}"),
                });
            }
        }
    }
    Ok(())
}

fn apply_styles(styles: &mut StyleSet, value: &Value, warnings: &mut Vec<Warning>) -> Result<()> {
    let Value::Object(fields) = value else {
        return Err(Error::InvalidAppearance {
            detail: "`styles` must be a JSON object".to_owned(),
        });
    };

    for (key, raw) in fields {
        let Some(text) = raw.as_str() else {
            return Err(Error::InvalidAppearance {
                detail: format!("`styles.{key}` must be a string"),
            });
        };
        reject_breakout(key, text)?;

        let slot: &mut String = match key.as_str() {
            "container" => &mut styles.container,
            "title" => &mut styles.title,
            "paragraph" => &mut styles.paragraph,
            "lead" => &mut styles.lead,
            "image_wrapper" => &mut styles.image_wrapper,
            "image" => &mut styles.image,
            "row_wrapper" => &mut styles.row_wrapper,
            "row_item" => &mut styles.row_item,
            "row_image" => &mut styles.row_image,
            "float_left" => &mut styles.float_left,
            "float_right" => &mut styles.float_right,
            "clear" => &mut styles.clear,
            other => {
                warnings.push(Warning::UnknownAppearanceKey {
                    key: format!("styles.{other}"),
                });
                continue;
            }
        };
        *slot = text.to_owned();
    }
    Ok(())
}

/// A style string is emitted verbatim into a `style="..."` attribute. These two sequences are
/// the ones that would end the attribute or the element, so they are refused rather than
/// escaped: silently rewriting an editor's CSS would be worse than telling them.
fn reject_breakout(key: &str, value: &str) -> Result<()> {
    let lowered = value.to_ascii_lowercase();
    if lowered.contains("</") || lowered.contains("<script") {
        return Err(Error::InvalidAppearance {
            detail: format!(
                "`styles.{key}` contains markup, which would break the style attribute"
            ),
        });
    }
    Ok(())
}

fn number(value: &Value, field: &str) -> Result<u64> {
    value.as_u64().ok_or_else(|| Error::InvalidAppearance {
        detail: format!("`{field}` must be a non-negative whole number"),
    })
}

fn quality(value: &Value, field: &str) -> Result<u8> {
    let raw = number(value, field)?;
    u8::try_from(raw)
        .ok()
        .filter(|v| (1..=100).contains(v))
        .ok_or_else(|| Error::InvalidAppearance {
            detail: format!("`{field}` must be between 1 and 100"),
        })
}

/// INV-7 and the contract's size floors.
fn validate(budget: &ImageBudget) -> Result<()> {
    if budget.jpeg_min_quality > budget.jpeg_quality {
        return Err(Error::InvalidAppearance {
            detail: "`image.jpeg_min_quality` must not exceed `image.jpeg_quality`".to_owned(),
        });
    }
    if budget.webp_min_quality > budget.webp_quality {
        return Err(Error::InvalidAppearance {
            detail: "`image.webp_min_quality` must not exceed `image.webp_quality`".to_owned(),
        });
    }
    if budget.max_width < 1 {
        return Err(Error::InvalidAppearance {
            detail: "`image.max_width` must be at least 1".to_owned(),
        });
    }
    if budget.max_bytes < 1024 {
        return Err(Error::InvalidAppearance {
            detail: "`image.max_bytes` must be at least 1024".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::load;
    use crate::error::Warning;
    use crate::model::appearance::{Appearance, StyleSet};

    #[test]
    fn an_empty_object_leaves_the_built_in_appearance_alone() {
        let loaded = load("{}").expect("an empty object is valid");
        assert_eq!(loaded.appearance, Appearance::built_in());
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn overrides_apply_key_by_key() {
        // The contract's resolution rule: one override must not reset everything else.
        let loaded = load(r#"{"image": {"max_width": 800}}"#).expect("valid");
        assert_eq!(loaded.appearance.image.max_width, 800);
        assert_eq!(loaded.appearance.image.max_bytes, 512_000);
        assert_eq!(loaded.appearance.styles, StyleSet::built_in());
    }

    #[test]
    fn one_style_string_can_be_overridden_alone() {
        let loaded = load(r#"{"styles": {"title": "font-size: 12px;"}}"#).expect("valid");
        assert_eq!(loaded.appearance.styles.title, "font-size: 12px;");
        assert_eq!(loaded.appearance.styles.lead, StyleSet::built_in().lead);
    }

    #[test]
    fn unknown_keys_warn_rather_than_fail() {
        let loaded =
            load(r#"{"styles": {"caption": "x"}, "image": {"avif_quality": 5}, "future": 1}"#)
                .expect("unknown keys must not fail the load");
        let keys: Vec<&str> = loaded
            .warnings
            .iter()
            .filter_map(|w| match w {
                Warning::UnknownAppearanceKey { key } => Some(key.as_str()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&"styles.caption"), "{keys:?}");
        assert!(keys.contains(&"image.avif_quality"), "{keys:?}");
        assert!(keys.contains(&"future"), "{keys:?}");
    }

    #[test]
    fn an_inverted_quality_pair_is_rejected_by_name() {
        // INV-7.
        let error = load(r#"{"image": {"jpeg_quality": 40, "jpeg_min_quality": 80}}"#)
            .expect_err("inverted bounds must be refused");
        assert!(error.to_string().contains("jpeg_min_quality"), "{error}");
    }

    #[test]
    fn out_of_range_qualities_are_rejected() {
        assert!(load(r#"{"image": {"jpeg_quality": 0}}"#).is_err());
        assert!(load(r#"{"image": {"jpeg_quality": 101}}"#).is_err());
        assert!(load(r#"{"image": {"webp_min_quality": 255}}"#).is_err());
    }

    #[test]
    fn the_size_floors_are_enforced() {
        assert!(load(r#"{"image": {"max_width": 0}}"#).is_err());
        assert!(load(r#"{"image": {"max_bytes": 1023}}"#).is_err());
        assert!(load(r#"{"image": {"max_bytes": 1024}}"#).is_ok());
    }

    #[test]
    fn a_style_string_that_could_break_out_is_rejected() {
        assert!(load(r#"{"styles": {"title": "x\"></div><script>alert(1)</script>"}}"#).is_err());
        assert!(load(r#"{"styles": {"title": "</style>"}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_rejected_with_a_position() {
        let error = load("{ not json ").expect_err("malformed JSON must be refused");
        let message = error.to_string();
        assert!(
            message.contains("line"),
            "the message should locate the problem: {message}"
        );
    }

    #[test]
    fn a_non_object_top_level_is_rejected() {
        assert!(load("[]").is_err());
        assert!(load("42").is_err());
    }
}
