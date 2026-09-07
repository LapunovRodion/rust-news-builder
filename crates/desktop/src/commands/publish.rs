//! Servers, credentials, and publishing (T065, T068).
//!
//! Two rules shape this module, and both are structural rather than a matter of care:
//!
//! - **No command returns a secret.** [`set_credential`] is the only one that accepts one, and
//!   it hands it to the store without logging, echoing or keeping it. Everything the frontend
//!   ever sees about a credential is its *kind* (FR-040, SC-010).
//! - **Publishing runs off the UI thread.** It is the one slow, network-bound operation in the
//!   product; running it on the invoke thread would freeze the window for its duration.
//!   Progress arrives as `publish-progress` events.

use newsbuilder_core::adapters::files::LocalFiles;
use newsbuilder_core::adapters::keyring_store::KeyringStore;
use newsbuilder_core::adapters::transport::SftpTransport;
use newsbuilder_core::error::Result as CoreResult;
use newsbuilder_core::model::server::{CredentialRef, ServerConfig};
use newsbuilder_core::model::server_store::ServerStore;
use newsbuilder_core::ports::SecretStore;
use newsbuilder_core::publish::{PublishMode, publish as core_publish};
use newsbuilder_core::secret::Secret;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::item::lock;
use crate::error::{CommandError, CommandResult};
use crate::state::{Session, SessionBytes, WarningView, warning_views};

/// A server configuration as the frontend sees it: connection settings and a credential
/// *kind*, never a credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub remote_base_path: String,
    pub public_base_url: String,
    /// `"password"` or `"key"`.
    pub auth: String,
    /// Set only when `auth` is `"key"`.
    pub key_path: Option<String>,
    /// Whether a credential is currently stored for it. A boolean, not a value.
    #[serde(default)]
    pub has_credential: bool,
}

/// What a publish produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationView {
    /// The fragment for the CMS, with real public URLs in it.
    pub fragment: String,
    pub remote_folder: String,
    pub slug: String,
    pub uploaded: Vec<PublishedFile>,
    /// Files the server already had, byte for byte, and which were left alone (FR-030).
    pub unchanged: Vec<String>,
    pub dry_run: bool,
    pub warnings: Vec<WarningView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedFile {
    pub name: String,
    pub url: String,
}

/// Progress, emitted as the upload proceeds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    file: String,
    index: usize,
    total: usize,
}

#[tauri::command]
pub fn list_servers() -> CommandResult<Vec<ServerView>> {
    let store = ServerStore::platform(LocalFiles::new())?;
    let secrets = KeyringStore::new();
    let store_reachable = secrets.available();

    Ok(store
        .load()?
        .into_iter()
        .map(|config| {
            let has_credential = store_reachable
                && secrets
                    .get(&config.credential)
                    .ok()
                    .flatten()
                    .is_some_and(|secret| !secret.is_empty());
            view_of(&config, has_credential)
        })
        .collect())
}

#[tauri::command]
pub fn save_server(config: ServerView) -> CommandResult<()> {
    let store = ServerStore::platform(LocalFiles::new())?;
    store.save(config_from(&config)?)?;
    Ok(())
}

#[tauri::command]
pub fn delete_server(name: String) -> CommandResult<()> {
    let store = ServerStore::platform(LocalFiles::new())?;
    if let Some(config) = store.get(&name)? {
        let secrets = KeyringStore::new();
        if secrets.available() {
            // A credential nobody can reach and nobody can clear is worse than no credential.
            let _ = secrets.delete(&config.credential);
        }
    }
    store.remove(&name)?;
    Ok(())
}

/// Stores a credential (FR-040).
///
/// The only command that accepts a secret. It goes straight into the OS store; nothing here
/// logs it, keeps it, or hands it back.
#[tauri::command]
pub fn set_credential(name: String, secret: String) -> CommandResult<()> {
    let store = ServerStore::platform(LocalFiles::new())?;
    let config = store.get(&name)?.ok_or_else(|| {
        CommandError::new(
            "no_such_server",
            format!("there is no saved server called `{name}`"),
        )
    })?;
    let secrets = KeyringStore::new();
    if !secrets.available() {
        return Err(CommandError::new(
            "secret_store_unavailable",
            "no operating system secret store is reachable. Enter the credential for this \
             session instead; it will not be written to disk.",
        ));
    }
    secrets.set(&config.credential, &Secret::new(secret))?;
    Ok(())
}

#[tauri::command]
pub fn delete_credential(name: String) -> CommandResult<()> {
    let store = ServerStore::platform(LocalFiles::new())?;
    let config = store.get(&name)?.ok_or_else(|| {
        CommandError::new(
            "no_such_server",
            format!("there is no saved server called `{name}`"),
        )
    })?;
    KeyringStore::new().delete(&config.credential)?;
    Ok(())
}

#[tauri::command]
pub fn secret_store_available() -> bool {
    KeyringStore::new().available()
}

/// Holds a credential for this session only (FR-041, T068).
///
/// Used when the OS store is unreachable. It lives in the session's memory and is written
/// nowhere — not to the configuration file, not to a cache, not to a log. Closing the window
/// is what clears it.
#[tauri::command]
pub fn set_session_credential(
    name: String,
    secret: String,
    session: State<'_, Session>,
) -> CommandResult<()> {
    let mut state = lock(&session)?;
    state.session_credentials.insert(name, Secret::new(secret));
    Ok(())
}

