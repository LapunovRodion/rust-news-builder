//! T099 — one publish, end to end, against a real `sshd` (research R10).
//!
//! Every other publish test runs against `RecordingTransport`, and deliberately: convergence
//! (FR-030) and dry-run (FR-031) are properties of the orchestration, so a fake proves them
//! precisely and in milliseconds. What a fake cannot prove is the thin layer underneath it —
//! that `russh` really authenticates, that `russh-sftp` really creates the directories, that
//! the paths this crate builds are the paths a server accepts, and that an unknown host key is
//! really refused (deviation D-15).
//!
//! That is all this file is for, and it is why it stays opt-in:
//!
//! ```text
//! just e2e          # NEWSBUILDER_E2E_SSH=1 … --features sftp … -- --ignored
//! ```
//!
//! It needs `sshd` and `ssh-keygen` on `PATH` and a free loopback port. Without
//! `NEWSBUILDER_E2E_SSH` set it skips with a message rather than failing, so someone who runs
//! `--ignored` out of curiosity gets an explanation instead of a red test.
//!
//! **`sshd` runs as the invoking user**, on loopback, in a temporary directory, with a
//! throwaway host key and a throwaway user key. Nothing outside that directory is touched and
//! nothing listens beyond the end of the test.

#![cfg(feature = "sftp")]

mod support;

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use newsbuilder_core::adapters::transport::SftpTransport;
use newsbuilder_core::error::{Error, Result};
use newsbuilder_core::model::server::{CredentialRef, ServerConfig};
use newsbuilder_core::ports::SecretStore;
use newsbuilder_core::publish::{PublishMode, publish};
use newsbuilder_core::secret::Secret;

/// The switch. Absent or `0` means skip.
const SWITCH: &str = "NEWSBUILDER_E2E_SSH";

/// The key's passphrase.
///
/// Not empty, and that is not an accident: `publish` treats an empty stored secret as no
/// credential at all (`publish_refusals.rs`), so a passphrase-less key is not a case this
/// product supports. Using a real passphrase also puts `load_secret_key` on the tested path.
const PASSPHRASE: &str = "e2e-passphrase";

/// The fixture case published. Markers and five photos, which exercises all four layouts.
const CASE: &str = "markers";

fn enabled() -> bool {
    match std::env::var(SWITCH) {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => false,
    }
}

// =============================================================================================
// The harness
// =============================================================================================

/// A throwaway `sshd`, its keys, and the directory it publishes into.
struct Sshd {
    child: Child,
    dir: PathBuf,
    port: u16,
    user: String,
}

impl Sshd {
    /// Starts one, or explains why it could not.
    fn start() -> std::result::Result<Self, String> {
        let sshd = which("sshd")?;
        let keygen = which("ssh-keygen")?;

        // A directory of our own, named after the port so two runs cannot collide.
        let port = free_port()?;
        let dir = std::env::temp_dir().join(format!("newsbuilder-e2e-{port}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docroot")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir.join(".ssh")).map_err(|e| e.to_string())?;

        run(
            &keygen,
            &[
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-f",
                str_of(&dir.join("host_ed25519"))?,
            ],
        )?;
        run(
            &keygen,
            &[
                "-q",
                "-t",
                "ed25519",
                "-N",
                PASSPHRASE,
                "-f",
                str_of(&dir.join("user_ed25519"))?,
            ],
        )?;
        std::fs::copy(dir.join("user_ed25519.pub"), dir.join("authorized_keys"))
            .map_err(|e| e.to_string())?;
        chmod_600(&dir.join("authorized_keys"))?;
        chmod_600(&dir.join("user_ed25519"))?;

        // `StrictModes no` and no PAM: this is a rootless sshd serving one user in a temporary
        // directory, and the checks it would otherwise make are about a system installation.
        let config = format!(
            "Port {port}\n\
             ListenAddress 127.0.0.1\n\
             HostKey {dir}/host_ed25519\n\
             AuthorizedKeysFile {dir}/authorized_keys\n\
             PidFile {dir}/sshd.pid\n\
             StrictModes no\n\
             UsePAM no\n\
             PasswordAuthentication no\n\
             PubkeyAuthentication yes\n\
             Subsystem sftp internal-sftp\n",
            dir = dir.display(),
        );
        let config_path = dir.join("sshd_config");
        std::fs::write(&config_path, config).map_err(|e| e.to_string())?;

        // sshd insists on being invoked by absolute path.
        let child = Command::new(&sshd)
            .args(["-D", "-e", "-f", str_of(&config_path)?])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", sshd.display()))?;

        let harness = Self {
            child,
            dir,
            port,
            user: whoami()?,
        };
        harness.wait_until_listening()?;
        Ok(harness)
    }

    fn wait_until_listening(&self) -> std::result::Result<(), String> {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(format!(
            "sshd never accepted a connection on port {}",
            self.port
        ))
    }

    /// Where published folders appear. Absolute, because a remote base path is.
    fn docroot(&self) -> PathBuf {
        self.dir.join("docroot")
    }

