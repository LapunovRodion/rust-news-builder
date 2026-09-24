//! Site insertion as the frontend sees it (002 contracts/desktop-commands.md).
//!
//! Views only, and the two commands that edit the open item's article overrides. Resolution,
//! validation and the re-publish decisions all happen in `core`; nothing here decides anything.

use newsbuilder_core::model::site::{
    ArticleOutcome, ArticleSettings, ArticleState, ConfirmationReason, IntroImage, SiteCatalog,
    SiteTarget,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::commands::item::lock;
use crate::error::{CommandError, CommandResult};
use crate::state::Session;

/// [`ArticleSettings`], with dates as RFC 3339 text.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleSettingsView {
    pub category: Option<u32>,
    /// `"published"` or `"unpublished"`.
    pub state: Option<String>,
    pub featured: Option<bool>,
    pub access: Option<u32>,
    pub language: Option<String>,
    pub author: Option<u32>,
    pub author_alias: Option<String>,
    pub publish_up: Option<String>,
    pub publish_down: Option<String>,
    pub meta_description: Option<String>,
    /// `None`: not set here. `Some([])`: no tags.
    #[serde(default)]
    pub tags: Option<Vec<u32>>,
}

/// A form field left empty means "not set", not "set to nothing".
fn filled(value: Option<&String>) -> Option<String> {
    value
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

fn instant(field: &str, value: Option<&String>) -> CommandResult<Option<OffsetDateTime>> {
    filled(value)
        .map(|text| {
            OffsetDateTime::parse(&text, &Rfc3339).map_err(|error| {
                CommandError::new(
                    "invalid_article_setting",
                    format!("`{field}` is not a date and time: {error}"),
                )
            })
        })
        .transpose()
}

impl ArticleSettingsView {
    pub fn into_settings(self) -> CommandResult<ArticleSettings> {
        let state = match filled(self.state.as_ref()).as_deref() {
            None => None,
            Some("published") => Some(ArticleState::Published),
            Some("unpublished") => Some(ArticleState::Unpublished),
            Some(other) => {
                return Err(CommandError::new(
                    "invalid_article_setting",
                    format!("`state` must be published or unpublished, not `{other}`"),
                ));
            }
        };
        Ok(ArticleSettings {
            category: self.category,
            state,
            featured: self.featured,
            access: self.access,
            language: filled(self.language.as_ref()),
            author: self.author,
            author_alias: filled(self.author_alias.as_ref()),
            publish_up: instant("publishUp", self.publish_up.as_ref())?,
            publish_down: instant("publishDown", self.publish_down.as_ref())?,
            meta_description: filled(self.meta_description.as_ref()),
            tags: self.tags,
        })
    }
}

fn rfc3339(value: Option<OffsetDateTime>) -> Option<String> {
    value.and_then(|v| v.format(&Rfc3339).ok())
}

impl From<&ArticleSettings> for ArticleSettingsView {
    fn from(settings: &ArticleSettings) -> Self {
        Self {
            category: settings.category,
            state: settings.state.map(|s| match s {
                ArticleState::Published => "published".to_owned(),
                ArticleState::Unpublished => "unpublished".to_owned(),
            }),
            featured: settings.featured,
            access: settings.access,
            language: settings.language.clone(),
            author: settings.author,
            author_alias: settings.author_alias.clone(),
            publish_up: rfc3339(settings.publish_up),
            publish_down: rfc3339(settings.publish_down),
            meta_description: settings.meta_description.clone(),
            tags: settings.tags.clone(),
        }
    }
}

/// [`SiteTarget`] for the server form.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteTargetView {
    pub joomla_root: String,
    pub site_url: String,
    pub php: String,
    pub defaults: ArticleSettingsView,
}

impl From<&SiteTarget> for SiteTargetView {
    fn from(site: &SiteTarget) -> Self {
        Self {
            joomla_root: site.joomla_root.clone(),
            site_url: site.site_url.to_string(),
            php: site.php.clone(),
            defaults: ArticleSettingsView::from(&site.defaults),
        }
    }
}

impl SiteTargetView {
    pub fn into_target(self, server: &str) -> CommandResult<SiteTarget> {
        let site_url = url::Url::parse(self.site_url.trim()).map_err(|error| {
            CommandError::new(
                "invalid_url",
                format!("`{}` is not a URL: {error}", self.site_url),
            )
        })?;
        let php = filled(Some(&self.php)).unwrap_or_else(|| "php".to_owned());
        let target = SiteTarget {
            joomla_root: self.joomla_root.trim().to_owned(),
            site_url,
            php,
            defaults: self.defaults.into_settings()?,
        };
        // Refused at save time rather than at the first publish. The category is not required
        // yet: it is chosen from what "check connection" reads, which needs a saved server.
        // Publishing still refuses without one (INV-S1).
        target.validate_connection(server)?;
        target.defaults.validate()?;
        Ok(target)
    }
}

