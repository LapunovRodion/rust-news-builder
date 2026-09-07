//! SFTP over SSH (T060, FR-029 – FR-031), on `russh` + `russh-sftp` (research R6).
//!
//! Pure Rust, so a Windows build needs no libssh2 and no system OpenSSL, and nothing here
//! shells out to an external `ssh` binary — that fallback is gone (deviation D-3).
//!
//! Three things about this module are deliberate:
//!
//! - **It is behind the `sftp` feature.** The domain crate's default build, and therefore every
//!   parity test, has no async runtime and no network stack in its dependency graph. Both
//!   frontends turn the feature on.
//! - **It bridges async to sync by owning a runtime.** [`Transport`] is a synchronous trait
//!   because publishing is a sequence of blocking steps with a progress report between them;
//!   `russh` is async. The adapter owns a current-thread runtime and blocks on each call, which
//!   keeps the asynchrony from leaking into the domain.
//! - **It checks the host key.** An unknown or changed host key is refused rather than
//!   accepted, because accepting one silently is what makes a machine-in-the-middle work
//!   (deviation D-15).
//!
//! [`Secret::expose`] is called in exactly one place in this crate, and it is here.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use russh::client::{self, AuthResult, Handle};
use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::ssh_key::PublicKey;
use russh::keys::{PrivateKeyWithHashAlg, check_known_hosts_path, load_secret_key};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::StatusCode;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Runtime;

use crate::error::{Error, Result};
use crate::model::server::{CredentialRef, ServerConfig};
use crate::ports::{RemoteEntry, Transport};
use crate::secret::Secret;

/// Why a host key was turned down, carried out of the async handler.
type Rejection = Arc<Mutex<Option<String>>>;

/// Where the host keys are recorded.
///
/// Resolved here rather than by `russh`, because `russh`'s own resolution is wrong on Windows:
/// it reads `%USERPROFILE%\ssh\known_hosts`, without the dot, while `ssh.exe` writes
/// `%USERPROFILE%\.ssh\known_hosts` (russh 0.52). Every publish from a Windows machine
/// therefore reported an unknown host, however many times the operator had accepted the key in
/// a terminal — and a missing file is indistinguishable from an unknown host inside `russh`, so
/// the message could not say what was actually wrong.
///
/// `SSH_KNOWN_HOSTS` overrides the location, for a layout that is neither.
fn known_hosts_path() -> Option<PathBuf> {
    known_hosts_at(std::env::var_os("SSH_KNOWN_HOSTS"), home_dir())
}

/// The rule itself, with both of its inputs passed in so it can be tested without touching the
/// environment of the process running the test.
fn known_hosts_at(explicit: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    match explicit.filter(|value| !value.is_empty()) {
        Some(value) => Some(PathBuf::from(value)),
        None => home.map(|home| home.join(".ssh").join("known_hosts")),
    }
}

/// The account's home directory, from the environment first — which is both what `ssh` itself
/// does and what lets the end-to-end test redirect `HOME` away from the developer's real
/// `known_hosts`.
fn home_dir() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    if let Some(home) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(home));
    }
    directories::UserDirs::new().map(|dirs| dirs.home_dir().to_owned())
}

/// Verifies the server's identity against the recorded host keys.
struct HostKeyCheck {
    host: String,
    port: u16,
    /// Resolved once, before connecting, so every message can name the file actually read.
    known_hosts: Option<PathBuf>,
    rejected: Rejection,
}

impl HostKeyCheck {
    /// Why a key that did not match did not match.
    ///
    /// `check_known_hosts_path` answers `false` both when the file records nothing for this host
    /// and when it records a key of another type. The two have different fixes, so they are told
    /// apart here rather than reported as one.
    fn unmatched(&self, path: &Path, offered: &PublicKey) -> String {
        let recorded = known_host_keys_path(&self.host, self.port, path).unwrap_or_default();
        if let Some(types) = recorded_types(&recorded) {
            return format!(
                "{} records a key for {}:{}, but only as {types}, and the server offered {}. \
                 Record that one too, with `ssh-keyscan -p {} -t {} {}`, then publish again.",
                path.display(),
                self.host,
                self.port,
                offered.algorithm().as_str(),
                self.port,
                offered.algorithm().as_str(),
                self.host,
            );
        }

        // Refusing an unknown host is the safe answer, and the message says exactly how to make
        // it known — which `ssh` itself would have done on a first connection.
        let absent = if path.exists() {
            ""
        } else {
            " That file does not exist."
        };
        format!(
            "the host key for {}:{} is not in {}.{absent} Connect once with `ssh -p {} {}` and \
             accept the key, or add it with `ssh-keyscan`, then publish again.",
            self.host,
            self.port,
            path.display(),
            self.port,
            self.host
        )
    }
}

