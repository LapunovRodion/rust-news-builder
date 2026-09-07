//! `newsbuilder` — the command-line half of the tool (FR-036 – FR-039).
//!
//! Deliberately small. It builds and publishes an item that already needs no human decision;
//! arrangement, cropping and editing belong to the desktop application. Where a run *would*
//! need a decision, it refuses and says so rather than guessing (contracts/cli.md).
//!
//! There is no second code path: every rule this binary applies comes from `newsbuilder-core`,
//! which is what makes FR-039's promise — the same item publishes identically from either
//! interface — structural rather than a thing to keep in step by hand.

mod exit;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use url::Url;

use newsbuilder_core::adapters::files::LocalFiles;
use newsbuilder_core::adapters::keyring_store::KeyringStore;
use newsbuilder_core::adapters::transport::SftpTransport;
use newsbuilder_core::build::{BuildContext, BuildOutput, EmbeddedBytes, build};
use newsbuilder_core::error::{Error, Warning};
use newsbuilder_core::import::{attach_photos, import_document};
use newsbuilder_core::model::appearance_config;
use newsbuilder_core::model::item::{NewsItem, SourceFormat};
use newsbuilder_core::model::photo::PhotoSource;
use newsbuilder_core::model::server::{CredentialRef, ServerConfig, Slug};
use newsbuilder_core::model::server_store::ServerStore;
use newsbuilder_core::ports::SecretStore;
use newsbuilder_core::publish::slug::slugify;
use newsbuilder_core::publish::{PublishMode, paths, publish};
use newsbuilder_core::secret::Secret;

use exit::{Exit, Failure, Refusal};

#[derive(Debug, Parser)]
#[command(
    name = "newsbuilder",
    version,
    about = "Build and publish a prepared news item",
    long_about = "Builds an inline-styled HTML fragment from a Word, Markdown or plain-text \
                  document, and publishes its photos over SFTP.\n\nThis is the automation \
                  surface. Anything that needs a human decision — arranging photos, cropping \
                  them — belongs to the desktop application, and is refused here rather than \
                  guessed at."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Render the fragment locally. Never touches the network.
    Build(BuildArgs),
    /// Build, then upload the photos and report the fragment.
    Publish(PublishArgs),
    /// Manage saved publishing targets.
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
}

/// The flags that describe *which item* — shared by `build` and `publish` so the two cannot
/// drift into reading a document differently (FR-039).
#[derive(Debug, Args)]
struct SourceArgs {
    /// The source document: `.docx`, `.txt`, or `.md`.
    #[arg(long, value_name = "PATH")]
    input: PathBuf,

    /// Photos for a marker document. Unnecessary for a `.docx` with embedded images.
    #[arg(long, value_name = "PATH")]
    images_dir: Option<PathBuf>,

    /// Overrides the title detected in the document.
    #[arg(long, value_name = "TEXT")]
    title: Option<String>,

    /// Overrides the slug derived from the title.
    #[arg(long, value_name = "TEXT")]
    news_slug: Option<String>,

    /// A JSON appearance override, applied over the built-in appearance.
    #[arg(long, value_name = "PATH")]
    appearance: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct BuildArgs {
    #[command(flatten)]
    source: SourceArgs,

    /// Where the HTML fragment is written.
    #[arg(long, value_name = "PATH")]
    output: PathBuf,

    /// Where the photos will live once published. Without it, image URLs are local
    /// placeholders and the fragment is a preview.
    #[arg(long, value_name = "URL")]
    public_base_url: Option<Url>,

    /// Write a single JSON result object to stdout instead of prose.
    #[arg(long)]
    json: bool,

    /// Exit non-zero when the run produced warnings.
    #[arg(long)]
    strict: bool,
}

#[derive(Debug, Args)]
struct PublishArgs {
    #[command(flatten)]
    source: SourceArgs,

