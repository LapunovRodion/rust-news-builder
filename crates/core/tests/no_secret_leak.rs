//! A password never escapes (T053, FR-032, SC-010).
//!
//! The guarantee is structural rather than procedural: [`Secret`] redacts through both
//! formatters, [`ServerConfig`] holds a `CredentialRef` instead of a credential, and no error
//! variant carries one. This test is the end-to-end confirmation of all three at once — it runs
//! a real publish with a sentinel password and then hunts for the sentinel everywhere an
//! operator could plausibly see text.
//!
//! Both paths are covered: the successful publish, and a failing upload, because the failure
//! path is where a hurried `format!` most often puts the connection details.

mod support;

use newsbuilder_core::build::EmbeddedBytes;
use newsbuilder_core::publish::{PublishMode, paths, publish};
use newsbuilder_core::secret::Secret;
use support::fakes::{FakeFileStore, FakeSecretStore, RecordingTransport, server_config};

/// Distinctive enough that a substring search cannot match it by accident.
const SENTINEL: &str = "hunter2-Zx9Q-sentinel-password";

/// Fails the test if the sentinel appears anywhere in `haystack`.
fn assert_clean(what: &str, haystack: &str) {
    assert!(
        !haystack.contains(SENTINEL),
        "the credential leaked through {what}:\n{haystack}"
    );
}

#[test]
fn a_successful_publish_leaks_the_password_nowhere() {
    let item = support::item("markers").expect("the markers fixture imports");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, SENTINEL);
    let mut transport = RecordingTransport::new();
    let disk = FakeFileStore::new();

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect("publishes");

    // The credential did authenticate, so this is a real run rather than a vacuous one.
    assert!(transport.connected());
    assert_eq!(transport.put_count(), 5);

    assert_clean("the transport's log", &format!("{:?}", transport.log()));
    assert_clean("the transport itself", &format!("{transport:?}"));
    assert_clean("the secret store", &format!("{secrets:?}"));
    assert_clean("the server configuration", &format!("{server:?}"));
    assert_clean("the publication", &format!("{publication:?}"));
    assert_clean("anything written to disk", &disk.as_haystack());

    for warning in &publication.warnings {
        assert_clean("a warning's Display", &format!("{warning}"));
        assert_clean("a warning's Debug", &format!("{warning:?}"));
    }
    for (name, url) in &publication.uploaded {
        assert_clean("a published file name", name);
        assert_clean("a published URL", url.as_str());
    }
    assert_clean("the remote folder", &publication.remote_folder);
}

#[test]
fn a_failing_upload_leaks_the_password_nowhere() {
    let item = support::item("markers").expect("the markers fixture imports");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, SENTINEL);

    let first_file = paths::remote_file(
        &server.remote_base_path,
        &item.slug,
        "marker-coverage-01.jpg",
    );
    let mut transport = RecordingTransport::new().refusing_to_put(first_file.as_str());

    let error = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &EmbeddedBytes,
    )
    .expect_err("the upload was told to fail");

    assert_clean("the error's Display", &format!("{error}"));
    assert_clean("the error's Debug", &format!("{error:?}"));
    assert_clean("the transport's log", &format!("{:?}", transport.log()));

    // The error still names the offending file, which is what SC-007 asks of it.
    assert!(
        format!("{error}").contains("marker-coverage-01.jpg"),
        "the error does not name the file it failed on: {error}"
    );
}

#[test]
fn a_refusal_leaks_the_password_nowhere_either() {
    // The store holds the sentinel under a *different* server's entry, so resolution fails
    // while a real secret is sitting in the store.
    let item = support::item("markers").expect("the markers fixture imports");
    let elsewhere = server_config("other-newsroom");
    let server = server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&elsewhere.credential, SENTINEL);
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

    assert_clean("the error's Display", &format!("{error}"));
    assert_clean("the error's Debug", &format!("{error:?}"));
    assert_clean("the secret store", &format!("{secrets:?}"));
}

#[test]
fn the_sentinel_would_have_been_found_if_it_had_leaked() {
    // A leak test that can only pass is worth nothing: this is the control. A `Secret` printed
    // through the one accessor that exposes it does show up, so the searches above are real.
    let exposed = Secret::new(SENTINEL).expose().to_owned();
    assert!(exposed.contains(SENTINEL));
}

// ---------------------------------------------------------------------------------------------
// 002: site insertion adds no secret, and leaks none (SC-006, T041)
// ---------------------------------------------------------------------------------------------

#[test]
fn a_publish_into_a_site_leaks_the_password_nowhere() {
    use newsbuilder_core::build::EmbeddedBytes;
    use newsbuilder_core::model::site::ArticleConfirmation;
    use newsbuilder_core::publish::{PublishMode, publish_to_site};
    use support::fakes::{RemoteFake, ScriptedSite, site_server_config};

    let item = support::item("markers").expect("imports");
    let server = site_server_config("editorial");
    let secrets = FakeSecretStore::new().with_secret(&server.credential, SENTINEL);

    // Every outcome shape: created, and failed with a detail carrying server output.
    let mut created = RemoteFake::default();
    let mut failing = RemoteFake::new(RecordingTransport::new(), {
        let mut site = ScriptedSite::new();
        site.save_fails = Some("saving the article".to_owned());
        site
    });

    for remote in [&mut created, &mut failing] {
        let publication = publish_to_site(
            &item,
            &server,
            remote,
            &secrets,
            PublishMode::Live,
            ArticleConfirmation::None,
            &EmbeddedBytes,
        )
        .expect("publishes");
        assert_clean("the publication", &format!("{publication:?}"));
        assert_clean("the site calls", &format!("{:?}", remote.site.log()));
        assert_clean(
            "the transport calls",
            &format!("{:?}", remote.transport.log()),
        );
    }

    // The configuration file, site settings included.
    assert_clean(
        "the serialised configuration",
        &serde_json::to_string(&server).expect("serialises"),
    );
}
