//! A problem in one photo does not cost the editor the item (T054, FR-034, SC-007).
//!
//! Two fixtures captured from the reference cover the two shapes of the problem: a marker that
//! names a photo the item does not hold, and a photo no marker names. Both build, both publish,
//! and both say which photo or marker was at fault.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::error::Warning;
use newsbuilder_core::publish::{PublishMode, paths, publish};
use pretty_assertions::assert_eq;
use support::fakes::{FakeSecretStore, RecordingTransport, server_config};

fn publish_case(
    case: &str,
) -> (
    newsbuilder_core::publish::Publication,
    RecordingTransport,
    String,
) {
    let item = support::item(case).expect("the fixture imports");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, "a-password");
    let mut transport = RecordingTransport::new();

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("a warning must not stop the publish");

    let folder = paths::remote_folder(&server.remote_base_path, &item.slug);
    (publication, transport, folder)
}

#[test]
fn a_marker_naming_a_photo_that_is_not_there_warns_and_publishes_the_rest() {
    // fixtures/inputs/missing-photo: `[image:1]` and `[image:7]`, with one photo present.
    let (publication, transport, folder) = publish_case("missing-photo");

    assert!(
        publication.warnings.iter().any(|w| matches!(
            w,
            Warning::MissingPhoto { marker } if marker.contains('7')
        )),
        "the missing photo was not named: {:?}",
        publication.warnings
    );
    assert_eq!(
        transport.names_in(&folder),
        vec!["missing-photo-01.jpg"],
        "the photo that does exist must still be published"
    );
    assert_eq!(publication.uploaded.len(), 1);
}

#[test]
fn photos_no_placement_references_are_warned_about_and_not_uploaded() {
    // fixtures/inputs/unused-photo: three photos, one marker.
    let (publication, transport, folder) = publish_case("unused-photo");

    let unused: Vec<u64> = publication
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::UnusedPhoto { id } => Some(id.0),
            _ => None,
        })
        .collect();
    assert_eq!(unused, vec![2, 3], "{:?}", publication.warnings);

    assert_eq!(
        transport.names_in(&folder),
        vec!["unused-photo-01.jpg"],
        "an unused photo was uploaded anyway"
    );
    assert_eq!(publication.uploaded.len(), 1);
    assert_eq!(transport.put_count(), 1);
}

#[test]
fn an_unused_photo_still_consumes_its_number_when_published() {
    // Parity: the published number is the photo's position in the item, not among the used
    // ones, so removing a marker cannot renumber — and therefore cannot re-upload — the rest.
    let (publication, _, _) = publish_case("unused-photo");
    let (name, url) = &publication.uploaded[0];
    assert_eq!(name, "unused-photo-01.jpg");
    assert_eq!(
        url.as_str(),
        "https://example.org/news/2026/03/unused-photo/unused-photo-01.jpg"
    );
}