    /// A saved server configuration, by name.
    #[arg(long, value_name = "NAME")]
    server: String,

    /// Where to also write the HTML fragment. Optional: the fragment is reported either way.
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// Plan everything and mutate nothing (FR-031).
    #[arg(long)]
    dry_run: bool,

    /// Write a single JSON result object to stdout instead of prose.
    #[arg(long)]
    json: bool,

    /// Exit non-zero when the run produced warnings.
    #[arg(long)]
    strict: bool,
}

#[derive(Debug, Subcommand)]
enum ServerCommand {
    /// Show the saved configurations. Never shows a credential.
    List {
        /// Write a single JSON result object to stdout instead of prose.
        #[arg(long)]
        json: bool,
    },
    /// Add a configuration, or replace the one with the same name.
    Add(ServerAddArgs),
    /// Forget a configuration. The stored credential goes with it.
    Remove {
        /// The configuration to remove.
        name: String,
    },
    /// Store the password or key passphrase for a configuration.
    ///
    /// Read from a no-echo prompt, or from stdin when that is not a terminal. Never from an
    /// argument, so it cannot land in shell history or a process listing (FR-040).
    SetCredential {
        /// The configuration the credential belongs to.
        name: String,
    },
}

#[derive(Debug, Args)]
struct ServerAddArgs {
    /// The name this configuration is known by.
    #[arg(long, value_name = "NAME")]
    name: String,

    /// SSH host.
    #[arg(long, value_name = "HOST")]
    host: String,

    /// SSH user.
    #[arg(long, value_name = "USER")]
    user: String,

    /// SSH port.
    #[arg(long, value_name = "PORT", default_value_t = 22)]
    port: u16,

    /// The directory item folders are created beneath.
    #[arg(long, value_name = "PATH")]
    remote_base_path: String,

    /// Item URLs are built beneath this.
    #[arg(long, value_name = "URL")]
    public_base_url: Url,

    /// A private key to authenticate with. Without it, a password is expected.
    #[arg(long, value_name = "PATH")]
    key: Option<PathBuf>,
}

fn main() -> std::process::ExitCode {
    init_tracing();

    // `--json` is read before parsing so that a flag error still answers a script in the shape
    // it asked for. Parsing cannot report a flag problem in a document it failed to parse.
    let wants_json = std::env::args().any(|arg| arg == "--json");

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return finish_parse_error(&error, wants_json),
    };

    match cli.command {
        Command::Build(args) => match run_build(&args) {
            Ok(report) => report.finish(args.json, args.strict),
            Err(failure) => fail(&failure, args.json),
        },
        Command::Publish(args) => match run_publish(&args) {
            Ok(report) => report.finish(args.json, args.strict),
            Err(failure) => fail(&failure, args.json),
        },
        Command::Server { command } => {
            let json = matches!(command, ServerCommand::List { json: true });
            match run_server(command) {
                Ok(()) => code(Exit::Success),
                Err(failure) => fail(&failure, json),
            }
        }
    }
}

/// Installs the log subscriber (T020).
///
/// It belongs here rather than in the library: a crate that installs a global subscriber on its
/// users' behalf takes a decision that is theirs. Off unless `RUST_LOG` asks for it, so a
/// script's stderr stays the warnings and nothing else.
///
/// Nothing that reaches it can print a credential. `Secret` renders as `[redacted]` through both
/// `Debug` and `Display`, so there is no formatting path that leaks one (SC-010).
fn init_tracing() {
    let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Reports a clap error under this contract's exit codes rather than clap's own.
fn finish_parse_error(error: &clap::Error, wants_json: bool) -> std::process::ExitCode {
    use clap::error::ErrorKind;

    // `--help` and `--version` are successful runs that happen to print.
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        print!("{error}");
        return code(Exit::Success);
    }

    let failure = Failure::Usage(error.render().to_string().trim_end().to_owned());
    fail(&failure, wants_json)
}

