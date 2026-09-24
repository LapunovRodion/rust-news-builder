//! 002 User Story 2: re-publishing converges on one article (T024, research R6).
//!
//! One test per row of the decision matrix, against [`ScriptedSite`]: what the site reports is
//! scripted, and what `core` decides is read off the outcome and the site's call log.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::model::item::NewsItem;
use newsbuilder_core::model::server::ServerConfig;
use newsbuilder_core::model::site::{
    ArticleConfirmation, ArticleOutcome, ConfirmationReason, FindResult, SiteArticle,
};
use newsbuilder_core::publish::{Publication, PublishMode, paths, publish_to_site};
use pretty_assertions::assert_eq;
use support::fakes::{
    FakeSecretStore, RecordingTransport, RemoteFake, ScriptedSite, site_server_config,
};

const SERVER: &str = "editorial";

fn item() -> NewsItem {
    support::item("markers").expect("the markers fixture imports")
}

fn ours(id: u32) -> SiteArticle {
    SiteArticle {
        id,
        category: 8,
        trashed: false,
        content_matches_mark: true,
        identical: false,
        url: ScriptedSite::url_of(id),
    }
}

fn found(articles: Vec<SiteArticle>) -> FindResult {
    FindResult {
        articles,
        alias_taken_by: None,
    }
}

/// A transport whose item folder already holds one of the item's photos: the evidence that the
/// item was published before.
fn published_before(item: &NewsItem, server: &ServerConfig) -> RecordingTransport {
    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    RecordingTransport::new().with_existing(folder, "marker-coverage-01.jpg", vec![0_u8; 3])
}

fn publish(
    remote: &mut RemoteFake,
    mode: PublishMode,
    confirmation: ArticleConfirmation,
) -> (Publication, ArticleOutcome) {
    let item = item();
    let server = site_server_config(SERVER);
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "secret");
    let publication = publish_to_site(
        &item,
        &server,
        remote,
        &secrets,
        mode,
        confirmation,
        &EmbeddedBytes,
    )
    .expect("publishes");
    let outcome = publication.article.clone().expect("the server has a site");
    (publication, outcome)
}

fn site(found: FindResult) -> ScriptedSite {
    let mut site = ScriptedSite::new();
    site.found = found;
    site
}

#[test]
fn an_identical_article_is_left_alone() {
    let mut article = ours(1234);
    article.identical = true;
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![article])));

    let (_, outcome) = publish(&mut remote, PublishMode::Live, ArticleConfirmation::None);

    assert_eq!(
        outcome,
        ArticleOutcome::Unchanged {
            id: 1234,
            url: ScriptedSite::url_of(1234)
        }
    );
    assert!(remote.site.saves().is_empty(), "{:?}", remote.site.log());
}

#[test]
fn a_changed_item_updates_its_own_article() {
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![ours(1234)])));

    let (publication, outcome) = publish(&mut remote, PublishMode::Live, ArticleConfirmation::None);

    let saves = remote.site.saves();
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].id, Some(1234));
    assert!(!saves[0].restore);
    assert_eq!(saves[0].articletext, publication.fragment);
    assert_eq!(
        outcome,
        ArticleOutcome::Updated {
            id: 1234,
            url: ScriptedSite::url_of(1234)
        }
    );
}

#[test]
fn an_article_edited_on_the_site_is_not_overwritten_without_asking() {
    let mut article = ours(1234);
    article.content_matches_mark = false;
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![article])));

    let (_, outcome) = publish(&mut remote, PublishMode::Live, ArticleConfirmation::None);

    assert_eq!(
        outcome,
        ArticleOutcome::NeedsConfirmation {
            reason: ConfirmationReason::EditedOnSite,
            id: Some(1234)
        }
    );
    assert!(remote.site.saves().is_empty());
}

#[test]
fn an_article_edited_on_the_site_is_overwritten_once_confirmed() {
    let mut article = ours(1234);
    article.content_matches_mark = false;
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![article])));

    let (_, outcome) = publish(
        &mut remote,
        PublishMode::Live,
        ArticleConfirmation::Overwrite,
    );

    assert!(
        matches!(outcome, ArticleOutcome::Updated { id: 1234, .. }),
        "{outcome:?}"
    );
    assert_eq!(remote.site.saves()[0].id, Some(1234));
}

