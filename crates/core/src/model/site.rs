//! Where articles go, and what they are created with (002 data-model.md).
//!
//! A [`SiteTarget`] is the optional half of a [`ServerConfig`](super::server::ServerConfig) that
//! turns a publish into "photos *and* the article". Like the rest of the server configuration it
//! holds no secret: the bridge on the server reads the site's own database settings (002
//! research R1), so there is nothing here that could leak.
//!
//! [`ArticleSettings`] has the same shape at both of its levels — the server's defaults and the
//! item's overrides — and every field is optional. A field set at neither level is left out of
//! the request altogether, so Joomla applies the value it gives an article made by hand
//! (FR-012).

use time::OffsetDateTime;
use url::Url;

use crate::error::{Error, Result};

/// The Joomla site behind a server (FR-013: absent means insertion is off).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SiteTarget {
    /// The Joomla install directory on the SSH host, as an absolute POSIX path.
    pub joomla_root: String,
    /// The site's public root. Article addresses are built beneath it.
    pub site_url: Url,
    /// The PHP command-line interpreter on the SSH host.
    #[serde(default = "default_php")]
    pub php: String,
    /// The server-level article settings. `category` must be set (INV-S1).
    #[serde(default)]
    pub defaults: ArticleSettings,
}

fn default_php() -> String {
    "php".to_owned()
}

impl SiteTarget {
    /// Checks the target before anything is uploaded, naming the field at fault.
    ///
    /// INV-S1: a target without a default category is incomplete, because Joomla's own default
    /// ("Uncategorised") is never where a news item belongs.
    pub fn validate(&self, server: &str) -> Result<()> {
        self.validate_connection(server)?;
        if self.defaults.category.is_none() {
            return Err(Error::SiteIncomplete {
                server: server.to_owned(),
                field: "default category".to_owned(),
            });
        }
        Ok(())
    }

    /// Everything [`validate`](Self::validate) checks except the category — what a target needs
    /// to be saved and checked, since the category is chosen from what the check reads.
    pub fn validate_connection(&self, server: &str) -> Result<()> {
        let incomplete = |field: &str| Error::SiteIncomplete {
            server: server.to_owned(),
            field: field.to_owned(),
        };
        if !self.joomla_root.starts_with('/') {
            return Err(incomplete("joomla_root (an absolute path)"));
        }
        if self.php.trim().is_empty() || self.php.contains(['\n', '\r']) {
            return Err(incomplete("php"));
        }
        if !matches!(self.site_url.scheme(), "http" | "https") {
            return Err(incomplete("site_url (http or https)"));
        }
        Ok(())
    }
}

/// Published or not. There is deliberately no trashed or archived state: the application
/// cannot put an article there (FR-008, INV-S3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArticleState {
    /// Live on the site.
    Published,
    /// Saved, not shown.
    Unpublished,
}

/// The options an editor sets on a Joomla article by hand (FR-010).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ArticleSettings {
    /// `catid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<u32>,
    /// `state`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<ArticleState>,
    /// `featured`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub featured: Option<bool>,
    /// `access`: a view level id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<u32>,
    /// `language`: `*` or a content language code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// `created_by`: a user id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<u32>,
    /// `created_by_alias`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_alias: Option<String>,
    /// `publish_up`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub publish_up: Option<OffsetDateTime>,
    /// `publish_down`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub publish_down: Option<OffsetDateTime>,
    /// `metadesc`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_description: Option<String>,
    /// Tag ids. `Some(vec![])` means "no tags"; an item's list replaces the server's rather than
    /// adding to it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<u32>>,
}

impl ArticleSettings {
    /// The settings an article is written with: the item's override where there is one, the
    /// server's default where there is not, and nothing — so Joomla decides — where neither is
    /// set. The one exception is the state, which is published unless someone said otherwise
    /// (spec clarification Q3).
    #[must_use]
    pub fn resolve(overrides: &Self, defaults: &Self) -> Self {
        Self {
            category: overrides.category.or(defaults.category),
            state: overrides
                .state
                .or(defaults.state)
                .or(Some(ArticleState::Published)),
            featured: overrides.featured.or(defaults.featured),
            access: overrides.access.or(defaults.access),
            language: overrides
                .language
                .clone()
                .or_else(|| defaults.language.clone()),
            author: overrides.author.or(defaults.author),
            author_alias: overrides
                .author_alias
                .clone()
                .or_else(|| defaults.author_alias.clone()),
            publish_up: overrides.publish_up.or(defaults.publish_up),
            publish_down: overrides.publish_down.or(defaults.publish_down),
            meta_description: overrides
                .meta_description
                .clone()
                .or_else(|| defaults.meta_description.clone()),
            // Sorted and once each, so "the same tags" compares equal however they were chosen.
            tags: overrides
                .tags
                .clone()
                .or_else(|| defaults.tags.clone())
                .map(|mut tags| {
                    tags.sort_unstable();
                    tags.dedup();
                    tags
                }),
        }
    }