/// Prints a failure and returns its status.
fn fail(failure: &Failure, json: bool) -> std::process::ExitCode {
    if json {
        println!("{}", exit::failure_json(failure));
    }
    // Diagnostics go to stderr whether or not stdout is JSON, so a script that only reads the
    // document still leaves a readable trace in its log.
    eprintln!("newsbuilder: {}", failure.detail());
    code(failure.exit())
}

fn code(exit: Exit) -> std::process::ExitCode {
    std::process::ExitCode::from(exit.code() as u8)
}

// =============================================================================================
// build (T040, T046)
// =============================================================================================

/// What a successful `build` or `publish` has to say.
///
/// One shape for both, because a script that can read the result of one should not have to
/// learn a second document to read the other. Fields that only publishing can fill are absent
/// from a build rather than present and empty.
struct Report {
    fragment_path: Option<PathBuf>,
    /// The fragment itself, carried when nowhere was named to write it. A publish that was not
    /// given `--output` still has to hand over the artefact the run exists to produce.
    fragment: Option<String>,
    slug: Slug,
    dry_run: bool,
    remote_folder: Option<String>,
    photos: Vec<PhotoLine>,
    warnings: Vec<Warning>,
}

/// One photo, in the shape contracts/cli.md's `--json` document expects.
struct PhotoLine {
    name: String,
    bytes: Option<usize>,
    quality: Option<u8>,
    url: String,
    /// Absent for a build. `false` on a publish means the server already had this exact file
    /// and it was left alone (FR-030).
    uploaded: Option<bool>,
}

impl Report {
    fn finish(&self, json: bool, strict: bool) -> std::process::ExitCode {
        if json {
            println!("{}", self.to_json());
        } else {
            match (&self.fragment_path, &self.fragment) {
                (Some(path), _) => println!("{}", path.display()),
                (None, Some(fragment)) => println!("{fragment}"),
                (None, None) => println!("{}", self.slug.as_str()),
            }
            if let Some(folder) = &self.remote_folder {
                let uploaded = self
                    .photos
                    .iter()
                    .filter(|p| p.uploaded == Some(true))
                    .count();
                let kept = self.photos.len() - uploaded;
                let verb = if self.dry_run {
                    "would publish"
                } else {
                    "published"
                };
                eprintln!(
                    "newsbuilder: {verb} {uploaded} photo(s) to {folder}, {kept} already there"
                );
            }
        }

        for warning in &self.warnings {
            eprintln!("newsbuilder: warning: {warning}");
        }

        if strict && !self.warnings.is_empty() {
            eprintln!(
                "newsbuilder: {} warning(s) and --strict was given",
                self.warnings.len()
            );
            // The warnings are all about what was fed in, so this is an input problem.
            return code(Exit::Input);
        }
        code(Exit::Success)
    }

    fn to_json(&self) -> serde_json::Value {
        let mut document = serde_json::json!({
            "ok": true,
            "fragment_path": self.fragment_path.as_ref().map(|p| p.display().to_string()),
            "slug": self.slug.as_str(),
            "dry_run": self.dry_run,
            "photos": self.photos.iter().map(|photo| {
                let mut one = serde_json::json!({
                    "name": photo.name,
                    "bytes": photo.bytes,
                    "quality": photo.quality,
                    "url": photo.url,
                });
                if let Some(uploaded) = photo.uploaded
                    && let Some(object) = one.as_object_mut()
                {
                    object.insert("uploaded".to_owned(), uploaded.into());
                }
                one
            }).collect::<Vec<_>>(),
            "warnings": exit::warnings_json(&self.warnings),
        });
        if let Some(object) = document.as_object_mut() {
            if let Some(folder) = &self.remote_folder {
                object.insert("remote_folder".to_owned(), folder.clone().into());
            }
            if let Some(fragment) = &self.fragment {
                object.insert("fragment".to_owned(), fragment.clone().into());
            }
        }
        document
    }
}