/// The key types recorded for a host, as a readable list, or `None` if none are.
fn recorded_types(recorded: &[(usize, PublicKey)]) -> Option<String> {
    if recorded.is_empty() {
        return None;
    }
    let mut types: Vec<String> = recorded
        .iter()
        .map(|(_, key)| key.algorithm().as_str().to_owned())
        .collect();
    types.sort_unstable();
    types.dedup();
    Some(types.join(", "))
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let note = |text: String| {
            if let Ok(mut slot) = self.rejected.lock() {
                *slot = Some(text);
            }
        };

        let Some(path) = self.known_hosts.as_deref() else {
            note(
                "there is no home directory to read `.ssh/known_hosts` from, so the server's \
                 identity cannot be checked. Point SSH_KNOWN_HOSTS at the file that records it."
                    .to_owned(),
            );
            return Ok(false);
        };

        match check_known_hosts_path(&self.host, self.port, server_public_key, path) {
            Ok(true) => Ok(true),
            Ok(false) => {
                note(self.unmatched(path, server_public_key));
                Ok(false)
            }
            Err(error) => {
                note(format!(
                    "the host key for {}:{} does not match the one in {} ({error}). This is what \
                     a machine-in-the-middle looks like; it is also what a rebuilt server looks \
                     like. Verify the new key out of band before removing the old entry.",
                    self.host,
                    self.port,
                    path.display()
                ));
                Ok(false)
            }
        }
    }
}

/// [`Transport`] over SFTP.
///
/// One instance is one connection. `connect` may be called again to reconnect; the previous
/// session is dropped.
pub struct SftpTransport {
    runtime: Runtime,
    session: Option<Handle<HostKeyCheck>>,
    sftp: Option<SftpSession>,
}

impl std::fmt::Debug for SftpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Neither the session nor the runtime has a useful `Debug`, and printing a connection
        // is a good way to print something that should not be printed.
        f.debug_struct("SftpTransport")
            .field("connected", &self.sftp.is_some())
            .finish()
    }
}

impl SftpTransport {
    /// A transport with its own single-threaded runtime.
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| Error::Transport {
                step: "start the network runtime".to_owned(),
                detail: error.to_string(),
            })?;
        Ok(Self {
            runtime,
            session: None,
            sftp: None,
        })
    }

    /// The open SFTP session, or a clear error saying `connect` was never called.
    fn sftp(&self) -> Result<&SftpSession> {
        self.sftp.as_ref().ok_or_else(|| Error::Transport {
            step: "use the connection".to_owned(),
            detail: "the transport is not connected".to_owned(),
        })
    }
}

