//! 002 User Story 1: a publish creates the article (T011).
//!
//! Every property here is a property of the orchestration in `core::publish::article`, so it is
//! asserted against [`RemoteFake`]: photos through [`RecordingTransport`], the article through
//! [`ScriptedSite`], one ordered log each.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::error::Error;
use newsbuilder_core::model::item::NewsItem;
use newsbuilder_core::model::server::ServerConfig;
use newsbuilder_core::model::site::{ArticleConfirmation, ArticleOutcome, ArticleState};
use newsbuilder_core::publish::{PublishMode, paths, publish, publish_to_site};
use pretty_assertions::assert_eq;
use support::fakes::{
    FakeSecretStore, NEW_ARTICLE_ID, RecordingTransport, RemoteFake, ScriptedSite, SiteOp,
    server_config, site_server_config,
};

const CASE: &str = "markers";
const SERVER: &str = "editorial";

fn item() -> NewsItem {
    support::item(CASE).expect("the markers fixture imports")
}

fn secrets(server: &ServerConfig) -> FakeSecretStore {
    FakeSecretStore::new().with_secret(&server.credential, "correct-horse-battery-staple")
}

fn run(
    item: &NewsItem,
    server: &ServerConfig,
    remote: &mut RemoteFake,
    mode: PublishMode,
) -> newsbuilder_core::Result<newsbuilder_core::publish::Publication> {
    publish_to_site(
        item,
        server,
        remote,
        &secrets(server),
        mode,
        ArticleConfirmation::None,
        &EmbeddedBytes,
    )
}

#[test]
fn a_first_publish_creates_one_article_holding_the_fragment() {
    let item = item();
    let server = site_server_config(SERVER);
    let mut remote = RemoteFake::default();

    let publication = run(&item, &server, &mut remote, PublishMode::Live).expect("publishes");

    let saves = remote.site.saves();
    assert_eq!(saves.len(), 1, "{:?}", remote.site.log());
    let write = saves[0];
    assert_eq!(write.id, None);
    assert!(!write.restore);
    assert_eq!(write.alias, publication.folder.as_str());
    assert_eq!(write.title, item.title);
    // INV-S4: the article's text is the fragment, byte for byte.
    assert_eq!(write.articletext, publication.fragment);
    assert_eq!(write.settings.category, Some(8));
    // Clarification Q3: published unless someone said otherwise.
    assert_eq!(write.settings.state, Some(ArticleState::Published));

    assert_eq!(
        publication.article,
        Some(ArticleOutcome::Created {
            id: NEW_ARTICLE_ID,
            url: ScriptedSite::url_of(NEW_ARTICLE_ID),
        })
    );
    assert_eq!(publication.uploaded.len(), 5, "the photos still go up");
}

#[test]
fn the_article_is_looked_for_before_it_is_written() {
    let item = item();
    let server = site_server_config(SERVER);
    let mut remote = RemoteFake::default();

    run(&item, &server, &mut remote, PublishMode::Live).expect("publishes");

    assert!(
        matches!(remote.site.log(), [SiteOp::Find(_), SiteOp::Save(_)]),
        "{:?}",
        remote.site.log()
    );
}

#[test]
fn a_dry_run_reports_the_article_it_would_create_and_writes_nothing() {
    let item = item();
    let server = site_server_config(SERVER);
    let mut remote = RemoteFake::default();

    let publication = run(&item, &server, &mut remote, PublishMode::DryRun).expect("plans");

    assert!(remote.site.saves().is_empty(), "{:?}", remote.site.log());
    assert!(!remote.transport.mutated());
    match publication.article {
        Some(ArticleOutcome::WouldCreate { settings }) => {
            assert_eq!(settings.category, Some(8));
            assert_eq!(settings.state, Some(ArticleState::Published));
        }
        other => panic!("expected WouldCreate, got {other:?}"),
    }
}

#[test]
fn a_failed_upload_touches_no_article() {
    // FR-004: the article is written only once every photo is on the server.
    let item = item();
    let server = site_server_config(SERVER);
    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    let transport =
        RecordingTransport::new().refusing_to_put(format!("{folder}/marker-coverage-03.jpg"));
    let mut remote = RemoteFake::new(transport, ScriptedSite::new());

    let result = run(&item, &server, &mut remote, PublishMode::Live);

    assert!(matches!(result, Err(Error::Transport { .. })), "{result:?}");
    assert!(remote.site.log().is_empty(), "{:?}", remote.site.log());
}