fn run_build(args: &BuildArgs) -> Result<Report, Failure> {
    let mut warnings = Vec::new();
    let item = prepare(&args.source, &mut warnings)?;

    let ctx = BuildContext {
        public_base_url: args.public_base_url.clone(),
        slug: item.slug.clone(),
    };
    let output: BuildOutput = build(&item, &ctx, &EmbeddedBytes)?;
    warnings.extend(output.warnings.iter().cloned());
    dedupe(&mut warnings);

    write_file(&args.output, output.fragment.as_bytes())?;

    let photos = output
        .processed
        .iter()
        .map(|processed| PhotoLine {
            name: processed.file_name.clone(),
            bytes: Some(processed.bytes.len()),
            quality: processed.quality,
            url: photo_url(
                args.public_base_url.as_ref(),
                &item.slug,
                &processed.file_name,
            ),
            uploaded: None,
        })
        .collect();

    Ok(Report {
        fragment_path: Some(args.output.clone()),
        fragment: None,
        slug: item.slug.clone(),
        dry_run: false,
        remote_folder: None,
        photos,
        warnings,
    })
}

// =============================================================================================
// publish (T063, T064)
// =============================================================================================

fn run_publish(args: &PublishArgs) -> Result<Report, Failure> {
    let mut warnings = Vec::new();
    let item = prepare(&args.source, &mut warnings)?;

    let store = ServerStore::platform(LocalFiles::new())?;
    let server = store.get(&args.server)?.ok_or_else(|| {
        Failure::Refused(Refusal {
            kind: "no_such_server",
            detail: format!(
                "there is no saved server called `{}`. `newsbuilder server list` shows the \
                 ones there are, and `newsbuilder server add` makes a new one.",
                args.server
            ),
        })
    })?;

    let mut transport = SftpTransport::new()?;
    let secrets = KeyringStore::new();
    let mode = if args.dry_run {
        PublishMode::DryRun
    } else {
        PublishMode::Live
    };

    let publication = publish(
        &item,
        &server,
        &mut transport,
        &secrets,
        mode,
        &EmbeddedBytes,
    )?;
    warnings.extend(publication.warnings.iter().cloned());
    dedupe(&mut warnings);

    let fragment_path = match &args.output {
        Some(path) => {
            write_file(path, publication.fragment.as_bytes())?;
            Some(path.clone())
        }
        None => None,
    };

    // Uploaded first, then the files already there — the order a reader cares about.
    let mut photos: Vec<PhotoLine> = publication
        .uploaded
        .iter()
        .map(|(name, url)| PhotoLine {
            name: name.clone(),
            bytes: None,
            quality: None,
            url: url.to_string(),
            uploaded: Some(true),
        })
        .collect();
    photos.extend(publication.unchanged.iter().map(|name| PhotoLine {
        name: name.clone(),
        bytes: None,
        quality: None,
        url: paths::public_url(server.public_base_url.as_str(), &publication.folder, name),
        uploaded: Some(false),
    }));

    Ok(Report {
        // The fragment is the artefact of the whole exercise, so a run that was not asked to
        // write it anywhere still hands it over rather than making the operator build again.
        fragment: fragment_path.is_none().then_some(publication.fragment),
        fragment_path,
        slug: publication.folder.clone(),
        dry_run: publication.dry_run,
        remote_folder: Some(publication.remote_folder.clone()),
        photos,
        warnings,
    })
}

/// Drops repeats, keeping the first of each.
///
/// Import, attachment and build each check the item they were handed, so a placement that
/// never resolves is legitimately noticed more than once. Reporting one problem twice reads as
/// two problems, and under `--strict` it inflates a count the operator is meant to act on.
fn dedupe(warnings: &mut Vec<Warning>) {
    let mut seen: Vec<Warning> = Vec::with_capacity(warnings.len());
    warnings.retain(|warning| {
        if seen.contains(warning) {
            return false;
        }
        seen.push(warning.clone());
        true
    });
}