    /// The home the adapter reads `~/.ssh/known_hosts` from during the test.
    fn home(&self) -> &Path {
        &self.dir
    }

    fn known_hosts(&self) -> PathBuf {
        self.dir.join(".ssh/known_hosts")
    }

    /// Writes the server's real host key into `known_hosts`, the way `ssh-keyscan` would.
    fn trust_the_host_key(&self) -> std::result::Result<(), String> {
        let keyscan = which("ssh-keyscan")?;
        let output = Command::new(keyscan)
            .args(["-p", &self.port.to_string(), "-t", "ed25519", "127.0.0.1"])
            .output()
            .map_err(|e| e.to_string())?;
        if output.stdout.is_empty() {
            return Err("ssh-keyscan returned no host key".to_owned());
        }
        std::fs::write(self.known_hosts(), output.stdout).map_err(|e| e.to_string())
    }

    /// An empty `known_hosts`, which is what a first connection to an unknown server looks like.
    fn forget_the_host_key(&self) -> std::result::Result<(), String> {
        std::fs::write(self.known_hosts(), b"").map_err(|e| e.to_string())
    }

    fn server_config(&self) -> ServerConfig {
        ServerConfig {
            name: "e2e".to_owned(),
            host: "127.0.0.1".to_owned(),
            user: self.user.clone(),
            port: self.port,
            remote_base_path: self.docroot().to_string_lossy().into_owned(),
            public_base_url: url::Url::parse(support::PUBLIC_BASE_URL)
                .expect("a valid constant url"),
            credential: CredentialRef::Key {
                path: self.dir.join("user_ed25519"),
                server: "e2e".to_owned(),
            },
        }
    }
}

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A secret store holding the one passphrase, so the test needs no OS keyring.
struct OnePassphrase;

impl SecretStore for OnePassphrase {
    fn get(&self, _: &CredentialRef) -> Result<Option<Secret>> {
        Ok(Some(Secret::new(PASSPHRASE)))
    }
    fn set(&self, _: &CredentialRef, _: &Secret) -> Result<()> {
        Ok(())
    }
    fn delete(&self, _: &CredentialRef) -> Result<()> {
        Ok(())
    }
    fn available(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------------
// Small shell helpers
// ---------------------------------------------------------------------------------------------

fn which(program: &str) -> std::result::Result<PathBuf, String> {
    let path = std::env::var_os("PATH").ok_or("PATH is not set")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("`{program}` is not on PATH; this test needs OpenSSH"))
}

fn str_of(path: &Path) -> std::result::Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", path.display()))
}

fn run(program: &Path, args: &[&str]) -> std::result::Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {e}", program.display()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{} {:?} failed: {}",
        program.display(),
        args,
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn chmod_600(path: &Path) -> std::result::Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}

fn whoami() -> std::result::Result<String, String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .map_err(|_| "neither USER nor LOGNAME is set".to_owned())
}

/// A loopback port nothing is listening on.
///
/// Racy in principle — the port is free when we look and could be taken before `sshd` binds it.
/// In practice it is the standard trick, and a lost race fails loudly rather than silently.
fn free_port() -> std::result::Result<u16, String> {
    let listener =
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| format!("no free port: {e}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|e| e.to_string())
}

/// Every file in a directory as `(name, bytes)`, sorted — the shape SC-009 compares.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Vec<u8>)> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            std::fs::read(entry.path()).ok().map(|bytes| (name, bytes))
        })
        .collect();
    out.sort();
    out
}

fn skip(reason: &str) {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "e2e_sshd: skipped — {reason}");
}

// =============================================================================================
// The test
// =============================================================================================