    /// Rejects values Joomla would refuse or silently mangle, naming the field.
    pub fn validate(&self) -> Result<()> {
        let invalid = |field: &str, detail: &str| Error::InvalidArticleSetting {
            field: field.to_owned(),
            detail: detail.to_owned(),
        };
        if let (Some(up), Some(down)) = (self.publish_up, self.publish_down) {
            if down <= up {
                return Err(invalid(
                    "publish_down",
                    "the publication must finish after it starts",
                ));
            }
        }
        if let Some(text) = &self.meta_description {
            if text.chars().count() > 300 {
                return Err(invalid("meta_description", "longer than 300 characters"));
            }
            if text.contains(['\n', '\r']) {
                return Err(invalid("meta_description", "contains a line break"));
            }
        }
        if let Some(alias) = &self.author_alias {
            if alias.chars().count() > 255 {
                return Err(invalid("author_alias", "longer than 255 characters"));
            }
        }
        if self.tags.as_ref().is_some_and(|tags| tags.contains(&0)) {
            return Err(invalid("tags", "0 is not a tag id"));
        }
        if let Some(language) = &self.language {
            if language.is_empty() || language.contains(char::is_whitespace) {
                return Err(invalid("language", "not `*` or a language code"));
            }
        }
        Ok(())
    }
}

/// The article's cover: Joomla's "Intro Image", shown beside the announcement in news lists.
///
/// Per item only — a server has no photo to default to — and chosen from the item's own placed
/// photos, which are already on the server by the time the article is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "photo", rename_all = "snake_case")]
pub enum IntroImage {
    /// The first photo in the text. What an editor who chooses nothing gets.
    #[default]
    First,
    /// This photo.
    Photo(crate::model::photo::PhotoId),
    /// No cover; an existing one is cleared.
    None,
}

/// The site's own choices, read from it (FR-011).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SiteCatalog {
    /// `JVERSION`, e.g. `5.2.3`.
    pub joomla_version: String,
    /// `com_content` categories, in tree order.
    pub categories: Vec<Category>,
    /// View levels.
    pub access_levels: Vec<AccessLevel>,
    /// Content languages, including `*`.
    pub languages: Vec<Language>,
    /// People who have written articles.
    pub authors: Vec<Author>,
    /// Published tags, in tree order.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// One tag (метка).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    /// Id.
    pub id: u32,
    /// Title.
    pub title: String,
    /// Depth in the tree, 1 at the top.
    pub level: u32,
}

/// One `com_content` category.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Category {
    /// Id.
    pub id: u32,
    /// Title.
    pub title: String,
    /// Depth in the tree, 1 at the top.
    pub level: u32,
    /// Whether the category is published.
    pub published: bool,
    /// `*` or a language code.
    pub language: String,
}

/// One view level.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AccessLevel {
    /// Id.
    pub id: u32,
    /// Title.
    pub title: String,
}

/// One content language.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Language {
    /// `*` or a code such as `ru-RU`.
    pub code: String,
    /// Title.
    pub title: String,
}

/// One author.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Author {
    /// User id.
    pub id: u32,
    /// Display name.
    pub name: String,
}

/// What `find` is asked about: the article this item would be, and what it would contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleProbe {
    /// The alias the item's article has, which is its photo folder's name (research R5).
    pub alias: String,
    /// The category it would be written into.
    pub category: u32,
    /// The title it would carry.
    pub title: String,
    /// The text it would carry.
    pub articletext: String,
    /// The settings it would carry, so `identical` can take them into account.
    pub settings: ArticleSettings,
    /// The cover's path or address; `Some("")` clears it, `None` leaves it alone.
    pub intro_image: Option<String>,
}