// =============================================================================================
// server (T063)
// =============================================================================================

fn run_server(command: ServerCommand) -> Result<(), Failure> {
    let store = ServerStore::platform(LocalFiles::new())?;

    match command {
        ServerCommand::List { json } => {
            let configs = store.load()?;
            if json {
                // The credential is a reference, never a value — there is nothing here to
                // redact because there is nothing here to leak (SC-010).
                let document = serde_json::json!({
                    "ok": true,
                    "path": store.path().display().to_string(),
                    "servers": configs.iter().map(|c| serde_json::json!({
                        "name": c.name,
                        "host": c.host,
                        "user": c.user,
                        "port": c.port,
                        "remote_base_path": c.remote_base_path,
                        "public_base_url": c.public_base_url.as_str(),
                        "auth": match &c.credential {
                            CredentialRef::Key { .. } => "key",
                            CredentialRef::Password { .. } => "password",
                        },
                    })).collect::<Vec<_>>(),
                });
                println!("{document}");
            } else if configs.is_empty() {
                println!("no servers configured ({})", store.path().display());
            } else {
                for config in &configs {
                    let auth = match &config.credential {
                        CredentialRef::Key { path, .. } => format!("key {}", path.display()),
                        CredentialRef::Password { .. } => "password".to_owned(),
                    };
                    println!(
                        "{}\t{}@{}:{}\t{}\t{}\t{auth}",
                        config.name,
                        config.user,
                        config.host,
                        config.port,
                        config.remote_base_path,
                        config.public_base_url
                    );
                }
            }
            Ok(())
        }

        ServerCommand::Add(args) => {
            let credential = match &args.key {
                Some(path) => CredentialRef::Key {
                    path: path.clone(),
                    server: args.name.clone(),
                },
                None => CredentialRef::Password {
                    server: args.name.clone(),
                },
            };
            store.save(ServerConfig {
                name: args.name.clone(),
                host: args.host,
                user: args.user,
                port: args.port,
                remote_base_path: args.remote_base_path,
                public_base_url: args.public_base_url,
                credential,
            })?;
            println!("{}", args.name);
            eprintln!(
                "newsbuilder: saved. Store the credential with `newsbuilder server \
                 set-credential {}`.",
                args.name
            );
            Ok(())
        }

        ServerCommand::Remove { name } => {
            let config = store.get(&name)?;
            if !store.remove(&name)? {
                return Err(Failure::Refused(Refusal {
                    kind: "no_such_server",
                    detail: format!("there is no saved server called `{name}`"),
                }));
            }
            // The credential outlives the configuration otherwise, and nothing would ever ask
            // about it again — a secret nobody can reach and nobody can clear.
            if let Some(config) = config {
                let secrets = KeyringStore::new();
                if secrets.available()
                    && let Err(error) = secrets.delete(&config.credential)
                {
                    eprintln!("newsbuilder: warning: the stored credential remains: {error}");
                }
            }
            println!("{name}");
            Ok(())
        }

        ServerCommand::SetCredential { name } => {
            let config = store.get(&name)?.ok_or_else(|| {
                Failure::Refused(Refusal {
                    kind: "no_such_server",
                    detail: format!(
                        "there is no saved server called `{name}`; add it before storing its \
                         credential"
                    ),
                })
            })?;

            let secrets = KeyringStore::new();
            if !secrets.available() {
                return Err(Failure::Core(Error::SecretStoreUnavailable {
                    detail: "no operating system secret store is reachable, and this tool will \
                             not write a credential to a file (FR-041). On a headless Linux \
                             machine, start a Secret Service such as gnome-keyring-daemon."
                        .to_owned(),
                }));
            }

            let secret = read_secret(&config)?;
            secrets.set(&config.credential, &secret)?;
            eprintln!("newsbuilder: stored the credential for `{name}`");
            Ok(())
        }
    }
}