#[test]
fn a_trashed_article_is_asked_about_and_restored_on_confirmation() {
    let mut article = ours(1234);
    article.trashed = true;

    let mut asking = RemoteFake::new(
        RecordingTransport::new(),
        site(found(vec![article.clone()])),
    );
    let (_, outcome) = publish(&mut asking, PublishMode::Live, ArticleConfirmation::None);
    assert_eq!(
        outcome,
        ArticleOutcome::NeedsConfirmation {
            reason: ConfirmationReason::Trashed,
            id: Some(1234)
        }
    );
    assert!(asking.site.saves().is_empty());

    let mut confirmed = RemoteFake::new(RecordingTransport::new(), site(found(vec![article])));
    let (_, outcome) = publish(
        &mut confirmed,
        PublishMode::Live,
        ArticleConfirmation::Overwrite,
    );
    assert!(
        matches!(outcome, ArticleOutcome::Updated { id: 1234, .. }),
        "{outcome:?}"
    );
    let write = confirmed.site.saves()[0].clone();
    assert_eq!(write.id, Some(1234));
    assert!(write.restore);
}

#[test]
fn an_article_deleted_from_the_site_is_asked_about_and_recreated_on_confirmation() {
    let item = item();
    let server = site_server_config(SERVER);

    let mut asking = RemoteFake::new(published_before(&item, &server), ScriptedSite::new());
    let (_, outcome) = publish(&mut asking, PublishMode::Live, ArticleConfirmation::None);
    assert_eq!(
        outcome,
        ArticleOutcome::NeedsConfirmation {
            reason: ConfirmationReason::Gone,
            id: None
        }
    );
    assert!(asking.site.saves().is_empty());

    let mut confirmed = RemoteFake::new(published_before(&item, &server), ScriptedSite::new());
    let (_, outcome) = publish(
        &mut confirmed,
        PublishMode::Live,
        ArticleConfirmation::CreateNew,
    );
    assert!(
        matches!(outcome, ArticleOutcome::Created { .. }),
        "{outcome:?}"
    );
    assert_eq!(confirmed.site.saves()[0].id, None);
}

#[test]
fn a_confirmation_for_something_else_authorises_nothing() {
    let mut article = ours(1234);
    article.content_matches_mark = false;
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![article])));

    let (_, outcome) = publish(
        &mut remote,
        PublishMode::Live,
        ArticleConfirmation::CreateNew,
    );

    assert!(
        matches!(
            outcome,
            ArticleOutcome::NeedsConfirmation {
                reason: ConfirmationReason::EditedOnSite,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert!(remote.site.saves().is_empty());
}

#[test]
fn two_articles_for_one_item_are_refused_by_id() {
    let mut remote = RemoteFake::new(
        RecordingTransport::new(),
        site(found(vec![ours(1204), ours(1311)])),
    );

    let (_, outcome) = publish(&mut remote, PublishMode::Live, ArticleConfirmation::None);

    match outcome {
        ArticleOutcome::Failed { detail, .. } => {
            assert!(
                detail.contains("1204") && detail.contains("1311"),
                "{detail}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(remote.site.saves().is_empty());
}

#[test]
fn an_alias_held_by_someone_elses_article_is_refused_by_id() {
    let mut remote = RemoteFake::new(
        RecordingTransport::new(),
        site(FindResult {
            articles: vec![],
            alias_taken_by: Some(777),
        }),
    );

    let (_, outcome) = publish(&mut remote, PublishMode::Live, ArticleConfirmation::None);

    match outcome {
        ArticleOutcome::Failed { detail, .. } => assert!(detail.contains("777"), "{detail}"),
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(remote.site.saves().is_empty());
}

#[test]
fn a_dry_run_over_an_existing_article_reports_the_update_and_writes_nothing() {
    let mut remote = RemoteFake::new(RecordingTransport::new(), site(found(vec![ours(1234)])));

    let (_, outcome) = publish(&mut remote, PublishMode::DryRun, ArticleConfirmation::None);

    assert!(
        matches!(outcome, ArticleOutcome::WouldUpdate { id: 1234, .. }),
        "{outcome:?}"
    );
    assert!(remote.site.saves().is_empty());
}
