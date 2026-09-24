//! The article half of a publish (002 FR-001 – FR-009, research R5 – R7).
//!
//! Photos first, through the unchanged [`publish`]; then the article, through the [`Site`] port.
//! Everything that *decides* lives here — which article is the item's, whether to create, update,
//! leave alone, or stop and ask — so it is tested against a fake site, and the bridge on the
//! server only answers facts and does what it is told.
//!
//! Two rules shape the error handling:
//!
//! - A failure **before** the photos are up is an `Err`, and no article is touched (FR-004).
//! - A failure **after** is an [`ArticleOutcome::Failed`] on an `Ok` publication, because the
//!   photos are on the server and the editor still needs the fragment (FR-015).

use crate::build::PhotoBytesSource;
use crate::error::{Error, Result};
use crate::model::item::NewsItem;
use crate::model::server::ServerConfig;
use crate::model::site::{
    ArticleConfirmation, ArticleOutcome, ArticleProbe, ArticleSettings, ArticleWrite,
    ConfirmationReason, FindResult, IntroImage, SiteCatalog, SiteTarget,
};
use crate::ports::{SecretStore, Site, Transport};

use super::{Publication, PublishMode, credential, publish};
use crate::adapters::joomla::step;

/// Publishes the photos and then, when the server has a site target, the article.
///
/// With no site target this *is* [`publish`] (INV-S5).
pub fn publish_to_site<R: Transport + Site>(
    item: &NewsItem,
    server: &ServerConfig,
    remote: &mut R,
    secrets: &dyn SecretStore,
    mode: PublishMode,
    confirmation: ArticleConfirmation,
    sources: &dyn PhotoBytesSource,
) -> Result<Publication> {
    let Some(target) = &server.site else {
        return publish(item, server, remote, secrets, mode, sources);
    };

    // Everything that can be refused without the network is refused before a photo is touched.
    target.validate(&server.name)?;
    let settings = ArticleSettings::resolve(&item.article, &target.defaults);
    settings.validate()?;
    let category = settings.category.ok_or_else(|| Error::SiteIncomplete {
        server: server.name.clone(),
        field: "default category".to_owned(),
    })?;
    if let IntroImage::Photo(id) = item.intro_image
        && !item.placed_photo_ids().contains(&id)
    {
        return Err(Error::InvalidArticleSetting {
            field: "intro_image".to_owned(),
            detail: format!("photo {id} is not placed in the text, so it is not published"),
        });
    }

    let mut publication = publish(item, server, remote, secrets, mode, sources)?;

    let probe = ArticleProbe {
        alias: publication.folder.as_str().to_owned(),
        category,
        title: item.title.clone(),
        articletext: publication.fragment.clone(),
        settings,
        intro_image: cover(item, target, &publication),
    };
    // A folder already holding the item's files is the evidence of an earlier publish, the same
    // evidence the folder itself is chosen by (research R5). Not "some photos were unchanged":
    // after a re-crop every photo is re-uploaded, and the article is still the item's.
    let published_before = publication.folder_existed;

    publication.article = Some(article(
        target,
        remote,
        probe,
        published_before,
        mode,
        confirmation,
    ));
    Ok(publication)
}

/// Connects and reads the site's choices, writing nothing (FR-011, FR-017).
///
/// Unlike a publish, this does not need a default category — choosing one is what the catalog
/// is for.
pub fn check_site<R: Transport + Site>(
    server: &ServerConfig,
    remote: &mut R,
    secrets: &dyn SecretStore,
) -> Result<SiteCatalog> {
    let target = server.site.as_ref().ok_or_else(|| Error::SiteIncomplete {
        server: server.name.clone(),
        field: "site settings".to_owned(),
    })?;
    let credential = credential(server, secrets)?;
    remote.connect(server, &credential)?;
    remote.describe(target)
}

/// The cover as the article stores it (Joomla's "Intro Image").
///
/// A photo under the site's own address is given relative to the site root — `images/…` —
/// which is how Joomla's media field stores a local file; anything else stays a full address.
fn cover(item: &NewsItem, target: &SiteTarget, publication: &Publication) -> Option<String> {
    let id = match item.intro_image {
        IntroImage::None => return Some(String::new()),
        IntroImage::Photo(id) => id,
        IntroImage::First => *item.placed_photo_ids().first()?,
    };
    let url = publication
        .photo_urls
        .iter()
        .find(|(photo, _)| *photo == id)
        .map(|(_, url)| url.as_str())?;
    Some(
        url.strip_prefix(target.site_url.as_str())
            .unwrap_or(url)
            .to_owned(),
    )
}

