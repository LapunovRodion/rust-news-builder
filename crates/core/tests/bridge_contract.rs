//! The bridge protocol, pinned to golden messages (002 T012, contracts/bridge-protocol.md).
//!
//! The PHP bridge and the Rust side meet only in these JSON documents. Pinning them to files
//! means a change to either side's shape is a reviewed diff in `fixtures/bridge/`, not a
//! surprise on a live site.

mod support;

use newsbuilder_core::adapters::joomla;
use newsbuilder_core::error::Error;
use newsbuilder_core::model::site::{
    ArticleProbe, ArticleSettings, ArticleState, ArticleWrite, FindResult, SavedArticle,
    SiteCatalog,
};
use pretty_assertions::assert_eq;
use serde_json::Value;
use support::fakes::site_target;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn golden(name: &str) -> Value {
    let path = support::repo_root().join("fixtures/bridge").join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} could not be read: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name} is not JSON: {e}"))
}

/// The golden as the bridge would print it: one line after the sentinel.
fn answered(name: &str) -> Vec<u8> {
    format!(
        "PHP Notice: some extension talks\n{}{}\n",
        joomla::SENTINEL,
        golden(name)
    )
    .into_bytes()
}

const TEXT: &str = "<hr id=\"system-readmore\"/><p>Текст «с кавычками» & 'апострофами'.</p>";

#[test]
fn the_describe_request_matches_its_golden() {
    assert_eq!(
        joomla::describe_request(&site_target()),
        golden("describe.request.json")
    );
}

#[test]
fn the_find_request_matches_its_golden() {
    let probe = ArticleProbe {
        alias: "den-konstitutsii".to_owned(),
        category: 8,
        title: "День Конституции".to_owned(),
        articletext: TEXT.to_owned(),
        settings: ArticleSettings {
            category: Some(8),
            state: Some(ArticleState::Published),
            featured: Some(false),
            ..ArticleSettings::default()
        },
        intro_image: None,
    };
    assert_eq!(
        joomla::find_request(&site_target(), &probe),
        golden("find.request.json")
    );
}

#[test]
fn the_save_request_matches_its_golden() {
    let write = ArticleWrite {
        id: None,
        restore: false,
        alias: "den-konstitutsii".to_owned(),
        title: "День Конституции".to_owned(),
        articletext: TEXT.to_owned(),
        settings: ArticleSettings {
            category: Some(8),
            state: Some(ArticleState::Published),
            featured: Some(false),
            access: Some(1),
            language: Some("*".to_owned()),
            author: Some(42),
            author_alias: Some("Пресс-служба".to_owned()),
            publish_up: Some(
                OffsetDateTime::parse("2026-09-17T10:00:00+03:00", &Rfc3339).expect("valid"),
            ),
            publish_down: None,
            meta_description: Some("Кратко о главном".to_owned()),
            tags: Some(vec![3, 5]),
        },
        intro_image: Some("images/news/den-konstitutsii/den-konstitutsii-01.jpg".to_owned()),
    };
    assert_eq!(
        joomla::save_request(&site_target(), &write),
        golden("save.request.json")
    );
}

#[test]
fn a_save_request_leaves_out_every_setting_nobody_chose() {
    // FR-012: an absent key is Joomla's default; a key sent as null would not be.
    let write = ArticleWrite {
        id: Some(1234),
        restore: false,
        alias: "den-konstitutsii".to_owned(),
        title: "День Конституции".to_owned(),
        articletext: "<hr id=\"system-readmore\"/><p>Текст.</p>".to_owned(),
        settings: ArticleSettings {
            category: Some(8),
            state: Some(ArticleState::Published),
            ..ArticleSettings::default()
        },
        // No `intro_image` key at all: the cover is left as it is.
        intro_image: None,
    };
    assert_eq!(
        joomla::save_request(&site_target(), &write),
        golden("save.minimal.request.json")
    );
}

#[test]
fn the_describe_response_reads_into_a_catalog() {
    let catalog: SiteCatalog = joomla::parse(
        "reading the site",
        &answered("describe.response.json"),
        b"",
        Some(0),
    )
    .expect("parses");
    assert_eq!(catalog.joomla_version, "5.2.3");
    assert_eq!(catalog.categories.len(), 3);
    assert_eq!(catalog.categories[2].level, 2);
    assert_eq!(catalog.languages[0].code, "*");
    assert_eq!(catalog.authors[0].id, 42);
    assert_eq!(catalog.tags[1].title, "Конференции");
    assert_eq!(catalog.tags[1].level, 2);
}

#[test]
fn the_find_response_reads_into_a_result() {
    let found: FindResult = joomla::parse(
        "reading articles",
        &answered("find.response.json"),
        b"",
        Some(0),
    )
    .expect("parses");
    assert_eq!(found.articles.len(), 1);
    assert_eq!(found.articles[0].id, 1234);
    assert!(found.articles[0].content_matches_mark);
    assert_eq!(found.alias_taken_by, None);
}

#[test]
fn the_save_response_reads_into_a_saved_article() {
    let saved: SavedArticle = joomla::parse(
        "saving the article",
        &answered("save.response.json"),
        b"",
        Some(0),
    )
    .expect("parses");
    assert_eq!(saved.id, 1234);
    assert!(saved.created);
}

#[test]
fn a_missing_category_is_reported_by_its_id() {
    let result: newsbuilder_core::Result<SavedArticle> = joomla::parse(
        "saving the article",
        &answered("error.category_missing.json"),
        b"",
        Some(0),
    );
    assert!(
        matches!(result, Err(Error::CategoryMissing { id: 8 })),
        "{result:?}"
    );
}

#[test]
fn an_unsupported_joomla_is_reported_by_its_version() {
    let result: newsbuilder_core::Result<SiteCatalog> = joomla::parse(
        "reading the site",
        &answered("error.unsupported_joomla.json"),
        b"",
        Some(0),
    );
    assert!(
        matches!(&result, Err(Error::SiteUnsupported { found }) if found == "3.10.12"),
        "{result:?}"
    );
}
