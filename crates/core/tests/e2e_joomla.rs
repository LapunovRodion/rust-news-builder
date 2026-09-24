//! 002 end to end: a real Joomla, reached over a real SSH session (T015, T025, T030).
//!
//! Everything else about site insertion is tested against `ScriptedSite`. What a fake cannot
//! prove is that the bridge actually boots Joomla from a shell, that Joomla's own article model
//! accepts what it is given, and that `find` reads back what `save` wrote. That is this file.
//!
//! ```text
//! just e2e-joomla     # containers up, this test, containers down
//! ```
//!
//! It needs `docker compose`, `ssh-keyscan`, and the stack in `tools/e2e-joomla/` listening on
//! 127.0.0.1:2222. Without `NEWSBUILDER_E2E_JOOMLA` it skips with a message.

#![cfg(feature = "sftp")]

mod support;

use std::path::PathBuf;
use std::process::Command;

use newsbuilder_core::adapters::transport::SftpTransport;
use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::model::item::NewsItem;
use newsbuilder_core::model::server::{CredentialRef, ServerConfig};
use newsbuilder_core::model::site::{
    ArticleConfirmation, ArticleOutcome, ArticleSettings, ArticleState, ConfirmationReason,
    SiteTarget,
};
use newsbuilder_core::publish::{Publication, PublishMode, check_site, publish_to_site};
use support::fakes::FakeSecretStore;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const SWITCH: &str = "NEWSBUILDER_E2E_JOOMLA";
const COMPOSE: &str = "tools/e2e-joomla/compose.yml";
const PASSWORD: &str = "e2e-editor-password";

fn enabled() -> bool {
    std::env::var(SWITCH).is_ok_and(|v| !v.is_empty() && v != "0")
}

