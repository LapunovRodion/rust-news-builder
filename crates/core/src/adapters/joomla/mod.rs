//! The Joomla bridge: what runs on the server, how it is started, and what it says back
//! (002 research R1, R2; contracts/bridge-protocol.md).
//!
//! The bridge is a PHP program sent on stdin to the server's `php`, which boots Joomla and saves
//! the article through Joomla's own article model. This module holds everything about it that
//! is pure — the command line, the stdin framing, the request documents and the reading of the
//! answer — so all of it is tested without a server and without the `sftp` feature. The one
//! impure step, running the command, lives beside the SFTP code in [`super::transport`].
//!
//! The command is a constant with exactly two variable parts, the Joomla directory and the PHP
//! command, and both go through [`sh_quote`]. Nothing an editor types reaches the shell: the
//! title and the article text travel inside the JSON on stdin.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::model::site::{ArticleProbe, ArticleWrite, SiteTarget};

/// The program run inside Joomla.
pub const BRIDGE: &str = include_str!("bridge.php");

/// The protocol version every request carries.
pub const PROTOCOL: u32 = 1;

/// The prefix of the one stdout line that carries the answer. Anything else on stdout — a PHP
/// notice, an extension that echoes — is ignored.
pub const SENTINEL: &str = "NEWSBUILDER-BRIDGE:";

/// What PHP runs from its command line: read a length, evaluate that many bytes of stdin as the
/// bridge, and leave the rest of stdin — the request — for the bridge to read.
const LOADER: &str = r#"$n=(int)fgets(STDIN);eval("?>".stream_get_contents(STDIN,$n));"#;

/// How much of the server's error output a failure keeps. Enough for a PHP fatal error and its
/// location, not enough to turn a message into a log dump.
const TAIL_BYTES: usize = 2048;

/// The step names a failure reports (FR-015, spec edge cases).
pub mod step {
    /// `describe`.
    pub const DESCRIBE: &str = "reading the site";
    /// `find`.
    pub const FIND: &str = "reading articles";
    /// `save`.
    pub const SAVE: &str = "saving the article";
    /// Joomla itself did not come up.
    pub const BOOT: &str = "starting Joomla";
}

/// Quotes one word for a POSIX shell.
#[must_use]
pub fn sh_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// The command the exec channel runs.
#[must_use]
pub fn command(target: &SiteTarget) -> String {
    format!(
        "cd -- {} && {} -r {}",
        sh_quote(&target.joomla_root),
        sh_quote(&target.php),
        sh_quote(LOADER)
    )
}

/// Everything written to the command's stdin: the bridge's length, the bridge, the request.
#[must_use]
pub fn frame(request: &Value) -> Vec<u8> {
    let mut bytes = format!("{}\n", BRIDGE.len()).into_bytes();
    bytes.extend_from_slice(BRIDGE.as_bytes());
    bytes.extend_from_slice(request.to_string().as_bytes());
    bytes
}

/// The `describe` request.
#[must_use]
pub fn describe_request(_target: &SiteTarget) -> Value {
    json!({ "protocol": PROTOCOL, "op": "describe" })
}

/// The `find` request.
#[must_use]
pub fn find_request(target: &SiteTarget, probe: &ArticleProbe) -> Value {
    with_cover(
        json!({
            "protocol": PROTOCOL,
            "op": "find",
            "site_url": target.site_url.as_str(),
            "alias": probe.alias,
            "category": probe.category,
            "title": probe.title,
            "articletext": probe.articletext,
            "settings": probe.settings,
        }),
        probe.intro_image.as_deref(),
    )
}

/// The `save` request. Settings nobody chose are absent, not `null`, so Joomla defaults them
/// (FR-012).
#[must_use]
pub fn save_request(target: &SiteTarget, write: &ArticleWrite) -> Value {
    with_cover(
        json!({
            "protocol": PROTOCOL,
            "op": "save",
            "site_url": target.site_url.as_str(),
            "id": write.id,
            "restore": write.restore,
            "alias": write.alias,
            "title": write.title,
            "articletext": write.articletext,
            "settings": write.settings,
        }),
        write.intro_image.as_deref(),
    )
}