impl Transport for SftpTransport {
    fn connect(&mut self, target: &ServerConfig, cred: &Secret) -> Result<()> {
        self.sftp = None;
        self.session = None;

        let rejected: Rejection = Arc::new(Mutex::new(None));
        let handler = HostKeyCheck {
            host: target.host.clone(),
            port: target.port,
            known_hosts: known_hosts_path(),
            rejected: Arc::clone(&rejected),
        };

        let config = Arc::new(client::Config::default());
        let address = (target.host.clone(), target.port);

        // The whole conversation happens inside one `block_on`, so a partially authenticated
        // session is never left behind for a later call to trip over.
        let (session, sftp) = self.runtime.block_on(async {
            let mut session = client::connect(config, address, handler)
                .await
                .map_err(|error| {
                    // A refused host key surfaces as a connection failure, so the specific
                    // reason has to be recovered here or the operator gets "connection closed".
                    let reason = rejected
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone())
                        .unwrap_or_else(|| error.to_string());
                    Error::Transport {
                        step: format!("connect to {}:{}", target.host, target.port),
                        detail: reason,
                    }
                })?;

            let outcome = authenticate(&mut session, target, cred).await?;
            if !outcome.success() {
                return Err(Error::Transport {
                    step: format!("authenticate as `{}`", target.user),
                    detail: match &target.credential {
                        CredentialRef::Password { .. } => {
                            "the server rejected the stored password".to_owned()
                        }
                        CredentialRef::Key { path, .. } => {
                            format!("the server rejected the key at {}", path.display())
                        }
                    },
                });
            }

            let channel =
                session
                    .channel_open_session()
                    .await
                    .map_err(|error| Error::Transport {
                        step: "open a session channel".to_owned(),
                        detail: error.to_string(),
                    })?;
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|error| Error::Transport {
                    step: "start the sftp subsystem".to_owned(),
                    detail: format!(
                        "{error}. The server allows SSH but may not offer SFTP for this account."
                    ),
                })?;
            let sftp = SftpSession::new(channel.into_stream())
                .await
                .map_err(|error| Error::Transport {
                    step: "open the sftp session".to_owned(),
                    detail: error.to_string(),
                })?;

            Ok::<_, Error>((session, sftp))
        })?;

        self.session = Some(session);
        self.sftp = Some(sftp);
        Ok(())
    }

    fn list(&mut self, remote_dir: &str) -> Result<Vec<RemoteEntry>> {
        let sftp = self.sftp()?;
        self.runtime.block_on(async {
            match sftp.read_dir(remote_dir).await {
                Ok(entries) => Ok(entries
                    .map(|entry| RemoteEntry {
                        name: entry.file_name(),
                        size: entry.metadata().size.unwrap_or(0),
                    })
                    .collect()),
                // A directory that is not there yet lists as empty — that is what an item's
                // first publish looks like, and the trait says so.
                Err(error) if is_absent(&error) => Ok(Vec::new()),
                Err(error) => Err(Error::RemotePathUnusable {
                    path: remote_dir.to_owned(),
                    detail: error.to_string(),
                }),
            }
        })
    }

    fn ensure_dir(&mut self, remote_dir: &str) -> Result<()> {
        let sftp = self.sftp()?;
        self.runtime.block_on(async {
            // Parents first, so a base path one level deeper than expected still works.
            for ancestor in ancestors(remote_dir) {
                match sftp.create_dir(ancestor.clone()).await {
                    Ok(()) => {}
                    // Already there is the desired state, not a failure.
                    Err(error) if is_present(&error) => {}
                    Err(error) => {
                        return Err(Error::RemotePathUnusable {
                            path: ancestor,
                            detail: error.to_string(),
                        });
                    }
                }
            }
            Ok(())
        })
    }

    fn put(&mut self, remote_path: &str, bytes: &[u8]) -> Result<()> {
        let sftp = self.sftp()?;
        self.runtime.block_on(async {
            // `SftpSession::write` is not what this wants: it opens with `OpenFlags::WRITE`
            // alone, which fails outright on a file that is not already there — every first
            // publish — and, on one that is, writes over the front and leaves whatever the old
            // longer file had after it. `create` is `CREATE | TRUNCATE | WRITE`, which is the
            // "overwriting any file already at that path" the port promises.
            //
            // Found by `tests/e2e_sshd.rs`; a recording fake cannot see this.
            let mut file = sftp
                .create(remote_path)
                .await
                .map_err(|error| Error::Transport {
                    step: format!("create {remote_path}"),
                    detail: error.to_string(),
                })?;

            file.write_all(bytes)
                .await
                .map_err(|error| Error::Transport {
                    step: format!("put {remote_path}"),
                    detail: error.to_string(),
                })?;

            // Closes the remote handle. Without it the file is at the server's discretion until
            // the session ends, and a publish that reported success could still be incomplete.
            file.shutdown().await.map_err(|error| Error::Transport {
                step: format!("close {remote_path}"),
                detail: error.to_string(),
            })
        })
    }
}

