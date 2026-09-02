//! Publishing refuses early, before a single photo is decoded (T052, FR-029, INV-8).
//!
//! "Early" is the point of these tests, not merely "eventually". Processing a thirty-photo item
//! and *then* discovering there is no password is the failure mode the ordering in
//! `core::publish` exists to prevent, and the recording fake is what makes the ordering visible:
//! a refusal that happened before the connection leaves an empty log.
//!
//! The third case in T052 — a slug colliding with another item's folder — is **not** a refusal.
//! Deviation D-8 settled that afterwards: the folder is suffixed and the other item is left
//! alone, which is what the last test here asserts.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::error::{Error, Warning};
use newsbuilder_core::publish::{PublishMode, paths, publish};
use pretty_assertions::assert_eq;
use support::fakes::{FakeSecretStore, RecordingTransport, TransportOp, server_config};

fn item() -> newsbuilder_core::model::item::NewsItem {
    support::item("markers").expect("the markers fixture imports")
}

#[test]
fn no_stored_credential_is_refused_before_the_connection() {
    let item = item();
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new(); // reachable, and holding nothing
    let mut transport = RecordingTransport::new();

    let error = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect_err("must refuse");

    assert!(
        matches!(&error, Error::NoCredential { server } if server == "editorial"),
        "{error:?}"
    );
    assert!(
        transport.log().is_empty(),
        "the transport was reached before the credential was resolved: {:?}",
        transport.log()
    );
    assert!(!transport.connected());
}

#[test]
fn an_empty_stored_credential_counts_as_none() {
    // An empty string in the secret store is a store that was written to badly, not a password.
    let item = item();
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "");
    let mut transport = RecordingTransport::new();

    let error = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect_err("must refuse");

    assert!(matches!(error, Error::NoCredential { .. }), "{error:?}");
    assert!(transport.log().is_empty());
}

#[test]
fn an_unreachable_secret_store_is_refused_before_the_connection() {
    // FR-041: the caller is sent to per-session entry, never to a password on disk.
    let item = item();
    let server = server_config("editorial");
    let secrets = FakeSecretStore::unavailable();
    let mut transport = RecordingTransport::new();

    let error = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect_err("must refuse");

    assert!(
        matches!(error, Error::SecretStoreUnavailable { .. }),
        "{error:?}"
    );
    assert!(transport.log().is_empty());
}

#[test]
fn an_unusable_remote_base_path_is_refused_before_anything_is_uploaded() {
    let item = item();
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "a-password");
    let mut transport =
        RecordingTransport::new().refusing_to_list(server.remote_base_path.as_str());

    let error = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect_err("must refuse");

    match &error {
        Error::RemotePathUnusable { path, .. } => assert_eq!(path, &server.remote_base_path),
        other => panic!("expected an unusable remote path, got {other:?}"),
    }
    assert!(
        !transport.mutated(),
        "the server was changed despite the refusal: {:?}",
        transport.log()
    );
    // The base path is checked immediately after connecting and before the folder is resolved.
    assert_eq!(
        transport.log().len(),
        2,
        "the refusal came too late: {:?}",
        transport.log()
    );
    assert!(matches!(transport.log()[1], TransportOp::List { .. }));
}

#[test]
fn a_folder_holding_a_different_item_is_suffixed_rather_than_written_into() {
    // Deviation D-8. "A different item" is decided by evidence: a folder full of files, none of
    // which carry our title's stem, belongs to someone else.
    let item = item();
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "a-password");

    let taken = paths::remote_folder(&server.remote_base_path, &item.slug);
    let squatter = b"another item's photo".to_vec();
    let mut transport = RecordingTransport::new().with_existing(
        taken.as_str(),
        "other-item-01.jpg",
        squatter.clone(),
    );

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes into a suffixed folder");

    assert_eq!(publication.remote_folder, format!("{taken}-2"));
    assert!(
        publication.warnings.iter().any(|w| matches!(
            w,
            Warning::SlugSuffixed { from, to }
                if from == "marker-coverage" && to == "marker-coverage-2"
        )),
        "no suffixing warning: {:?}",
        publication.warnings
    );

    // The other item is untouched: nothing added to its folder, nothing overwritten.
    assert_eq!(transport.names_in(&taken), vec!["other-item-01.jpg"]);
    assert_eq!(
        transport.file(&taken, "other-item-01.jpg"),
        Some(squatter.as_slice())
    );
    assert!(
        transport.puts().iter().all(|path| path.contains("-2/")),
        "something was written outside the suffixed folder: {:?}",
        transport.puts()
    );
}
