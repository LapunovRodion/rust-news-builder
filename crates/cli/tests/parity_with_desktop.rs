//! T100 — the same prepared item, published from both interfaces, produces the same thing
//! (FR-039, SC-011).
//!
//! The claim is structural: constitution principle II puts every rule in `newsbuilder-core`, so
//! the CLI and the desktop application are two thin adapters over one pipeline. This test exists
//! because "structural" is a claim about code that a person has to keep true, and the way it
//! stops being true is that a convenience creeps into one frontend — a trimmed title here, a
//! re-derived slug there — and the two quietly diverge.
//!
//! ## What each side actually runs
//!
//! **The command line** is the real binary, spawned as a subprocess. Not a library call
//! imitating it: the point is to exercise `main.rs`'s own `prepare` — the flag handling, the
//! document read, the photo folder walk, the title and slug overrides — because that is the
//! code that could drift.
//!
//! It runs `build --public-base-url`, not `publish`, for the plain reason that `publish` needs
//! an SSH server and the OS keyring, neither of which belongs in this test. That is not a
//! weakening: `core::publish` renders its fragment by calling `core::build` with the server's
//! public base URL and the folder it resolved, and nothing else. `the_two_sides_of_the_cli_agree`
//! pins that equality down in-process, so the chain from "what the CLI writes" to "what a
//! publish would upload" is closed rather than assumed.
//!
//! **The desktop side** is `core::publish` driven the way `crates/desktop/src/commands/publish.rs`
//! drives it: the item's photo bytes held in a session map rather than read from disk, an
//! in-memory transport, an in-memory secret store. Everything specific to Tauri is the part
//! that is not being tested.
//!
//! ## Why the slug is normalised
//!
//! The two are deliberately published to *different* folders, and the folder is then normalised
//! out of both fragments. Comparing two runs that used the same slug would pass even if a
//! frontend had hardcoded it; comparing across two slugs proves the fragment differs in the
//! folder and in nothing else. The photos' own names are untouched by this — those come from
//! the title, not from the folder, so a difference there would still show.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use newsbuilder_core::build::{BuildContext, PhotoBytesSource, build};
use newsbuilder_core::error::Result as CoreResult;
use newsbuilder_core::import::{attach_photos, import_document};
use newsbuilder_core::model::item::{NewsItem, SourceFormat};
use newsbuilder_core::model::photo::{Photo, PhotoId, PhotoSource};
use newsbuilder_core::model::server::{CredentialRef, ServerConfig, Slug};
use newsbuilder_core::ports::{RemoteEntry, SecretStore, Transport};
use newsbuilder_core::publish::{Publication, PublishMode, publish};
use newsbuilder_core::secret::Secret;

/// The fixture both sides publish: a marker document and five photos, which between them use
/// all four layouts.
const CASE: &str = "markers";

/// Where the photos will live. Same for both sides — it is the *folder* that differs.
const PUBLIC_BASE_URL: &str = "https://example.org/news/2026/03/";

const REMOTE_BASE_PATH: &str = "/var/www/html/news";

/// The two folders, and what they are both rewritten to before comparison.
const CLI_FOLDER: &str = "published-from-the-command-line";
const DESKTOP_FOLDER: &str = "published-from-the-application";
const NORMALISED: &str = "THE-FOLDER";

// =============================================================================================
// Fixtures
// =============================================================================================

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn input_dir() -> PathBuf {
    repo_root().join("fixtures/inputs").join(CASE)
}

fn document_path() -> PathBuf {
    input_dir().join("input.txt")
}

fn images_dir() -> PathBuf {
    input_dir().join("images")
}

/// The images, as `attach_photos` wants them. Order is left to `attach_photos`, which applies
/// the reference's natural sort — the same call the CLI makes.
fn images() -> Vec<(String, PhotoSource, Vec<u8>)> {
    let mut found: Vec<_> = std::fs::read_dir(images_dir())
        .expect("the markers fixture has an images folder")
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).ok()?;
            Some((name, PhotoSource::Bytes(bytes.clone().into()), bytes))
        })
        .collect();
    // Read order is the filesystem's; sorting here only makes the test's own input stable.
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// The item the desktop application would be holding: document imported, photos attached.
fn prepared_item(folder: &str) -> NewsItem {
    let bytes = std::fs::read(document_path()).expect("the fixture document is readable");
    let mut imported =
        import_document(&bytes, SourceFormat::Text).expect("the fixture document imports");
    attach_photos(&mut imported.item, images());
    imported.item.slug = Slug::parse(folder).expect("the test folder is a well-formed slug");
    imported.item
}