fn compose(args: &[&str]) -> String {
    let output = Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(support::repo_root().join(COMPOSE))
        .args(args)
        .output()
        .expect("docker compose runs");
    assert!(
        output.status.success(),
        "docker compose {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs SQL against the site's database, with `#__` replaced by the real table prefix.
fn sql(query: &str) -> String {
    let prefix = compose(&[
        "exec",
        "-T",
        "db",
        "mariadb",
        "-N",
        "-ujoomla",
        "-pe2e-db-password",
        "joomla",
        "-e",
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'joomla' \
         AND table_name LIKE '%\\_content' AND table_name NOT LIKE '%\\_ucm\\_content' LIMIT 1",
    ]);
    let prefix = prefix.trim().trim_end_matches("content");
    let query = query.replace("#__", prefix);
    compose(&[
        "exec",
        "-T",
        "db",
        "mariadb",
        "-N",
        "-ujoomla",
        "-pe2e-db-password",
        "joomla",
        "-e",
        &query,
    ])
    .trim()
    .to_owned()
}

fn server(site: SiteTarget) -> ServerConfig {
    ServerConfig {
        name: "e2e-joomla".to_owned(),
        host: "127.0.0.1".to_owned(),
        user: "editor".to_owned(),
        port: 2222,
        remote_base_path: "/srv/newsbuilder-photos".to_owned(),
        public_base_url: url::Url::parse("http://127.0.0.1:8088/photos/").expect("valid"),
        credential: CredentialRef::Password {
            server: "e2e-joomla".to_owned(),
        },
        site: Some(site),
    }
}

fn site(category: Option<u32>) -> SiteTarget {
    SiteTarget {
        joomla_root: "/var/www/html".to_owned(),
        site_url: url::Url::parse("http://127.0.0.1:8088/").expect("valid"),
        php: "php".to_owned(),
        defaults: ArticleSettings {
            category,
            ..ArticleSettings::default()
        },
    }
}

fn run(
    item: &NewsItem,
    server: &ServerConfig,
    mode: PublishMode,
    confirmation: ArticleConfirmation,
) -> (Publication, ArticleOutcome) {
    let secrets = FakeSecretStore::new().with_secret(&server.credential, PASSWORD);
    let mut remote = SftpTransport::new().expect("a transport");
    let publication = publish_to_site(
        item,
        server,
        &mut remote,
        &secrets,
        mode,
        confirmation,
        &EmbeddedBytes,
    )
    .expect("the photos publish");
    let outcome = publication.article.clone().expect("the server has a site");
    (publication, outcome)
}

fn created_id(outcome: &ArticleOutcome) -> u32 {
    match outcome {
        ArticleOutcome::Created { id, .. } => *id,
        other => panic!("expected Created, got {other:?}"),
    }
}

#[test]
#[ignore = "needs the tools/e2e-joomla containers; run with `just e2e-joomla`"]
fn a_real_joomla_receives_updates_and_protects_its_articles() {
    if !enabled() {
        eprintln!("skipped: {SWITCH} is not set. Run `just e2e-joomla`.");
        return;
    }

    // The container's host key, known only to this test.
    let home: PathBuf = std::env::temp_dir().join(format!("nb-e2e-joomla-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("a temp dir");
    let keys = Command::new("ssh-keyscan")
        .args(["-p", "2222", "127.0.0.1"])
        .output()
        .expect("ssh-keyscan runs");
    let known_hosts = home.join("known_hosts");
    std::fs::write(&known_hosts, keys.stdout).expect("known_hosts written");
    // SAFETY: this binary holds one test, so nothing reads the environment concurrently.
    unsafe {
        std::env::set_var("SSH_KNOWN_HOSTS", &known_hosts);
    }

    // ---------------------------------------------------------------------------------------
    // describe: the site's own choices (T030)
    // ---------------------------------------------------------------------------------------
    let secrets = FakeSecretStore::new().with_secret(
        &CredentialRef::Password {
            server: "e2e-joomla".to_owned(),
        },
        PASSWORD,
    );
    let catalog = check_site(
        &server(site(None)),
        &mut SftpTransport::new().expect("a transport"),
        &secrets,
    )
    .expect("the site describes itself");
    assert!(
        catalog.joomla_version.starts_with('5'),
        "{}",
        catalog.joomla_version
    );
    assert!(catalog.languages.iter().any(|l| l.code == "*"));
    assert!(!catalog.access_levels.is_empty());
    let category = catalog
        .categories
        .iter()
        .find(|c| c.title == "Uncategorised")
        .expect("a fresh Joomla has Uncategorised")
        .id;

    let server = server(site(Some(category)));
    let mut item = support::item("markers").expect("the markers fixture imports");
    let alias = item.slug.as_str().to_owned();
    let count = || {
        sql(&format!(
            "SELECT COUNT(*) FROM #__content WHERE alias = '{alias}'"
        ))
    };

    // ---------------------------------------------------------------------------------------
    // US1: dry run, create, failure
    // ---------------------------------------------------------------------------------------
    let (_, outcome) = run(
        &item,
        &server,
        PublishMode::DryRun,
        ArticleConfirmation::None,
    );
    assert!(
        matches!(outcome, ArticleOutcome::WouldCreate { .. }),
        "{outcome:?}"
    );
    assert_eq!(count(), "0", "a dry run writes nothing");

    let (publication, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    let id = created_id(&outcome);
    assert_eq!(count(), "1");
    let stored = sql(&format!(
        "SELECT CONCAT(introtext, '<hr id=\"system-readmore\"/>', `fulltext`) FROM #__content WHERE id = {id}"
    ));
    let expected: String = publication.fragment.replace('\n', "\\n");
    assert_eq!(stored, expected, "the article holds the fragment");
    assert_eq!(
        sql(&format!("SELECT state FROM #__content WHERE id = {id}")),
        "1"
    );
    assert!(
        sql(&format!("SELECT note FROM #__content WHERE id = {id}")).starts_with("newsbuilder:")
    );

    // ---------------------------------------------------------------------------------------
    // US2: unchanged, update, edited on site, trashed, gone
    // ---------------------------------------------------------------------------------------
    let modified = sql(&format!("SELECT modified FROM #__content WHERE id = {id}"));
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Unchanged { id: same, .. } if same == id),
        "{outcome:?}"
    );
    assert_eq!(
        sql(&format!("SELECT modified FROM #__content WHERE id = {id}")),
        modified
    );

    // A corrected paragraph. (A corrected *title* renames the photos, which 001 treats as a
    // different item in a different folder — so it gets a new article. Known, and reported.)
    if let Some(newsbuilder_core::model::item::Block::Paragraph { text, .. }) =
        item.body.iter_mut().find(|block| {
            matches!(
                block,
                newsbuilder_core::model::item::Block::Paragraph { .. }
            )
        })
    {
        text.push_str(" (исправлено)");
    }
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { id: same, .. } if same == id),
        "{outcome:?}"
    );
    assert_eq!(count(), "1", "an update never duplicates");

    sql(&format!(
        "UPDATE #__content SET introtext = CONCAT(introtext, '<p>edited</p>') WHERE id = {id}"
    ));
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert_eq!(
        outcome,
        ArticleOutcome::NeedsConfirmation {
            reason: ConfirmationReason::EditedOnSite,
            id: Some(id)
        }
    );
    let (_, outcome) = run(
        &item,
        &server,
        PublishMode::Live,
        ArticleConfirmation::Overwrite,
    );
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );

    sql(&format!("UPDATE #__content SET state = -2 WHERE id = {id}"));
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(
            outcome,
            ArticleOutcome::NeedsConfirmation {
                reason: ConfirmationReason::Trashed,
                ..
            }
        ),
        "{outcome:?}"
    );
    let (_, outcome) = run(
        &item,
        &server,
        PublishMode::Live,
        ArticleConfirmation::Overwrite,
    );
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        sql(&format!("SELECT state FROM #__content WHERE id = {id}")),
        "1",
        "restored"
    );

    sql(&format!("DELETE FROM #__content WHERE id = {id}"));
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(
            outcome,
            ArticleOutcome::NeedsConfirmation {
                reason: ConfirmationReason::Gone,
                ..
            }
        ),
        "{outcome:?}"
    );
    let (_, outcome) = run(
        &item,
        &server,
        PublishMode::Live,
        ArticleConfirmation::CreateNew,
    );
    let id = created_id(&outcome);

    // ---------------------------------------------------------------------------------------
    // US3: overrides land as written, dates in UTC
    // ---------------------------------------------------------------------------------------
    item.article = ArticleSettings {
        featured: Some(true),
        access: Some(2),
        state: Some(ArticleState::Unpublished),
        publish_up: Some(
            OffsetDateTime::parse("2030-01-02T12:00:00+03:00", &Rfc3339).expect("valid"),
        ),
        meta_description: Some("Кратко о главном".to_owned()),
        author_alias: Some("Пресс-служба".to_owned()),
        ..ArticleSettings::default()
    };
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        sql(&format!(
            "SELECT CONCAT_WS('|', featured, access, state, publish_up, metadesc, created_by_alias) \
             FROM #__content WHERE id = {id}"
        )),
        "1|2|0|2030-01-02 09:00:00|Кратко о главном|Пресс-служба"
    );

    // ---------------------------------------------------------------------------------------
    // The cover: first photo by default, a chosen one on request, merged into `images`
    // ---------------------------------------------------------------------------------------
    let cover = || {
        sql(&format!(
            "SELECT JSON_UNQUOTE(JSON_EXTRACT(images, '$.image_intro')) FROM #__content WHERE id = {id}"
        ))
    };
    assert!(
        cover().starts_with(&format!("photos/{alias}/")) && cover().ends_with("-01.jpg"),
        "the first photo, site-relative: {}",
        cover()
    );

    // Something an editor set by hand on the same tab must survive.
    sql(&format!(
        "UPDATE #__content SET images = JSON_SET(images, '$.image_fulltext', 'images/kept.jpg') WHERE id = {id}"
    ));
    item.intro_image = newsbuilder_core::model::site::IntroImage::Photo(item.placed_photo_ids()[2]);
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert!(cover().ends_with("-03.jpg"), "{}", cover());
    assert_eq!(
        sql(&format!(
            "SELECT JSON_UNQUOTE(JSON_EXTRACT(images, '$.image_fulltext')) FROM #__content WHERE id = {id}"
        )),
        "images/kept.jpg"
    );
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Unchanged { .. }),
        "{outcome:?}"
    );

    // ---------------------------------------------------------------------------------------
    // Tags: read from the site, written through Joomla's own tag handling, compared on re-publish
    // ---------------------------------------------------------------------------------------
    // Two tags under the root, as the tree Joomla keeps (root lft 0, rgt 5).
    sql("UPDATE #__tags SET rgt = 5 WHERE id = 1");
    for (lft, alias, title) in [(1, "nauka", "Наука"), (3, "konferentsii", "Конференции")]
    {
        sql(&format!(
            "INSERT INTO #__tags (parent_id, lft, rgt, level, path, title, alias, description, \
             published, access, params, metadesc, metadata, created_time, modified_time, images, \
             urls, language) VALUES (1, {lft}, {rgt}, 1, '{alias}', '{title}', '{alias}', '', 1, 1, \
             '{{}}', '', '{{}}', NOW(), NOW(), '{{}}', '{{}}', '*')",
            rgt = lft + 1
        ));
    }
    let tags = check_site(
        &server,
        &mut SftpTransport::new().expect("a transport"),
        &secrets,
    )
    .expect("the site describes itself")
    .tags;
    assert_eq!(
        tags.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        ["Наука", "Конференции"]
    );
    let (science, conferences) = (tags[0].id, tags[1].id);
    let tags_of = || {
        sql(&format!(
            "SELECT GROUP_CONCAT(tag_id ORDER BY tag_id) FROM #__contentitem_tag_map \
             WHERE type_alias = 'com_content.article' AND content_item_id = {id}"
        ))
    };

    item.article.tags = Some(vec![conferences, science]);
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert_eq!(tags_of(), format!("{science},{conferences}"));
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Unchanged { .. }),
        "same tags: {outcome:?}"
    );

    item.article.tags = Some(vec![science]);
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert_eq!(tags_of(), science.to_string());

    item.article.tags = Some(vec![]);
    let (_, outcome) = run(&item, &server, PublishMode::Live, ArticleConfirmation::None);
    assert!(
        matches!(outcome, ArticleOutcome::Updated { .. }),
        "{outcome:?}"
    );
    assert_eq!(tags_of(), "NULL", "an empty list clears the tags");

    // ---------------------------------------------------------------------------------------
    // US1 scenario 5: a broken site still leaves the fragment
    // ---------------------------------------------------------------------------------------
    let mut broken = server.clone();
    if let Some(site) = broken.site.as_mut() {
        site.joomla_root = "/nonexistent".to_owned();
    }
    let (publication, outcome) = run(&item, &broken, PublishMode::Live, ArticleConfirmation::None);
    assert!(!publication.fragment.is_empty());
    assert!(
        matches!(&outcome, ArticleOutcome::Failed { .. }),
        "{outcome:?}"
    );
}
