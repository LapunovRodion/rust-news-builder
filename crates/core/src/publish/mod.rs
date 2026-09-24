//! Upload orchestration: resolve, refuse early, converge, never delete.
//!
//! Two properties matter more than the wire protocol, and both live here rather than in the
//! SFTP adapter, so both are provable against a recording fake (research R10):
//!
//! - **Convergence** (FR-030): the desired remote set is computed from the item, compared
//!   against what is present, and only the difference is uploaded. Re-publishing an unchanged
//!   item performs zero writes.
//! - **No deletion** (deviation D-7): a photo dropped from an item stays on the server as an
//!   orphan. [`Transport`] has no removal method, so this cannot be violated by mistake.

pub mod article;
pub mod paths;
pub mod slug;

pub use article::{check_site, publish_to_site};

use url::Url;

use crate::build::{BuildContext, PhotoBytesSource, build};
use crate::error::{Error, Result, Warning};
use crate::model::item::NewsItem;
use crate::model::server::{ServerConfig, Slug};
use crate::model::site::ArticleOutcome;
use crate::ports::{SecretStore, Transport};
use crate::secret::Secret;

/// Whether a publish may touch the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishMode {
    /// Plan everything, mutate nothing (FR-031).
    DryRun,
    /// Actually upload.
    Live,
}

/// The record of one publish attempt.
#[derive(Debug, Clone)]
pub struct Publication {
    /// The fragment for the CMS, with the photos' real public URLs in it.
    ///
    /// Publishing builds this on the way past, and both frontends need it: it is the artefact
    /// the whole exercise is for (US2). Returning it here rather than making the caller build
    /// a second time is also what keeps the fragment consistent with the folder that was
    /// actually used, which a suffixed slug can change under them (deviation D-8).
    pub fragment: String,
    /// The folder actually published into, without the base path. May be suffixed.
    pub folder: Slug,
    /// `remote_base_path` + `/` + the folder actually used.
    pub remote_folder: String,
    /// Every published file and its public URL, in item order.
    pub uploaded: Vec<(String, Url)>,
    /// Every placed photo's public URL, uploaded now or already there, in item order. What the
    /// article's cover is chosen from (002).
    pub photo_urls: Vec<(crate::model::photo::PhotoId, Url)>,
    /// Files already on the server and left alone. Empty on a first publish; on a re-publish
    /// of an unchanged item this holds everything and `uploaded` is empty (SC-009).
    pub unchanged: Vec<String>,
    /// True when no remote mutation occurred (FR-031).
    pub dry_run: bool,
    /// True when the folder already held this item's files before this publish — the evidence
    /// that the item was published before, whether or not any photo changed since (002
    /// research R6).
    pub folder_existed: bool,
    /// Everything the build and the upload wanted to say (FR-034).
    pub warnings: Vec<Warning>,
    /// What happened to the article, when the server has a site target (002). `None` means the
    /// publish was photos only, exactly as in 001.
    pub article: Option<ArticleOutcome>,
}