/// Authenticates by whichever method the configuration names (FR-029).
async fn authenticate(
    session: &mut Handle<HostKeyCheck>,
    target: &ServerConfig,
    cred: &Secret,
) -> Result<AuthResult> {
    match &target.credential {
        CredentialRef::Password { .. } => session
            .authenticate_password(target.user.clone(), cred.expose())
            .await
            .map_err(|error| Error::Transport {
                step: "password authentication".to_owned(),
                detail: error.to_string(),
            }),

        CredentialRef::Key { path, .. } => {
            // An empty stored secret means the key has no passphrase, which is different from
            // a passphrase that happens to be the empty string.
            let passphrase = if cred.is_empty() {
                None
            } else {
                Some(cred.expose())
            };
            let key = load_secret_key(path, passphrase).map_err(|error| Error::Transport {
                step: format!("read the private key at {}", path.display()),
                detail: error.to_string(),
            })?;
            session
                .authenticate_publickey(
                    target.user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), None),
                )
                .await
                .map_err(|error| Error::Transport {
                    step: "public-key authentication".to_owned(),
                    detail: error.to_string(),
                })
        }
    }
}

/// Whether an SFTP failure means "there is nothing at that path".
fn is_absent(error: &russh_sftp::client::error::Error) -> bool {
    matches!(
        error,
        russh_sftp::client::error::Error::Status(status)
            if status.status_code == StatusCode::NoSuchFile
    )
}

/// Whether an SFTP failure means "it is already there", which `ensure_dir` wants to ignore.
fn is_present(error: &russh_sftp::client::error::Error) -> bool {
    matches!(
        error,
        russh_sftp::client::error::Error::Status(status)
            if status.status_code == StatusCode::Failure
                || status.status_code == StatusCode::PermissionDenied
    )
}

/// Every directory on the way to `path`, outermost first.
///
/// Split here rather than with `Path`, because a remote path is POSIX whatever the machine
/// running this happens to be — building it with `std::path` on Windows would produce
/// backslashes the server has never heard of.
fn ancestors(path: &str) -> Vec<String> {
    let absolute = path.starts_with('/');
    let mut out = Vec::new();
    let mut current = String::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        if current.is_empty() && !absolute {
            current.push_str(part);
        } else {
            current.push('/');
            current.push_str(part);
        }
        out.push(current.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use super::{ancestors, known_hosts_at};

    #[test]
    fn the_host_keys_are_read_from_dot_ssh_on_every_platform() {
        // russh 0.52 reads `<home>/ssh/known_hosts` on Windows, without the dot, which is not
        // where `ssh.exe` writes. The dot is the whole bug, so it is the whole assertion.
        let path = known_hosts_at(None, Some(PathBuf::from("/home/rodion")))
            .expect("a home directory is enough to locate the file");
        assert!(
            path.ends_with(Path::new(".ssh/known_hosts")),
            "{} is not the file ssh itself writes",
            path.display()
        );
    }

    #[test]
    fn an_explicit_location_wins_over_the_home_directory() {
        assert_eq!(
            known_hosts_at(
                Some(OsString::from("/etc/ssh/ssh_known_hosts")),
                Some(PathBuf::from("/home/rodion"))
            ),
            Some(PathBuf::from("/etc/ssh/ssh_known_hosts"))
        );
    }

    #[test]
    fn an_empty_override_is_no_override() {
        assert_eq!(
            known_hosts_at(Some(OsString::new()), Some(PathBuf::from("/home/rodion"))),
            Some(PathBuf::from("/home/rodion/.ssh/known_hosts"))
        );
    }

    #[test]
    fn without_a_home_there_is_nowhere_to_look() {
        assert_eq!(known_hosts_at(None, None), None);
    }

    #[test]
    fn an_absolute_path_yields_each_directory_outermost_first() {
        assert_eq!(
            ancestors("/var/www/html/news/item"),
            vec![
                "/var",
                "/var/www",
                "/var/www/html",
                "/var/www/html/news",
                "/var/www/html/news/item"
            ]
        );
    }

    #[test]
    fn a_relative_path_stays_relative() {
        assert_eq!(ancestors("news/item"), vec!["news", "news/item"]);
    }

    #[test]
    fn repeated_and_trailing_slashes_do_not_produce_empty_names() {
        assert_eq!(ancestors("/var//news/"), vec!["/var", "/var/news"]);
    }

    #[test]
    fn the_root_has_no_ancestors_to_create() {
        assert!(ancestors("/").is_empty());
    }
}
