//! Dry run plans everything and mutates nothing (T051, FR-031).
//!
//! `build` consults no port, so the plan is produced by exactly the code a live publish runs —
//! the only difference is that the two mutating [`Transport`](newsbuilder_core::ports::Transport)
//! methods are never reached. That is why the reported URL set can be trusted.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::publish::{PublishMode, paths, publish};
use pretty_assertions::assert_eq;
use support::fakes::{FakeSecretStore, RecordingTransport, TransportOp, server_config};

#[test]
fn a_dry_run_touches_nothing_and_still_reports_every_url() {
    let item = support::item("markers").expect("the markers fixture imports");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "a-password");
    let mut transport = RecordingTransport::new();

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::DryRun,
        &EmbeddedBytes,
    )
    .expect("plans");

    assert!(publication.dry_run);
    assert!(
        !transport.mutated(),
        "a dry run changed remote state: {:?}",
        transport.log()
    );
    assert!(
        transport
            .log()
            .iter()
            .all(|op| matches!(op, TransportOp::Connect { .. } | TransportOp::List { .. })),
        "a dry run did more than look: {:?}",
        transport.log()
    );

    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    assert!(
        transport.names_in(&folder).is_empty(),
        "a dry run created files"
    );

    // The plan is complete: every photo, with the URL a live publish would produce.
    let planned: Vec<String> = publication
        .uploaded
        .iter()
        .map(|(_, url)| url.to_string())
        .collect();
    assert_eq!(
        planned,
        vec![
            "https://example.org/news/2026/03/marker-coverage/marker-coverage-01.jpg",
            "https://example.org/news/2026/03/marker-coverage/marker-coverage-02.jpg",
            "https://example.org/news/2026/03/marker-coverage/marker-coverage-03.jpg",
            "https://example.org/news/2026/03/marker-coverage/marker-coverage-04.jpg",
            "https://example.org/news/2026/03/marker-coverage/marker-coverage-05.jpg",
        ]
    );
}

#[test]
fn a_dry_run_after_a_live_publish_reports_the_files_as_unchanged() {
    // The plan reflects the server as it is, not as it was: convergence is computed in the
    // dry run too, so an editor can see in advance that a re-publish would do nothing.
    let item = support::item("markers").expect("the markers fixture imports");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "a-password");
    let mut transport = RecordingTransport::new();

    publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes");

    transport.forget_log();
    let plan = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::DryRun,
        &EmbeddedBytes,
    )
    .expect("plans");

    assert!(plan.dry_run);
    assert!(plan.uploaded.is_empty());
    assert_eq!(plan.unchanged.len(), 5);
    assert!(!transport.mutated());
}
