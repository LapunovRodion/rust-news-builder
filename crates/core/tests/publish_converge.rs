//! Convergence and the no-deletion rule (T050, FR-030, deviation D-7, SC-009).
//!
//! Both properties live in `core::publish` rather than in the SFTP adapter, which is what lets
//! them be asserted against a recording fake with no server in sight. The evidence is
//! [`RecordingTransport`]'s ordered log: a re-publish that writes nothing writes nothing
//! observably, not just in principle.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::model::item::{Block, NewsItem};
use newsbuilder_core::model::photo::PhotoId;
use newsbuilder_core::publish::{PublishMode, paths, publish};
use pretty_assertions::assert_eq;
use support::fakes::{FakeSecretStore, RecordingTransport, server_config};

const CASE: &str = "markers";
const SERVER: &str = "editorial";

/// The `markers` case: five photos, every one of them placed, so a publish has real work to do.
fn item() -> NewsItem {
    support::item(CASE).expect("the markers fixture imports")
}

fn secrets(server: &newsbuilder_core::model::server::ServerConfig) -> FakeSecretStore {
    FakeSecretStore::new().with_secret(&server.credential, "correct-horse-battery-staple")
}

#[test]
fn a_first_publish_uploads_every_placed_photo() {
    let item = item();
    let server = server_config(SERVER);
    let secrets = secrets(&server);
    let mut transport = RecordingTransport::new();

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes");

    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    assert_eq!(publication.remote_folder, folder);
    assert_eq!(publication.uploaded.len(), 5, "{:?}", publication.uploaded);
    assert!(publication.unchanged.is_empty());
    assert!(!publication.dry_run);
    assert_eq!(
        transport.names_in(&folder),
        vec![
            "marker-coverage-01.jpg",
            "marker-coverage-02.jpg",
            "marker-coverage-03.jpg",
            "marker-coverage-04.jpg",
            "marker-coverage-05.jpg",
        ]
    );
}

#[test]
fn republishing_an_unchanged_item_writes_nothing() {
    // SC-009: the second run is a no-op on the wire, not merely idempotent in its result.
    let item = item();
    let server = server_config(SERVER);
    let secrets = secrets(&server);
    let mut transport = RecordingTransport::new();

    publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("first publish");

    transport.forget_log();
    let second = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("second publish");

    assert_eq!(
        transport.put_count(),
        0,
        "the second run uploaded something: {:?}",
        transport.log()
    );
    assert!(
        !transport.mutated(),
        "the second run changed remote state: {:?}",
        transport.log()
    );
    assert!(second.uploaded.is_empty());
    assert_eq!(second.unchanged.len(), 5);
}

#[test]
fn a_photo_dropped_from_the_item_is_left_on_the_server() {
    // Deviation D-7: publishing never deletes. The fragment simply stops referencing the file.
    //
    // The *last* photo is the one removed, because published names carry the photo's position
    // in the item: dropping one from the middle renumbers everything after it, and the test
    // would then be measuring renaming rather than deletion.
    let mut item = item();
    let server = server_config(SERVER);
    let secrets = secrets(&server);
    let mut transport = RecordingTransport::new();

    publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("first publish");

    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    let orphan = "marker-coverage-05.jpg";
    let before = transport
        .file(&folder, orphan)
        .expect("the fifth photo was uploaded")
        .to_vec();

    let dropped = PhotoId(5);
    item.photos.retain(|photo| photo.id != dropped);
    item.body.retain(|block| !references_only(block, dropped));

    transport.forget_log();
    let after_drop = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes without the dropped photo");

    assert_eq!(
        transport.put_count(),
        0,
        "dropping a photo re-uploaded the others: {:?}",
        transport.log()
    );
    assert!(
        transport.names_in(&folder).iter().any(|n| n == orphan),
        "the dropped photo's file was removed from the server: {:?}",
        transport.names_in(&folder)
    );
    assert_eq!(
        transport.file(&folder, orphan),
        Some(before.as_slice()),
        "the orphaned file was rewritten"
    );
    // The file survives on the server, but the item no longer claims it: the orphan appears in
    // neither half of the publication, so nothing the CMS receives points at it.
    assert_eq!(after_drop.unchanged.len(), 4);
    assert!(
        !after_drop.unchanged.iter().any(|name| name == orphan)
            && !after_drop.uploaded.iter().any(|(name, _)| name == orphan),
        "the publication still claims the dropped photo: {after_drop:?}"
    );
}

/// Whether a block is a placement that names this photo and nothing else.
fn references_only(block: &Block, id: PhotoId) -> bool {
    matches!(block, Block::Placement { photos, .. } if photos.as_slice() == [id])
}
