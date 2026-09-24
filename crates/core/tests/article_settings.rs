//! 002 User Story 3: article settings — resolution, validation, and reading the site (T029).

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::error::Error;
use newsbuilder_core::model::site::{
    ArticleConfirmation, ArticleSettings, ArticleState, Category, SiteCatalog,
};
use newsbuilder_core::publish::{PublishMode, check_site, publish_to_site};
use pretty_assertions::assert_eq;
use support::fakes::{
    FakeSecretStore, RecordingTransport, RemoteFake, ScriptedSite, SiteOp, TransportOp,
    server_config, site_server_config,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn at(text: &str) -> OffsetDateTime {
    OffsetDateTime::parse(text, &Rfc3339).expect("a valid instant")
}

#[test]
fn an_override_wins_a_default_fills_in_and_the_rest_stays_unset() {
    let defaults = ArticleSettings {
        category: Some(8),
        featured: Some(false),
        language: Some("ru-RU".to_owned()),
        author: Some(42),
        ..ArticleSettings::default()
    };
    let overrides = ArticleSettings {
        featured: Some(true),
        publish_up: Some(at("2026-09-20T09:00:00+03:00")),
        ..ArticleSettings::default()
    };

    let resolved = ArticleSettings::resolve(&overrides, &defaults);

    assert_eq!(
        resolved,
        ArticleSettings {
            category: Some(8),
            state: Some(ArticleState::Published),
            featured: Some(true),
            access: None,
            language: Some("ru-RU".to_owned()),
            author: Some(42),
            author_alias: None,
            publish_up: Some(at("2026-09-20T09:00:00+03:00")),
            publish_down: None,
            meta_description: None,
            tags: None,
        }
    );
}

#[test]
fn the_state_is_published_unless_someone_said_otherwise() {
    // Clarification Q3.
    let neither =
        ArticleSettings::resolve(&ArticleSettings::default(), &ArticleSettings::default());
    assert_eq!(neither.state, Some(ArticleState::Published));

    let server_says = ArticleSettings {
        state: Some(ArticleState::Unpublished),
        ..ArticleSettings::default()
    };
    assert_eq!(
        ArticleSettings::resolve(&ArticleSettings::default(), &server_says).state,
        Some(ArticleState::Unpublished)
    );
}

#[test]
fn inv_s2_a_publication_must_finish_after_it_starts() {
    let settings = ArticleSettings {
        publish_up: Some(at("2026-09-20T09:00:00+03:00")),
        publish_down: Some(at("2026-09-20T06:00:00Z")),
        ..ArticleSettings::default()
    };
    assert!(
        matches!(settings.validate(), Err(Error::InvalidArticleSetting { field, .. }) if field == "publish_down")
    );
}

#[test]
fn a_meta_description_must_fit_and_stay_on_one_line() {
    let long = ArticleSettings {
        meta_description: Some("а".repeat(301)),
        ..ArticleSettings::default()
    };
    assert!(long.validate().is_err());

    let broken = ArticleSettings {
        meta_description: Some("one\ntwo".to_owned()),
        ..ArticleSettings::default()
    };
    assert!(broken.validate().is_err());

    let fine = ArticleSettings {
        meta_description: Some("а".repeat(300)),
        ..ArticleSettings::default()
    };
    assert!(fine.validate().is_ok());
}

#[test]
fn an_invalid_override_is_refused_before_any_photo_moves() {
    let mut item = support::item("markers").expect("imports");
    item.article.meta_description = Some("one\ntwo".to_owned());
    let server = site_server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "secret");
    let mut remote = RemoteFake::default();

    let result = publish_to_site(
        &item,
        &server,
        &mut remote,
        &secrets,
        PublishMode::Live,
        ArticleConfirmation::None,
        &EmbeddedBytes,
    );

    assert!(
        matches!(result, Err(Error::InvalidArticleSetting { .. })),
        "{result:?}"
    );
    assert!(remote.transport.log().is_empty());
}

#[test]
fn checking_a_site_connects_and_reads_and_writes_nothing() {
    let server = site_server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "secret");
    let mut site = ScriptedSite::new();
    site.catalog = SiteCatalog {
        joomla_version: "5.2.3".to_owned(),
        categories: vec![Category {
            id: 8,
            title: "Новости".to_owned(),
            level: 1,
            published: true,
            language: "*".to_owned(),
        }],
        ..SiteCatalog::default()
    };
    let mut remote = RemoteFake::new(RecordingTransport::new(), site);

    let catalog = check_site(&server, &mut remote, &secrets).expect("reads");

    assert_eq!(catalog.joomla_version, "5.2.3");
    assert_eq!(catalog.categories[0].title, "Новости");
    assert_eq!(
        remote.transport.log(),
        &[TransportOp::Connect {
            server: "editorial".to_owned()
        }]
    );
    assert_eq!(remote.site.log(), &[SiteOp::Describe]);
}

#[test]
fn checking_a_site_does_not_need_a_category_yet() {
    let mut server = site_server_config("editorial");
    if let Some(site) = server.site.as_mut() {
        site.defaults.category = None;
    }
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "secret");
    let mut remote = RemoteFake::default();

    assert!(check_site(&server, &mut remote, &secrets).is_ok());
}

#[test]
fn checking_a_server_with_no_site_is_refused_by_name() {
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "secret");
    let mut remote = RemoteFake::default();

    let result = check_site(&server, &mut remote, &secrets);

    assert!(
        matches!(&result, Err(Error::SiteIncomplete { server, .. }) if server == "editorial"),
        "{result:?}"
    );
    assert!(remote.transport.log().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Tags (метки)
// ---------------------------------------------------------------------------------------------

#[test]
fn item_tags_replace_the_server_tags_rather_than_adding_to_them() {
    let defaults = ArticleSettings {
        tags: Some(vec![3, 5]),
        ..ArticleSettings::default()
    };
    let overrides = ArticleSettings {
        tags: Some(vec![7]),
        ..ArticleSettings::default()
    };
    assert_eq!(
        ArticleSettings::resolve(&overrides, &defaults).tags,
        Some(vec![7])
    );
    assert_eq!(
        ArticleSettings::resolve(&ArticleSettings::default(), &defaults).tags,
        Some(vec![3, 5])
    );
    // An explicitly empty list means "no tags", not "not set".
    let none = ArticleSettings {
        tags: Some(vec![]),
        ..ArticleSettings::default()
    };
    assert_eq!(
        ArticleSettings::resolve(&none, &defaults).tags,
        Some(vec![])
    );
}

#[test]
fn tags_are_sent_sorted_and_once_each() {
    let settings = ArticleSettings {
        tags: Some(vec![9, 3, 9, 5]),
        ..ArticleSettings::default()
    };
    assert_eq!(
        ArticleSettings::resolve(&settings, &ArticleSettings::default()).tags,
        Some(vec![3, 5, 9])
    );
}

#[test]
fn tag_zero_is_not_a_tag() {
    let settings = ArticleSettings {
        tags: Some(vec![0]),
        ..ArticleSettings::default()
    };
    assert!(
        matches!(settings.validate(), Err(Error::InvalidArticleSetting { field, .. }) if field == "tags")
    );
}