#[test]
fn a_failed_article_keeps_the_photos_and_the_fragment() {
    // FR-015, US1 scenario 5: the editor is told which step failed and can still paste.
    let item = item();
    let server = site_server_config(SERVER);
    let mut site = ScriptedSite::new();
    site.save_fails = Some("saving the article".to_owned());
    let mut remote = RemoteFake::new(RecordingTransport::new(), site);

    let publication = run(&item, &server, &mut remote, PublishMode::Live)
        .expect("a failed article is an outcome, not an error");

    assert!(!publication.fragment.is_empty());
    assert_eq!(publication.uploaded.len(), 5);
    match publication.article {
        Some(ArticleOutcome::Failed { step, detail }) => {
            assert_eq!(step, "saving the article");
            assert!(!detail.is_empty());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_failed_lookup_is_reported_the_same_way() {
    let item = item();
    let server = site_server_config(SERVER);
    let mut site = ScriptedSite::new();
    site.find_fails = Some("starting Joomla".to_owned());
    let mut remote = RemoteFake::new(RecordingTransport::new(), site);

    let publication = run(&item, &server, &mut remote, PublishMode::Live).expect("an outcome");

    assert!(
        matches!(&publication.article, Some(ArticleOutcome::Failed { step, .. }) if step == "starting Joomla"),
        "{:?}",
        publication.article
    );
    assert!(remote.site.saves().is_empty());
}

#[test]
fn without_a_site_target_a_publish_is_exactly_what_it_was() {
    // INV-S5 / FR-013.
    let item = item();
    let server = server_config(SERVER);
    let mut remote = RemoteFake::default();
    let mut plain = RecordingTransport::new();

    let with_site = run(&item, &server, &mut remote, PublishMode::Live).expect("publishes");
    let without = publish(
        &item,
        &server,
        &mut plain,
        &secrets(&server),
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes");

    assert_eq!(with_site.article, None);
    assert_eq!(with_site.fragment, without.fragment);
    assert_eq!(with_site.folder, without.folder);
    assert_eq!(with_site.remote_folder, without.remote_folder);
    assert_eq!(with_site.uploaded, without.uploaded);
    assert_eq!(with_site.unchanged, without.unchanged);
    assert_eq!(remote.transport.log(), plain.log());
    assert!(remote.site.log().is_empty());
}

#[test]
fn a_site_target_without_a_category_is_refused_before_anything_happens() {
    // INV-S1.
    let item = item();
    let mut server = site_server_config(SERVER);
    if let Some(site) = server.site.as_mut() {
        site.defaults.category = None;
    }
    let mut remote = RemoteFake::default();

    let result = run(&item, &server, &mut remote, PublishMode::Live);

    assert!(
        matches!(&result, Err(Error::SiteIncomplete { server, .. }) if server == SERVER),
        "{result:?}"
    );
    assert!(
        remote.transport.log().is_empty(),
        "{:?}",
        remote.transport.log()
    );
    assert!(remote.site.log().is_empty());
}

#[test]
fn an_item_override_wins_over_the_server_default() {
    let mut item = item();
    item.article.category = Some(12);
    item.article.state = Some(ArticleState::Unpublished);
    let server = site_server_config(SERVER);
    let mut remote = RemoteFake::default();

    run(&item, &server, &mut remote, PublishMode::Live).expect("publishes");

    let write = remote.site.saves()[0].clone();
    assert_eq!(write.settings.category, Some(12));
    assert_eq!(write.settings.state, Some(ArticleState::Unpublished));
}

// ---------------------------------------------------------------------------------------------
// The cover: Joomla's intro image
// ---------------------------------------------------------------------------------------------

fn cover_of(item: &NewsItem, server: &ServerConfig) -> Option<String> {
    let mut remote = RemoteFake::default();
    run(item, server, &mut remote, PublishMode::Live).expect("publishes");
    remote.site.saves()[0].intro_image.clone()
}

#[test]
fn the_cover_is_the_first_photo_in_the_text_unless_chosen() {
    use newsbuilder_core::model::site::IntroImage;

    let item = item();
    assert_eq!(item.intro_image, IntroImage::First);
    let server = site_server_config(SERVER);
    let first = item.placed_photo_ids()[0];

    let mut remote = RemoteFake::default();
    let publication = run(&item, &server, &mut remote, PublishMode::Live).expect("publishes");
    let expected = publication
        .photo_urls
        .iter()
        .find(|(id, _)| *id == first)
        // The photos live under the fake site's own address, so the cover is site-relative.
        .map(|(_, url)| {
            url.as_str()
                .trim_start_matches("https://example.org/")
                .to_owned()
        });
    assert!(
        expected.as_deref().is_some_and(|c| c.starts_with("news/")),
        "{expected:?}"
    );
    assert_eq!(remote.site.saves()[0].intro_image, expected);
}

#[test]
fn a_chosen_photo_becomes_the_cover() {
    use newsbuilder_core::model::site::IntroImage;

    let mut item = item();
    let third = item.placed_photo_ids()[2];
    item.intro_image = IntroImage::Photo(third);
    let server = site_server_config(SERVER);

    let cover = cover_of(&item, &server).expect("a cover");
    assert!(cover.ends_with("-03.jpg"), "{cover}");
}

#[test]
fn no_cover_clears_it() {
    use newsbuilder_core::model::site::IntroImage;

    let mut item = item();
    item.intro_image = IntroImage::None;
    assert_eq!(
        cover_of(&item, &site_server_config(SERVER)),
        Some(String::new())
    );
}

#[test]
fn a_cover_under_the_site_is_given_as_a_site_relative_path() {
    // Joomla's media fields expect `images/…`, relative to the site root, for local files.
    let item = item();
    let mut server = site_server_config(SERVER);
    server.public_base_url = url::Url::parse("https://example.org/images/news/").expect("valid");

    let cover = cover_of(&item, &server).expect("a cover");
    assert!(cover.starts_with("images/news/"), "{cover}");
}

#[test]
fn a_cover_that_is_not_placed_is_refused_before_anything_is_uploaded() {
    use newsbuilder_core::model::photo::PhotoId;
    use newsbuilder_core::model::site::IntroImage;

    let mut item = item();
    item.intro_image = IntroImage::Photo(PhotoId(999));
    let server = site_server_config(SERVER);
    let mut remote = RemoteFake::default();

    let result = run(&item, &server, &mut remote, PublishMode::Live);

    assert!(
        matches!(&result, Err(Error::InvalidArticleSetting { field, .. }) if field == "intro_image"),
        "{result:?}"
    );
    assert!(remote.transport.log().is_empty());
}