/// What the site holds under an alias.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FindResult {
    /// Every article carrying this alias and the application's mark, in any category.
    pub articles: Vec<SiteArticle>,
    /// An article *without* the mark holding the alias in the target category.
    pub alias_taken_by: Option<u32>,
}

/// One article the application wrote earlier.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SiteArticle {
    /// Article id.
    pub id: u32,
    /// Current category.
    pub category: u32,
    /// Whether it is in the trash.
    pub trashed: bool,
    /// False when someone changed the title or text on the site since the application wrote it.
    pub content_matches_mark: bool,
    /// True when title, text and every requested setting already equal the probe.
    pub identical: bool,
    /// The article's address.
    pub url: String,
}

/// What the bridge is asked to save. Built only by `core`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleWrite {
    /// `Some` updates that article; `None` creates one.
    pub id: Option<u32>,
    /// Takes a trashed article out of the trash (only ever on confirmation).
    pub restore: bool,
    /// The alias: the folder slug the photos went into.
    pub alias: String,
    /// The item's title.
    pub title: String,
    /// The fragment, byte for byte (INV-S4).
    pub articletext: String,
    /// Resolved settings. Unset fields are left for Joomla to default.
    pub settings: ArticleSettings,
    /// The cover's path or address; `Some("")` clears it, `None` leaves it alone.
    pub intro_image: Option<String>,
}

/// What the bridge reports after a save.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedArticle {
    /// Article id.
    pub id: u32,
    /// True when the article was new.
    pub created: bool,
    /// The article's address.
    pub url: String,
}

/// Why a publish stopped to ask before touching the article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationReason {
    /// The item was published before and its article is no longer on the site.
    Gone,
    /// The article is in the trash.
    Trashed,
    /// Someone changed the article on the site since the application wrote it.
    EditedOnSite,
}

/// The editor's answer to a [`ConfirmationReason`]. The default asks nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArticleConfirmation {
    /// No confirmation given.
    #[default]
    None,
    /// Accepts [`ConfirmationReason::EditedOnSite`] and [`ConfirmationReason::Trashed`].
    Overwrite,
    /// Accepts [`ConfirmationReason::Gone`].
    CreateNew,
}

impl ArticleConfirmation {
    /// Whether this answer authorises going ahead in this situation. Confirming one thing never
    /// authorises another.
    #[must_use]
    pub fn accepts(self, reason: ConfirmationReason) -> bool {
        matches!(
            (self, reason),
            (
                Self::Overwrite,
                ConfirmationReason::EditedOnSite | ConfirmationReason::Trashed
            ) | (Self::CreateNew, ConfirmationReason::Gone)
        )
    }
}

/// What happened to the article during a publish.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ArticleOutcome {
    /// A new article was created.
    Created {
        /// Article id.
        id: u32,
        /// Address.
        url: String,
    },
    /// The item's article was changed.
    Updated {
        /// Article id.
        id: u32,
        /// Address.
        url: String,
    },
    /// The article already said exactly this; nothing was written (FR-006).
    Unchanged {
        /// Article id.
        id: u32,
        /// Address.
        url: String,
    },
    /// Dry run: an article would be created with these settings (FR-009).
    WouldCreate {
        /// The resolved settings.
        settings: ArticleSettings,
    },
    /// Dry run: this article would be updated with these settings.
    WouldUpdate {
        /// Article id.
        id: u32,
        /// The resolved settings.
        settings: ArticleSettings,
    },
    /// Nothing was written; the editor has to decide first.
    NeedsConfirmation {
        /// What was found.
        reason: ConfirmationReason,
        /// The article concerned, when there is one.
        id: Option<u32>,
    },
    /// The photos are on the server and the article is not (FR-015).
    Failed {
        /// The step that failed.
        step: String,
        /// What went wrong.
        detail: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirmation_only_authorises_what_it_names() {
        use ConfirmationReason::*;
        assert!(ArticleConfirmation::Overwrite.accepts(EditedOnSite));
        assert!(ArticleConfirmation::Overwrite.accepts(Trashed));
        assert!(!ArticleConfirmation::Overwrite.accepts(Gone));
        assert!(ArticleConfirmation::CreateNew.accepts(Gone));
        assert!(!ArticleConfirmation::CreateNew.accepts(EditedOnSite));
        assert!(!ArticleConfirmation::None.accepts(Gone));
    }
}