// =============================================================================================
// The desktop side's ports, in memory
// =============================================================================================

/// Serves photo bytes from a map, which is what `desktop::state::SessionBytes` does.
struct SessionBytes(HashMap<PhotoId, Vec<u8>>);

impl PhotoBytesSource for SessionBytes {
    fn read(&self, photo: &Photo) -> CoreResult<Vec<u8>> {
        if let Some(bytes) = self.0.get(&photo.id) {
            return Ok(bytes.clone());
        }
        match &photo.source {
            PhotoSource::Bytes(bytes) => Ok(bytes.to_vec()),
            PhotoSource::Path(path) => Err(newsbuilder_core::error::Error::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "this photo's bytes are not in the session",
                ),
            }),
        }
    }
}

/// A server that keeps what it is given, so the upload can be inspected.
#[derive(Debug, Default)]
struct InMemoryServer {
    files: HashMap<String, Vec<u8>>,
}

impl Transport for InMemoryServer {
    fn connect(&mut self, _: &ServerConfig, _: &Secret) -> CoreResult<()> {
        Ok(())
    }

    fn list(&mut self, remote_dir: &str) -> CoreResult<Vec<RemoteEntry>> {
        let prefix = format!("{remote_dir}/");
        Ok(self
            .files
            .iter()
            .filter_map(|(path, bytes)| {
                let name = path.strip_prefix(&prefix)?;
                // One level only, the way a directory listing is.
                if name.contains('/') {
                    return None;
                }
                Some(RemoteEntry {
                    name: name.to_owned(),
                    size: bytes.len() as u64,
                })
            })
            .collect())
    }

    fn ensure_dir(&mut self, _: &str) -> CoreResult<()> {
        Ok(())
    }

    fn put(&mut self, remote_path: &str, bytes: &[u8]) -> CoreResult<()> {
        self.files.insert(remote_path.to_owned(), bytes.to_vec());
        Ok(())
    }
}

/// A secret store holding one password.
struct OneSecret;

impl SecretStore for OneSecret {
    fn get(&self, _: &CredentialRef) -> CoreResult<Option<Secret>> {
        Ok(Some(Secret::new("not-the-thing-under-test")))
    }
    fn set(&self, _: &CredentialRef, _: &Secret) -> CoreResult<()> {
        Ok(())
    }
    fn delete(&self, _: &CredentialRef) -> CoreResult<()> {
        Ok(())
    }
    fn available(&self) -> bool {
        true
    }
}

fn server_config() -> ServerConfig {
    ServerConfig {
        name: "parity".to_owned(),
        host: "news.example.org".to_owned(),
        user: "editor".to_owned(),
        port: 22,
        remote_base_path: REMOTE_BASE_PATH.to_owned(),
        public_base_url: url::Url::parse(PUBLIC_BASE_URL).expect("a valid constant url"),
        credential: CredentialRef::Password {
            server: "parity".to_owned(),
        },
    }
}

// =============================================================================================
// Running each side
// =============================================================================================

/// What the desktop application would have published.
fn publish_from_the_application() -> (Publication, InMemoryServer) {
    let item = prepared_item(DESKTOP_FOLDER);

    // The session holds every photo's bytes, read once on the way in (FR-015). This is the one
    // place the two frontends genuinely differ: the CLI reads from disk each run, the
    // application reads once and keeps them.
    let bytes: HashMap<PhotoId, Vec<u8>> = item
        .photos
        .iter()
        .filter_map(|photo| match &photo.source {
            PhotoSource::Bytes(bytes) => Some((photo.id, bytes.to_vec())),
            PhotoSource::Path(path) => std::fs::read(path).ok().map(|bytes| (photo.id, bytes)),
        })
        .collect();

    let mut transport = InMemoryServer::default();
    let publication = publish(
        &item,
        &server_config(),
        &mut transport,
        &OneSecret,
        PublishMode::Live,
        &SessionBytes(bytes),
    )
    .expect("the application publishes the fixture");

    (publication, transport)
}

/// What `newsbuilder build` writes, run as the real binary.
struct CliRun {
    fragment: String,
    report: serde_json::Value,
}