/// [`SiteCatalog`] for the selects.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteCatalogView {
    pub joomla_version: String,
    pub categories: Vec<CategoryView>,
    pub access_levels: Vec<IdTitle>,
    pub languages: Vec<LanguageView>,
    pub authors: Vec<IdTitle>,
    pub tags: Vec<TagView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagView {
    pub id: u32,
    pub title: String,
    pub level: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryView {
    pub id: u32,
    pub title: String,
    pub level: u32,
    pub published: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdTitle {
    pub id: u32,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageView {
    pub code: String,
    pub title: String,
}

impl From<SiteCatalog> for SiteCatalogView {
    fn from(catalog: SiteCatalog) -> Self {
        Self {
            joomla_version: catalog.joomla_version,
            categories: catalog
                .categories
                .into_iter()
                .map(|c| CategoryView {
                    id: c.id,
                    title: c.title,
                    level: c.level,
                    published: c.published,
                })
                .collect(),
            access_levels: catalog
                .access_levels
                .into_iter()
                .map(|l| IdTitle {
                    id: l.id,
                    title: l.title,
                })
                .collect(),
            languages: catalog
                .languages
                .into_iter()
                .map(|l| LanguageView {
                    code: l.code,
                    title: l.title,
                })
                .collect(),
            authors: catalog
                .authors
                .into_iter()
                .map(|a| IdTitle {
                    id: a.id,
                    title: a.name,
                })
                .collect(),
            tags: catalog
                .tags
                .into_iter()
                .map(|t| TagView {
                    id: t.id,
                    title: t.title,
                    level: t.level,
                })
                .collect(),
        }
    }
}

/// [`ArticleOutcome`] for the publish dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ArticleOutcomeView {
    Created {
        id: u32,
        url: String,
    },
    Updated {
        id: u32,
        url: String,
    },
    Unchanged {
        id: u32,
        url: String,
    },
    WouldCreate {
        id: Option<u32>,
        settings: ArticleSettingsView,
    },
    WouldUpdate {
        id: Option<u32>,
        settings: ArticleSettingsView,
    },
    NeedsConfirmation {
        reason: ConfirmationReason,
        id: Option<u32>,
    },
    Failed {
        step: String,
        detail: String,
    },
}

impl From<ArticleOutcome> for ArticleOutcomeView {
    fn from(outcome: ArticleOutcome) -> Self {
        match outcome {
            ArticleOutcome::Created { id, url } => Self::Created { id, url },
            ArticleOutcome::Updated { id, url } => Self::Updated { id, url },
            ArticleOutcome::Unchanged { id, url } => Self::Unchanged { id, url },
            ArticleOutcome::WouldCreate { settings } => Self::WouldCreate {
                id: None,
                settings: ArticleSettingsView::from(&settings),
            },
            ArticleOutcome::WouldUpdate { id, settings } => Self::WouldUpdate {
                id: Some(id),
                settings: ArticleSettingsView::from(&settings),
            },
            ArticleOutcome::NeedsConfirmation { reason, id } => {
                Self::NeedsConfirmation { reason, id }
            }
            ArticleOutcome::Failed { step, detail } => Self::Failed { step, detail },
        }
    }
}

/// The open item's article overrides.
#[tauri::command]
pub fn get_article_settings(session: State<'_, Session>) -> CommandResult<ArticleSettingsView> {
    let state = lock(&session)?;
    Ok(ArticleSettingsView::from(&state.item.article))
}

/// Replaces the open item's article overrides, validated by core.
#[tauri::command]
pub fn set_article_settings(
    settings: ArticleSettingsView,
    session: State<'_, Session>,
) -> CommandResult<ArticleSettingsView> {
    let settings = settings.into_settings()?;
    settings.validate()?;
    let mut state = lock(&session)?;
    state.item.article = settings;
    Ok(ArticleSettingsView::from(&state.item.article))
}

/// The cover as the frontend names it: `"first"`, `"none"`, or a photo id.
fn cover_view(cover: IntroImage) -> String {
    match cover {
        IntroImage::First => "first".to_owned(),
        IntroImage::None => "none".to_owned(),
        IntroImage::Photo(id) => id.0.to_string(),
    }
}

/// The open item's cover.
#[tauri::command]
pub fn get_intro_image(session: State<'_, Session>) -> CommandResult<String> {
    Ok(cover_view(lock(&session)?.item.intro_image))
}

/// Chooses the open item's cover. Whether the photo is placed is checked at publish, where it
/// matters; here it only has to be one of the item's photos.
#[tauri::command]
pub fn set_intro_image(choice: String, session: State<'_, Session>) -> CommandResult<String> {
    let mut state = lock(&session)?;
    let cover = match choice.as_str() {
        "first" => IntroImage::First,
        "none" => IntroImage::None,
        id => IntroImage::Photo(state.photo_id(id)?),
    };
    state.item.intro_image = cover;
    Ok(cover_view(cover))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_form_fields_are_unset_rather_than_empty() {
        let view = ArticleSettingsView {
            language: Some("  ".to_owned()),
            author_alias: Some(String::new()),
            publish_up: Some(String::new()),
            ..ArticleSettingsView::default()
        };
        assert_eq!(view.into_settings().unwrap(), ArticleSettings::default());
    }

    #[test]
    fn a_date_round_trips_through_the_view() {
        let view = ArticleSettingsView {
            publish_up: Some("2026-09-17T10:00:00+03:00".to_owned()),
            state: Some("unpublished".to_owned()),
            ..ArticleSettingsView::default()
        };
        let settings = view.into_settings().unwrap();
        assert_eq!(settings.state, Some(ArticleState::Unpublished));
        let back = ArticleSettingsView::from(&settings);
        assert_eq!(
            back.publish_up.as_deref(),
            Some("2026-09-17T10:00:00+03:00")
        );
    }

    #[test]
    fn a_site_can_be_saved_before_its_category_is_chosen() {
        let view = SiteTargetView {
            joomla_root: "/var/www/html".to_owned(),
            site_url: "https://example.org/".to_owned(),
            php: String::new(),
            defaults: ArticleSettingsView::default(),
        };
        let target = view.into_target("newsroom").unwrap();
        assert_eq!(target.php, "php");
    }

    #[test]
    fn a_relative_joomla_directory_is_refused_when_saved() {
        let view = SiteTargetView {
            joomla_root: "public_html".to_owned(),
            site_url: "https://example.org/".to_owned(),
            php: "php".to_owned(),
            defaults: ArticleSettingsView::default(),
        };
        assert!(view.into_target("newsroom").is_err());
    }
}