#[tauri::command]
pub fn clear_session_credential(name: String, session: State<'_, Session>) -> CommandResult<()> {
    let mut state = lock(&session)?;
    state.session_credentials.remove(&name);
    Ok(())
}

/// Publishes the open item (FR-026, FR-031).
///
/// `async` so Tauri runs it on its worker pool rather than on the thread that serves the
/// window: the whole call is network-bound, and the interface has to stay responsive through it.
#[tauri::command]
pub async fn publish(
    server: String,
    dry_run: bool,
    app: AppHandle,
    session: State<'_, Session>,
) -> CommandResult<PublicationView> {
    let store = ServerStore::platform(LocalFiles::new())?;
    let config = store.get(&server)?.ok_or_else(|| {
        CommandError::new(
            "no_such_server",
            format!("there is no saved server called `{server}`"),
        )
    })?;

    // The item and its bytes are copied out under the lock and the lock is released, so the
    // window keeps working while the upload runs.
    let (item, bytes, session_secret) = {
        let state = lock(&session)?;
        (
            state.item.clone(),
            state.bytes.clone(),
            state.session_credentials.get(&server).cloned(),
        )
    };

    let total = item.placed_photo_ids().len();
    let _ = app.emit(
        "publish-progress",
        Progress {
            file: String::new(),
            index: 0,
            total,
        },
    );

    let mut transport = SftpTransport::new()?;
    let secrets = ResolvedSecrets {
        keyring: KeyringStore::new(),
        session: session_secret,
        reference: config.credential.clone(),
    };
    let mode = if dry_run {
        PublishMode::DryRun
    } else {
        PublishMode::Live
    };

    let publication = core_publish(
        &item,
        &config,
        &mut transport,
        &secrets,
        mode,
        &SessionBytes(&bytes),
    )?;

    for (index, (name, _)) in publication.uploaded.iter().enumerate() {
        let _ = app.emit(
            "publish-progress",
            Progress {
                file: name.clone(),
                index: index + 1,
                total,
            },
        );
    }

    if !dry_run && let Ok(mut state) = session.0.lock() {
        state.dirty = false;
    }

    Ok(PublicationView {
        fragment: publication.fragment,
        remote_folder: publication.remote_folder,
        slug: publication.folder.to_string(),
        uploaded: publication
            .uploaded
            .iter()
            .map(|(name, url)| PublishedFile {
                name: name.clone(),
                url: url.to_string(),
            })
            .collect(),
        unchanged: publication.unchanged,
        dry_run: publication.dry_run,
        warnings: warning_views(&publication.warnings),
    })
}

/// The OS store, with a per-session credential in front of it (FR-041).
///
/// When the session holds one for this server it wins, which is what makes the fallback work
/// on a machine with no Secret Service at all: `available()` answers for the pair, not for the
/// keyring alone.
struct ResolvedSecrets {
    keyring: KeyringStore,
    session: Option<Secret>,
    reference: CredentialRef,
}

impl SecretStore for ResolvedSecrets {
    fn get(&self, r: &CredentialRef) -> CoreResult<Option<Secret>> {
        if *r == self.reference
            && let Some(secret) = &self.session
        {
            return Ok(Some(secret.clone()));
        }
        if self.keyring.available() {
            return self.keyring.get(r);
        }
        Ok(None)
    }

    fn set(&self, r: &CredentialRef, s: &Secret) -> CoreResult<()> {
        self.keyring.set(r, s)
    }

    fn delete(&self, r: &CredentialRef) -> CoreResult<()> {
        self.keyring.delete(r)
    }

    fn available(&self) -> bool {
        self.session.is_some() || self.keyring.available()
    }
}

fn view_of(config: &ServerConfig, has_credential: bool) -> ServerView {
    let (auth, key_path) = match &config.credential {
        CredentialRef::Key { path, .. } => {
            ("key".to_owned(), Some(path.to_string_lossy().into_owned()))
        }
        CredentialRef::Password { .. } => ("password".to_owned(), None),
    };
    ServerView {
        name: config.name.clone(),
        host: config.host.clone(),
        user: config.user.clone(),
        port: config.port,
        remote_base_path: config.remote_base_path.clone(),
        public_base_url: config.public_base_url.to_string(),
        auth,
        key_path,
        has_credential,
    }
}

fn config_from(view: &ServerView) -> CommandResult<ServerConfig> {
    let public_base_url = url::Url::parse(&view.public_base_url).map_err(|error| {
        CommandError::new(
            "invalid_url",
            format!("`{}` is not a URL: {error}", view.public_base_url),
        )
    })?;

    let credential = if view.auth == "key" {
        let path = view.key_path.as_deref().ok_or_else(|| {
            CommandError::new(
                "missing_key_path",
                "key authentication needs the path of the private key",
            )
        })?;
        CredentialRef::Key {
            path: path.into(),
            server: view.name.clone(),
        }
    } else {
        CredentialRef::Password {
            server: view.name.clone(),
        }
    };

    if view.name.trim().is_empty() {
        return Err(CommandError::new(
            "invalid_server",
            "a server configuration needs a name",
        ));
    }

    Ok(ServerConfig {
        name: view.name.clone(),
        host: view.host.clone(),
        user: view.user.clone(),
        port: view.port,
        remote_base_path: view.remote_base_path.clone(),
        public_base_url,
        credential,
    })
}