/// Publishes an item.
///
/// Refuses before processing any photo when no credential resolves (FR-029, INV-8) or when the
/// remote base path cannot be used. Photos no placement references are not uploaded.
pub fn publish(
    item: &NewsItem,
    server: &ServerConfig,
    transport: &mut dyn Transport,
    secrets: &dyn SecretStore,
    mode: PublishMode,
    sources: &dyn PhotoBytesSource,
) -> Result<Publication> {
    let dry_run = mode == PublishMode::DryRun;

    // 1. The credential, before anything expensive happens (FR-029).
    let credential = credential(server, secrets)?;

    // 2. The connection and the base path, still before any photo is touched.
    transport.connect(server, &credential)?;
    transport
        .list(&server.remote_base_path)
        .map_err(|error| Error::RemotePathUnusable {
            path: server.remote_base_path.clone(),
            detail: error.to_string(),
        })?;

    // 3. The folder, suffixed if it is already another item's (deviation D-8).
    let mut warnings = Vec::new();
    let folder = resolve_folder(item, server, transport, &mut warnings)?;
    let remote_folder = paths::remote_folder(&server.remote_base_path, &folder);

    // 4. The build. Offline, and the same call the preview makes (FR-022).
    let output = build(
        item,
        &BuildContext {
            public_base_url: Some(server.public_base_url.clone()),
            slug: folder.clone(),
        },
        sources,
    )?;
    warnings.extend(output.warnings);

    // 5. Converge: list what is there, upload only what differs.
    let present = transport.list(&remote_folder).unwrap_or_default();
    let mut uploaded = Vec::new();
    let mut unchanged = Vec::new();
    let mut photo_urls = Vec::new();
    let mut created_directory = false;

    for processed in &output.processed {
        let url = Url::parse(&paths::public_url(
            server.public_base_url.as_str(),
            &folder,
            &processed.file_name,
        ))
        .map_err(|e| Error::RemotePathUnusable {
            path: processed.file_name.clone(),
            detail: format!("the public URL could not be built: {e}"),
        })?;

        photo_urls.push((processed.id, url.clone()));

        let already = present
            .iter()
            .find(|entry| entry.name == processed.file_name)
            .is_some_and(|entry| entry.size == processed.bytes.len() as u64);

        if already {
            unchanged.push(processed.file_name.clone());
            continue;
        }

        if !dry_run {
            if !created_directory {
                transport.ensure_dir(&remote_folder)?;
                created_directory = true;
            }
            let remote_path =
                paths::remote_file(&server.remote_base_path, &folder, &processed.file_name);
            transport
                .put(&remote_path, &processed.bytes)
                .map_err(|error| Error::Transport {
                    step: format!("uploading {}", processed.file_name),
                    detail: error.to_string(),
                })?;
        }
        uploaded.push((processed.file_name.clone(), url));
    }

    Ok(Publication {
        fragment: output.fragment,
        folder,
        remote_folder,
        uploaded,
        unchanged,
        photo_urls,
        dry_run,
        folder_existed: !present.is_empty(),
        warnings,
        article: None,
    })
}

/// The server's credential, or the refusal that says why there is none (FR-029, INV-8).
pub(crate) fn credential(server: &ServerConfig, secrets: &dyn SecretStore) -> Result<Secret> {
    if !secrets.available() {
        return Err(Error::SecretStoreUnavailable {
            detail: format!(
                "no operating system secret store is reachable, so the credential for \
                 `{}` cannot be read",
                server.name
            ),
        });
    }
    secrets
        .get(&server.credential)?
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| Error::NoCredential {
            server: server.name.clone(),
        })
}

/// Picks the folder to publish into, suffixing past a folder that holds a different item.
///
/// "A different item" is decided by evidence rather than by bookkeeping: a folder that holds
/// files and none of ours is someone else's. A folder holding some of ours is ours, and is
/// converged into. An empty folder is free to use.
fn resolve_folder(
    item: &NewsItem,
    server: &ServerConfig,
    transport: &mut dyn Transport,
    warnings: &mut Vec<Warning>,
) -> Result<Slug> {
    let wanted = item.slug.clone();
    let stem = slug::slugify(&item.title);
    let prefix = format!("{}-", stem.as_str());

    for attempt in 0..100u32 {
        let candidate = if attempt == 0 {
            wanted.clone()
        } else {
            wanted.suffixed(attempt + 1)
        };
        let remote = paths::remote_folder(&server.remote_base_path, &candidate);
        let present = transport.list(&remote).unwrap_or_default();

        let free = present.is_empty();
        let ours = present.iter().any(|entry| entry.name.starts_with(&prefix));
        if free || ours {
            if candidate != wanted {
                warnings.push(Warning::SlugSuffixed {
                    from: wanted.to_string(),
                    to: candidate.to_string(),
                });
            }
            return Ok(candidate);
        }
    }

    Err(Error::RemotePathUnusable {
        path: paths::remote_folder(&server.remote_base_path, &wanted),
        detail: "a hundred suffixed folder names were all taken by other items".to_owned(),
    })
}