/// Reads a credential without it ever becoming an argument (FR-040).
///
/// A terminal gets a prompt with echo disabled. Anything else — a pipe, a CI job — reads one
/// line from stdin. Either way it never appears in `ps` output or in shell history.
fn read_secret(config: &ServerConfig) -> Result<Secret, Failure> {
    use std::io::IsTerminal;

    let prompt = match &config.credential {
        CredentialRef::Key { path, .. } => {
            format!("Passphrase for {} (empty if none): ", path.display())
        }
        CredentialRef::Password { .. } => {
            format!("Password for {}@{}: ", config.user, config.host)
        }
    };

    let raw = if std::io::stdin().is_terminal() {
        rpassword::prompt_password(&prompt).map_err(|error| Error::Io {
            path: PathBuf::from("<terminal>"),
            source: error,
        })?
    } else {
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line).map_err(|error| {
            Error::Io {
                path: PathBuf::from("<stdin>"),
                source: error,
            }
        })?;
        // A trailing newline is the pipe's, not the operator's.
        line.trim_end_matches(['\r', '\n']).to_owned()
    };

    // An empty password is a mistake; an empty key passphrase means the key has none.
    if raw.is_empty() && matches!(config.credential, CredentialRef::Password { .. }) {
        return Err(Failure::Refused(Refusal {
            kind: "empty_credential",
            detail: "an empty password would be stored and then refused at publish time".to_owned(),
        }));
    }

    Ok(Secret::new(raw))
}

/// Everything both commands do before they diverge: read the document, attach the photos,
/// apply the overrides, and refuse an item that still needs a human.
///
/// One function rather than two, because FR-039 is the promise that `build` and `publish` — and
/// the desktop application — see the same item. Two copies of this would be two places for
/// that to stop being true.
fn prepare(args: &SourceArgs, warnings: &mut Vec<Warning>) -> Result<NewsItem, Failure> {
    let mut item = load_item(args, warnings)?;
    apply_title_and_slug(&mut item, args)?;
    apply_appearance(&mut item, args, warnings)?;
    refuse_if_it_needs_a_decision(&item)?;
    Ok(item)
}

/// Reads the document and, where one was given, the folder of photos its markers refer to.
fn load_item(args: &SourceArgs, warnings: &mut Vec<Warning>) -> Result<NewsItem, Failure> {
    let name = args
        .input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let format = SourceFormat::from_file_name(&name).ok_or_else(|| Error::UnsupportedFormat {
        name: name.clone(),
        detail: "the input must be a .docx, .txt, or .md file".to_owned(),
    })?;

    let bytes = read_file(&args.input)?;
    // Import is handed bytes and never a path, so a failure it reports cannot name the file on
    // its own. This is the code that opened it, so it attaches the name here (SC-007, T101).
    let imported = import_document(&bytes, format).map_err(|error| error.in_file(&name))?;
    let mut item = imported.item;

    match &args.images_dir {
        // Attaching photos is the second half of importing a marker document, so the import's
        // own "this marker has no photo" warnings are about a half-built item and are simply
        // premature. `attach_photos` re-derives them against the finished one and re-issues
        // whichever still hold — keeping both would report every marker in a perfectly good
        // document as broken, and `--strict` would then fail the run.
        Some(dir) => {
            warnings.extend(
                imported
                    .warnings
                    .into_iter()
                    .filter(|warning| !matches!(warning, Warning::MissingPhoto { .. })),
            );
            warnings.extend(attach_photos(&mut item, read_images(dir)?));
        }
        None => warnings.extend(imported.warnings),
    }

    Ok(item)
}