/// What the matrix decided.
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Create,
    Update {
        id: u32,
        restore: bool,
    },
    Unchanged {
        id: u32,
        url: String,
    },
    Ask {
        reason: ConfirmationReason,
        id: Option<u32>,
    },
}

/// The re-publish matrix (research R6).
fn decide(
    probe: &ArticleProbe,
    found: FindResult,
    published_before: bool,
    confirmation: ArticleConfirmation,
) -> Result<Decision> {
    if let Some(article) = found.alias_taken_by {
        return Err(Error::AliasTaken {
            alias: probe.alias.clone(),
            article,
        });
    }

    let mut articles = found.articles;
    if articles.len() > 1 {
        return Err(Error::ArticleAmbiguous {
            alias: probe.alias.clone(),
            ids: articles.iter().map(|a| a.id).collect(),
        });
    }

    let ask_or = |reason, id, then: Decision| {
        if confirmation.accepts(reason) {
            then
        } else {
            Decision::Ask { reason, id }
        }
    };

    Ok(match articles.pop() {
        None if published_before => ask_or(ConfirmationReason::Gone, None, Decision::Create),
        None => Decision::Create,
        Some(a) if a.trashed => ask_or(
            ConfirmationReason::Trashed,
            Some(a.id),
            Decision::Update {
                id: a.id,
                restore: true,
            },
        ),
        Some(a) if !a.content_matches_mark => ask_or(
            ConfirmationReason::EditedOnSite,
            Some(a.id),
            Decision::Update {
                id: a.id,
                restore: false,
            },
        ),
        Some(a) if a.identical => Decision::Unchanged {
            id: a.id,
            url: a.url,
        },
        Some(a) => Decision::Update {
            id: a.id,
            restore: false,
        },
    })
}

/// Finds, decides, and — on a live run — writes. Never fails: a failure is an outcome.
fn article(
    target: &SiteTarget,
    site: &mut dyn Site,
    probe: ArticleProbe,
    published_before: bool,
    mode: PublishMode,
    confirmation: ArticleConfirmation,
) -> ArticleOutcome {
    let found = match site.find(target, &probe) {
        Ok(found) => found,
        Err(error) => return failed(step::FIND, error),
    };
    let decision = match decide(&probe, found, published_before, confirmation) {
        Ok(decision) => decision,
        Err(error) => return failed(step::FIND, error),
    };

    let (id, restore) = match decision {
        Decision::Unchanged { id, url } => return ArticleOutcome::Unchanged { id, url },
        Decision::Ask { reason, id } => return ArticleOutcome::NeedsConfirmation { reason, id },
        Decision::Create => (None, false),
        Decision::Update { id, restore } => (Some(id), restore),
    };

    if mode == PublishMode::DryRun {
        return match id {
            None => ArticleOutcome::WouldCreate {
                settings: probe.settings,
            },
            Some(id) => ArticleOutcome::WouldUpdate {
                id,
                settings: probe.settings,
            },
        };
    }

    let write = ArticleWrite {
        id,
        restore,
        alias: probe.alias,
        title: probe.title,
        articletext: probe.articletext,
        settings: probe.settings,
        intro_image: probe.intro_image,
    };
    match site.save(target, &write) {
        Ok(saved) if saved.created => ArticleOutcome::Created {
            id: saved.id,
            url: saved.url,
        },
        Ok(saved) => ArticleOutcome::Updated {
            id: saved.id,
            url: saved.url,
        },
        Err(error) => failed(step::SAVE, error),
    }
}

/// An error after the upload, as the outcome the editor sees.
fn failed(during: &str, error: Error) -> ArticleOutcome {
    match error {
        Error::SiteBridge { step, detail } => ArticleOutcome::Failed { step, detail },
        other => ArticleOutcome::Failed {
            step: during.to_owned(),
            detail: other.to_string(),
        },
    }
}