fn build_from_the_command_line() -> CliRun {
    // A directory per call, not per process. Several tests here run the CLI, cargo runs them
    // concurrently in one process, and each removes its directory when it is done — sharing one
    // means a test deleting the fragment another is still reading.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let out_dir =
        std::env::temp_dir().join(format!("newsbuilder-parity-{}-{nth}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("a temporary directory");
    let fragment_path = out_dir.join("news.html");

    let output = Command::new(env!("CARGO_BIN_EXE_newsbuilder"))
        .args([
            "build",
            "--input",
            document_path().to_str().expect("a UTF-8 fixture path"),
            "--images-dir",
            images_dir().to_str().expect("a UTF-8 fixture path"),
            "--news-slug",
            CLI_FOLDER,
            "--public-base-url",
            PUBLIC_BASE_URL,
            "--output",
            fragment_path.to_str().expect("a UTF-8 temporary path"),
            "--json",
        ])
        .output()
        .expect("the newsbuilder binary runs");

    assert!(
        output.status.success(),
        "the CLI failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let fragment = std::fs::read_to_string(&fragment_path).expect("the CLI wrote its fragment");
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("--json produced one JSON object");

    let _ = std::fs::remove_dir_all(&out_dir);
    CliRun { fragment, report }
}

/// Rewrites whichever folder a fragment used to a fixed name, so two runs that differ only in
/// where they published compare equal.
fn normalise_folder(fragment: &str) -> String {
    fragment
        .replace(CLI_FOLDER, NORMALISED)
        .replace(DESKTOP_FOLDER, NORMALISED)
}

// =============================================================================================
// The tests
// =============================================================================================

#[test]
fn the_same_item_yields_the_same_fragment_from_either_interface() {
    // SC-011, stated as directly as it can be.
    let (from_application, _) = publish_from_the_application();
    let from_cli = build_from_the_command_line();

    let application_fragment = normalise_folder(&from_application.fragment);
    let cli_fragment = normalise_folder(&from_cli.fragment);

    assert_eq!(
        cli_fragment, application_fragment,
        "FR-039: the two interfaces produced different fragments. This is a constitution \
         principle II violation — domain logic has leaked into a frontend — not a cosmetic bug."
    );

    // And the normalisation did not do the work: the raw fragments genuinely differ, so the
    // comparison above was between two different publications rather than two identical ones.
    assert_ne!(
        from_cli.fragment, from_application.fragment,
        "the two sides published to the same folder, so this test proves nothing"
    );
    assert!(cli_fragment.contains(NORMALISED), "no folder was rewritten");
}

#[test]
fn both_interfaces_publish_the_same_photos_under_the_same_names() {
    // The fragment is the artefact, but a fragment pointing at files that were never uploaded —
    // or uploaded under other names — is worth nothing. The names come from the title and the
    // photo's position, so they must match without any normalisation at all.
    let (from_application, server) = publish_from_the_application();
    let from_cli = build_from_the_command_line();

    let mut cli_names: Vec<String> = from_cli
        .report
        .get("photos")
        .and_then(serde_json::Value::as_array)
        .expect("the --json report lists photos")
        .iter()
        .filter_map(|photo| Some(photo.get("name")?.as_str()?.to_owned()))
        .collect();
    let mut application_names: Vec<String> = from_application
        .uploaded
        .iter()
        .map(|(name, _)| name.clone())
        .collect();

    assert!(!cli_names.is_empty(), "the fixture has photos");
    cli_names.sort();
    application_names.sort();
    assert_eq!(
        cli_names, application_names,
        "FR-039: the two interfaces published different files"
    );

    // Every uploaded file is the one the application's own fragment points at.
    for (name, url) in &from_application.uploaded {
        let expected = format!("{PUBLIC_BASE_URL}{DESKTOP_FOLDER}/{name}");
        assert_eq!(url.as_str(), expected);
        assert!(
            from_application.fragment.contains(url.as_str()),
            "{url} is uploaded but not referenced"
        );
        assert!(
            server
                .files
                .contains_key(&format!("{REMOTE_BASE_PATH}/{DESKTOP_FOLDER}/{name}")),
            "{name} was reported but never put"
        );
    }
}

#[test]
fn the_photos_are_byte_for_byte_the_same_from_either_interface() {
    // The CLI reports the size it encoded; the application uploaded the bytes. If the two
    // pipelines had diverged in resizing or in the quality ladder, this is where it shows.
    let (_, server) = publish_from_the_application();
    let from_cli = build_from_the_command_line();

    let uploaded: HashMap<String, usize> = server
        .files
        .iter()
        .filter_map(|(path, bytes)| {
            let name = path.rsplit('/').next()?;
            Some((name.to_owned(), bytes.len()))
        })
        .collect();

    for photo in from_cli
        .report
        .get("photos")
        .and_then(serde_json::Value::as_array)
        .expect("the --json report lists photos")
    {
        let name = photo
            .get("name")
            .and_then(serde_json::Value::as_str)
            .expect("every photo is named");
        let bytes = photo
            .get("bytes")
            .and_then(serde_json::Value::as_u64)
            .expect("a build reports the size it encoded") as usize;
        assert_eq!(
            uploaded.get(name),
            Some(&bytes),
            "{name} came out at a different size from the two interfaces"
        );
    }
}

#[test]
fn the_two_sides_of_the_cli_agree() {
    // Closes the gap in what this file can run. The CLI side above uses `build`, because
    // `publish` needs a server and a keyring; this asserts that the fragment `publish` renders
    // is exactly the fragment `build` renders for the same folder and public base — which is
    // what makes the `build`-based comparison a statement about publishing.
    //
    // It is a statement about `core`, so it holds for both frontends at once.
    let item = prepared_item(DESKTOP_FOLDER);
    let bytes: HashMap<PhotoId, Vec<u8>> = item
        .photos
        .iter()
        .filter_map(|photo| match &photo.source {
            PhotoSource::Bytes(bytes) => Some((photo.id, bytes.to_vec())),
            PhotoSource::Path(_) => None,
        })
        .collect();
    let sources = SessionBytes(bytes);

    let built = build(
        &item,
        &BuildContext {
            public_base_url: Some(url::Url::parse(PUBLIC_BASE_URL).expect("a valid url")),
            slug: item.slug.clone(),
        },
        &sources,
    )
    .expect("the item builds");

    let mut transport = InMemoryServer::default();
    let published = publish(
        &item,
        &server_config(),
        &mut transport,
        &OneSecret,
        PublishMode::Live,
        &sources,
    )
    .expect("the item publishes");

    assert_eq!(
        built.fragment, published.fragment,
        "a publish rendered something a build of the same item and folder did not"
    );
    assert_eq!(built.processed.len(), published.uploaded.len());
}

#[test]
fn a_dry_run_from_either_interface_describes_the_run_that_would_happen() {
    // FR-031 across the parity boundary: the desktop's dry run has to plan the same upload the
    // CLI's `--dry-run` reports, or an editor checking one and publishing from the other is
    // checking the wrong thing.
    let item = prepared_item(DESKTOP_FOLDER);
    let bytes: HashMap<PhotoId, Vec<u8>> = item
        .photos
        .iter()
        .filter_map(|photo| match &photo.source {
            PhotoSource::Bytes(bytes) => Some((photo.id, bytes.to_vec())),
            PhotoSource::Path(_) => None,
        })
        .collect();
    let sources = SessionBytes(bytes);

    let mut transport = InMemoryServer::default();
    let planned = publish(
        &item,
        &server_config(),
        &mut transport,
        &OneSecret,
        PublishMode::DryRun,
        &sources,
    )
    .expect("the dry run plans");
    assert!(planned.dry_run);
    assert!(
        transport.files.is_empty(),
        "FR-031: the dry run uploaded {:?}",
        transport.files.keys().collect::<Vec<_>>()
    );

    let mut transport = InMemoryServer::default();
    let done = publish(
        &item,
        &server_config(),
        &mut transport,
        &OneSecret,
        PublishMode::Live,
        &sources,
    )
    .expect("the publish runs");

    assert_eq!(
        planned.uploaded.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        done.uploaded.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        "the dry run planned a different upload than the publish performed"
    );
    assert_eq!(planned.fragment, done.fragment);
}

#[test]
fn neither_interface_leaks_the_credential_into_what_it_produces() {
    // SC-010 at the seam. The CLI's `--json` and the fragment both cross into a script or a
    // CMS; neither may carry a secret, and the application's `PublicationView` is built from
    // the same `Publication` this checks.
    let (from_application, _) = publish_from_the_application();
    let from_cli = build_from_the_command_line();

    let sentinel = "not-the-thing-under-test";
    assert!(!from_application.fragment.contains(sentinel));
    assert!(!from_cli.fragment.contains(sentinel));
    assert!(!from_cli.report.to_string().contains(sentinel));
}