/// The photos of a folder, as bytes.
///
/// Files that are not images are passed over silently rather than warned about, matching the
/// reference's `discover_images`: the folder is a photo folder, and a stray `notes.txt` in it
/// is not something the operator needs telling about. A file that *looks* like an image and
/// will not decode is a different matter, and `attach_photos` warns about that one.
fn read_images(dir: &Path) -> Result<Vec<(String, PhotoSource, Vec<u8>)>, Failure> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut images = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if !newsbuilder_core::photo::extension_is_supported(&name) {
            continue;
        }
        let bytes = read_file(&path)?;
        let source = PhotoSource::Bytes(Arc::from(bytes.clone().into_boxed_slice()));
        images.push((name, source, bytes));
    }

    // `attach_photos` applies the reference's natural sort itself, so the order the filesystem
    // happened to offer never reaches the item (constitution IV).
    Ok(images)
}

/// Applies `--title` and `--news-slug`.
///
/// A title override re-derives the slug, because a slug that still spells the old headline is
/// not what anyone means by overriding the title. An explicit `--news-slug` wins over both.
fn apply_title_and_slug(item: &mut NewsItem, args: &SourceArgs) -> Result<(), Failure> {
    if let Some(title) = &args.title {
        item.title = title.clone();
        item.slug = slugify(title);
    }

    if let Some(wanted) = &args.news_slug {
        item.slug = Slug::parse(wanted).ok_or_else(|| {
            Failure::Usage(format!(
                "--news-slug `{wanted}` is not a slug: it must be lowercase letters, digits \
                 and hyphens, and cannot be empty"
            ))
        })?;
    }

    Ok(())
}

/// Applies a `--appearance` file over the built-in appearance.
fn apply_appearance(
    item: &mut NewsItem,
    args: &SourceArgs,
    warnings: &mut Vec<Warning>,
) -> Result<(), Failure> {
    let Some(path) = &args.appearance else {
        return Ok(());
    };
    let bytes = read_file(path)?;
    let json = String::from_utf8(bytes).map_err(|_| Error::InvalidAppearance {
        detail: format!("`{}` is not valid UTF-8", path.display()),
    })?;
    // `apply` names the field it rejected; only this call knows which of the three appearance
    // layers the field came from, so it adds the file (SC-007, T101).
    let name = path.display().to_string();
    let loaded =
        appearance_config::apply(&item.appearance, &json).map_err(|error| error.in_file(&name))?;
    item.appearance = loaded.appearance;
    warnings.extend(loaded.warnings);
    Ok(())
}

/// The one refusal contracts/cli.md names for `build` (T046).
///
/// An item holding photos that no placement references, and no markers to derive placements
/// from, has not been arranged. Choosing where those photos go is the editor's judgement, so
/// the CLI declines to invent an answer and points at the surface that can ask.
fn refuse_if_it_needs_a_decision(item: &NewsItem) -> Result<(), Failure> {
    if item.photos.is_empty() || !item.placed_photo_ids().is_empty() {
        return Ok(());
    }
    Err(Failure::Refused(Refusal {
        kind: "needs_arrangement",
        detail: format!(
            "this item has {} photo(s) but no placements, and the document has no markers to \
             derive them from. Where the photos go is an editorial decision: open it in the \
             News Builder desktop application, arrange them there, and publish from there — \
             or add [image:N] markers to the document and build again.",
            item.photos.len()
        ),
    }))
}

/// The URL a photo will carry, which is the published one when a public base was given and the
/// preview placeholder otherwise — the same choice `core::build` made for the fragment.
fn photo_url(public_base_url: Option<&Url>, slug: &Slug, file_name: &str) -> String {
    match public_base_url {
        Some(base) => paths::public_url(base.as_str(), slug, file_name),
        None => format!(
            "{}://{slug}/{file_name}",
            newsbuilder_core::build::PREVIEW_URL_SCHEME
        ),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, Failure> {
    std::fs::read(path).map_err(|source| {
        Failure::from(Error::Io {
            path: path.to_path_buf(),
            source,
        })
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}