/// Adds `intro_image` only when there is something to say about it, so an absent key keeps
/// meaning "leave the cover alone".
fn with_cover(mut request: Value, cover: Option<&str>) -> Value {
    if let (Some(cover), Some(object)) = (cover, request.as_object_mut()) {
        object.insert("intro_image".to_owned(), cover.into());
    }
    request
}

/// Reads the bridge's answer out of everything the command printed.
///
/// `status` is the exit status, when the server sent one.
pub fn parse<T: DeserializeOwned>(
    step: &str,
    stdout: &[u8],
    stderr: &[u8],
    status: Option<u32>,
) -> Result<T> {
    let out = String::from_utf8_lossy(stdout);
    let Some(line) = out
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(SENTINEL))
    else {
        // PHP's command line prints fatal errors to stdout, so when stderr is silent the reason
        // is usually in stdout.
        let said = if stderr.iter().any(|b| !b.is_ascii_whitespace()) {
            tail(stderr)
        } else {
            tail(stdout)
        };
        let status = status.map_or_else(|| "no exit status".to_owned(), |s| format!("exit {s}"));
        return Err(Error::SiteBridge {
            step: step.to_owned(),
            detail: format!("the server gave no answer ({status}): {said}"),
        });
    };

    let value: Value = serde_json::from_str(line).map_err(|e| Error::SiteBridge {
        step: step.to_owned(),
        detail: format!("the answer is not valid JSON: {e}"),
    })?;

    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(bridge_error(step, value.get("error")));
    }

    serde_json::from_value(value).map_err(|e| Error::SiteBridge {
        step: step.to_owned(),
        detail: format!("the answer does not have the expected shape: {e}"),
    })
}

/// Maps a `{ "kind", "detail", … }` object to the error it stands for.
fn bridge_error(step: &str, error: Option<&Value>) -> Error {
    let field = |name: &str| error.and_then(|e| e.get(name));
    let kind = field("kind").and_then(Value::as_str).unwrap_or("unknown");
    let detail = field("detail")
        .and_then(Value::as_str)
        .unwrap_or("the bridge reported a failure without saying why")
        .to_owned();

    match kind {
        "unsupported_joomla" => Error::SiteUnsupported {
            found: field("found")
                .and_then(Value::as_str)
                .unwrap_or("an unknown version")
                .to_owned(),
        },
        "category_missing" => match field("id")
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
        {
            Some(id) => Error::CategoryMissing { id },
            None => Error::SiteBridge {
                step: step.to_owned(),
                detail,
            },
        },
        "boot_failed" => Error::SiteBridge {
            step: step::BOOT.to_owned(),
            detail,
        },
        _ => Error::SiteBridge {
            step: step.to_owned(),
            detail: format!("{kind}: {detail}"),
        },
    }
}