/// One `sshd`, five phases, in order.
///
/// Deliberately one test rather than five. Each phase needs the server in the state the phase
/// before it left, and `HOME` is redirected process-wide — two tests doing that at once would
/// fight. Running them as one sequence also means one `sshd` rather than five.
#[test]
#[ignore = "needs a local sshd; run with `just e2e`"]
fn a_real_publish_over_sftp_converges_and_checks_the_host_key() {
    if !enabled() {
        skip(&format!("{SWITCH} is not set. Run `just e2e`."));
        return;
    }

    let harness = match Sshd::start() {
        Ok(harness) => harness,
        Err(reason) => {
            skip(&reason);
            return;
        }
    };

    // The adapter checks `~/.ssh/known_hosts`, which means the *process's* home. Redirecting it
    // is what keeps this test away from the developer's real known_hosts — it neither reads a
    // trusted key from there nor writes a throwaway one into it.
    //
    // SAFETY: this test binary contains exactly one test, so nothing else in the process is
    // reading the environment concurrently.
    unsafe {
        std::env::set_var("HOME", harness.home());
    }

    let item = support::item(CASE).expect("the markers fixture is captured");
    let server = harness.server_config();
    let secrets = OnePassphrase;
    let folder = harness.docroot().join(item.slug.as_str());

    // ---------------------------------------------------------------------------------------
    // 1. An unknown host key is refused, and the message says how to make it known (D-15).
    // ---------------------------------------------------------------------------------------
    harness
        .forget_the_host_key()
        .expect("known_hosts can be written");
    let mut transport = SftpTransport::new().expect("a transport starts");
    let refused = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &newsbuilder_core::build::EmbeddedBytes,
    )
    .expect_err("an unknown host key must not be accepted");
    let message = refused.to_string();
    assert!(
        message.contains("known_hosts"),
        "the refusal does not explain itself (SC-007): {message}"
    );
    assert!(
        !folder.exists(),
        "a refused connection still created {}",
        folder.display()
    );

    // ---------------------------------------------------------------------------------------
    // 2. With the key known, a dry run plans everything and writes nothing (FR-031).
    // ---------------------------------------------------------------------------------------
    harness
        .trust_the_host_key()
        .expect("ssh-keyscan can reach the harness");
    let mut transport = SftpTransport::new().expect("a transport starts");
    let planned = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::DryRun,
        &newsbuilder_core::build::EmbeddedBytes,
    )
    .expect("the dry run reaches the server");
    assert!(planned.dry_run);
    assert!(
        !planned.uploaded.is_empty(),
        "the fixture has photos to publish"
    );
    assert!(
        !folder.exists(),
        "FR-031: the dry run created {}",
        folder.display()
    );

    // ---------------------------------------------------------------------------------------
    // 3. The real publish puts the photos where the fragment says they are.
    // ---------------------------------------------------------------------------------------
    let mut transport = SftpTransport::new().expect("a transport starts");
    let published = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &newsbuilder_core::build::EmbeddedBytes,
    )
    .expect("the publish succeeds");

    assert!(!published.dry_run);
    assert!(
        published.unchanged.is_empty(),
        "a first publish has nothing to leave alone"
    );
    assert_eq!(
        published.uploaded.len(),
        planned.uploaded.len(),
        "the dry run planned a different set than the publish uploaded"
    );

    let on_disk = snapshot(&folder);
    assert_eq!(
        on_disk.len(),
        published.uploaded.len(),
        "{} holds {:?}",
        folder.display(),
        on_disk.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    for (name, url) in &published.uploaded {
        let landed = folder.join(name);
        assert!(
            landed.is_file(),
            "{name} was reported uploaded but is not at {}",
            landed.display()
        );
        // The URL the CMS will use has to end in the file the server actually holds; a path
        // built one way and uploaded another is exactly the class of bug a fake cannot catch.
        assert!(
            url.as_str()
                .ends_with(&format!("{}/{name}", published.folder)),
            "{url} does not point at {name}"
        );
        // And the fragment points at that same URL.
        assert!(
            published.fragment.contains(url.as_str()),
            "the fragment does not reference {url}"
        );
    }

    // ---------------------------------------------------------------------------------------
    // 4. Publishing again changes nothing at all (FR-030, SC-009).
    // ---------------------------------------------------------------------------------------
    let before = snapshot(&folder);
    let mut transport = SftpTransport::new().expect("a transport starts");
    let again = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        PublishMode::Live,
        &newsbuilder_core::build::EmbeddedBytes,
    )
    .expect("the second publish succeeds");

    assert!(
        again.uploaded.is_empty(),
        "SC-009: re-publishing an unchanged item uploaded {:?}",
        again
            .uploaded
            .iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>()
    );
    assert_eq!(again.unchanged.len(), published.uploaded.len());
    assert_eq!(
        before,
        snapshot(&folder),
        "SC-009: the server is not byte-for-byte what it was"
    );
    assert_eq!(
        published.fragment, again.fragment,
        "FR-024: the same item published twice produced different fragments"
    );

    // ---------------------------------------------------------------------------------------
    // 5. A wrong passphrase is an authentication failure that names the step (SC-007).
    // ---------------------------------------------------------------------------------------
    struct WrongPassphrase;
    impl SecretStore for WrongPassphrase {
        fn get(&self, _: &CredentialRef) -> Result<Option<Secret>> {
            Ok(Some(Secret::new("not-the-passphrase")))
        }
        fn set(&self, _: &CredentialRef, _: &Secret) -> Result<()> {
            Ok(())
        }
        fn delete(&self, _: &CredentialRef) -> Result<()> {
            Ok(())
        }
        fn available(&self) -> bool {
            true
        }
    }

    let mut transport = SftpTransport::new().expect("a transport starts");
    let denied = publish(
        &item,
        &server,
        &mut transport,
        &WrongPassphrase,
        PublishMode::Live,
        &newsbuilder_core::build::EmbeddedBytes,
    )
    .expect_err("a wrong passphrase must not authenticate");
    let message = denied.to_string();
    assert!(
        matches!(denied, Error::Transport { .. }),
        "a rejected key is a transport failure: {denied:?}"
    );
    assert!(
        message.contains("key") || message.contains("passphrase") || message.contains("private"),
        "the failure does not say what went wrong (SC-007): {message}"
    );
    assert!(
        !message.contains(PASSPHRASE) && !message.contains("not-the-passphrase"),
        "SC-010: the message carries the secret: {message}"
    );

    // And nothing about the failed attempt disturbed what was already published.
    assert_eq!(before, snapshot(&folder));
}