/// The last [`TAIL_BYTES`] of some output, trimmed, as text.
fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(TAIL_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::site::{ArticleSettings, SavedArticle};

    fn target(root: &str, php: &str) -> SiteTarget {
        SiteTarget {
            joomla_root: root.to_owned(),
            site_url: url::Url::parse("https://example.org/").unwrap(),
            php: php.to_owned(),
            defaults: ArticleSettings::default(),
        }
    }

    #[test]
    fn plain_words_are_quoted_whole() {
        assert_eq!(sh_quote("/var/www/html"), "'/var/www/html'");
        assert_eq!(sh_quote("with space"), "'with space'");
    }

    #[test]
    fn a_quote_cannot_close_the_quoting() {
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote("x'; rm -rf / #"), r"'x'\''; rm -rf / #'");
    }

    #[test]
    fn shell_expansion_stays_literal() {
        assert_eq!(sh_quote("$(reboot)`id`"), "'$(reboot)`id`'");
    }

    #[test]
    fn the_command_is_the_constant_with_both_parts_quoted() {
        assert_eq!(
            command(&target("/srv/my site", "/usr/bin/php8.2")),
            r#"cd -- '/srv/my site' && '/usr/bin/php8.2' -r '$n=(int)fgets(STDIN);eval("?>".stream_get_contents(STDIN,$n));'"#
        );
    }

    #[test]
    fn the_frame_is_length_then_bridge_then_request() {
        let request = json!({ "op": "describe" });
        let framed = frame(&request);
        let expected = format!("{}\n{BRIDGE}{request}", BRIDGE.len());
        assert_eq!(String::from_utf8(framed).unwrap(), expected);
    }

    #[test]
    fn the_bridge_is_a_php_program() {
        assert!(BRIDGE.starts_with("<?php"));
    }

    #[test]
    fn the_answer_is_found_among_noise() {
        let stdout = format!(
            "Deprecated: something\n{SENTINEL}{}\ntrailing chatter\n",
            json!({ "ok": true, "id": 7, "created": false, "url": "u" })
        );
        let saved: SavedArticle = parse(step::SAVE, stdout.as_bytes(), b"", Some(0)).unwrap();
        assert_eq!(saved.id, 7);
        assert!(!saved.created);
    }

    #[test]
    fn no_answer_names_the_step_the_status_and_what_php_said() {
        let result: Result<SavedArticle> = parse(
            step::SAVE,
            b"",
            b"PHP Fatal error: Class not found in /srv/x.php:3",
            Some(255),
        );
        match result {
            Err(Error::SiteBridge { step, detail }) => {
                assert_eq!(step, step::SAVE);
                assert!(detail.contains("exit 255"), "{detail}");
                assert!(detail.contains("Class not found"), "{detail}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn with_a_silent_stderr_the_reason_comes_from_stdout() {
        let result: Result<SavedArticle> = parse(
            step::FIND,
            b"PHP Fatal error: Allowed memory size exhausted",
            b"",
            Some(255),
        );
        assert!(
            matches!(&result, Err(Error::SiteBridge { detail, .. }) if detail.contains("memory size")),
            "{result:?}"
        );
    }

    #[test]
    fn only_the_tail_of_a_long_error_is_kept() {
        let noise = "x".repeat(10_000) + "THE END";
        let result: Result<SavedArticle> = parse(step::FIND, b"", noise.as_bytes(), None);
        match result {
            Err(Error::SiteBridge { detail, .. }) => {
                assert!(detail.ends_with("THE END"));
                assert!(detail.len() < 2200, "{}", detail.len());
                assert!(detail.contains("no exit status"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_failed_boot_is_reported_as_starting_joomla() {
        let stdout = format!(
            "{SENTINEL}{}",
            json!({ "ok": false, "error": { "kind": "boot_failed", "detail": "no configuration.php" } })
        );
        let result: Result<SavedArticle> = parse(step::SAVE, stdout.as_bytes(), b"", Some(1));
        assert!(
            matches!(&result, Err(Error::SiteBridge { step, detail }) if step == step::BOOT && detail.contains("configuration.php")),
            "{result:?}"
        );
    }

    #[test]
    fn an_unknown_failure_keeps_its_kind_in_the_message() {
        let stdout = format!(
            "{SENTINEL}{}",
            json!({ "ok": false, "error": { "kind": "save_rejected", "detail": "Save failed with the following error: alias" } })
        );
        let result: Result<SavedArticle> = parse(step::SAVE, stdout.as_bytes(), b"", Some(0));
        assert!(
            matches!(&result, Err(Error::SiteBridge { detail, .. }) if detail.starts_with("save_rejected:")),
            "{result:?}"
        );
    }
}
