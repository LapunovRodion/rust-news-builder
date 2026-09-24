//! 002 T037 — `newsbuilder server site …` and the publish flags that need no network.
//!
//! Each test gets its own configuration directory through `XDG_CONFIG_HOME`, which is where the
//! `directories` crate puts `servers.json` on Linux. Elsewhere the variable is ignored and the
//! tests would touch the real configuration, so they are Linux only.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Sandbox {
    config: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let config = std::env::temp_dir().join(format!(
            "newsbuilder-site-cli-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&config);
        std::fs::create_dir_all(&config).expect("a temporary directory");
        Self { config }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_newsbuilder"))
            .args(args)
            .env("XDG_CONFIG_HOME", &self.config)
            .env_remove("RUST_LOG")
            .output()
            .expect("the newsbuilder binary runs")
    }

    fn servers(&self) -> serde_json::Value {
        let output = self.run(&["server", "list", "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        serde_json::from_slice(&output.stdout).expect("one JSON document")
    }

    fn add_server(&self) {
        let output = self.run(&[
            "server",
            "add",
            "--name",
            "newsroom",
            "--host",
            "news.example.org",
            "--user",
            "editor",
            "--remote-base-path",
            "/var/www/html/images/news",
            "--public-base-url",
            "https://example.org/images/news/",
        ]);
        assert!(output.status.success(), "{}", stderr(&output));
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.config);
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn fixture(path: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/inputs/markers")
        .join(path)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn turning_insertion_on_needs_the_root_the_address_and_a_category() {
    let sandbox = Sandbox::new("first-time");
    sandbox.add_server();

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--joomla-root",
        "/var/www/html",
    ]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--category"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        sandbox.servers()["servers"][0]["site"],
        serde_json::Value::Null
    );
}

#[test]
fn site_settings_are_set_changed_one_flag_at_a_time_and_disabled() {
    let sandbox = Sandbox::new("lifecycle");
    sandbox.add_server();

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--joomla-root",
        "/var/www/html",
        "--site-url",
        "https://example.org/",
        "--category",
        "8",
        "--language",
        "ru-RU",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    let site = &sandbox.servers()["servers"][0]["site"];
    assert_eq!(site["joomla_root"], "/var/www/html");
    assert_eq!(site["php"], "php");
    assert_eq!(site["defaults"]["category"], 8);
    assert_eq!(site["defaults"]["language"], "ru-RU");

    // A later call changes only what it names.
    let output = sandbox.run(&["server", "site", "set", "newsroom", "--featured"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let site = &sandbox.servers()["servers"][0]["site"];
    assert_eq!(site["defaults"]["featured"], true);
    assert_eq!(site["defaults"]["category"], 8);
    assert_eq!(site["defaults"]["language"], "ru-RU");

    // Re-adding the server keeps its site settings.
    sandbox.add_server();
    assert_eq!(
        sandbox.servers()["servers"][0]["site"]["defaults"]["category"],
        8
    );

    let output = sandbox.run(&["server", "site", "disable", "newsroom"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        sandbox.servers()["servers"][0]["site"],
        serde_json::Value::Null
    );
}

#[test]
fn a_relative_joomla_directory_is_refused() {
    let sandbox = Sandbox::new("relative");
    sandbox.add_server();

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--joomla-root",
        "public_html",
        "--site-url",
        "https://example.org/",
        "--category",
        "8",
    ]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("joomla_root"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn checking_a_server_without_a_site_is_refused_by_name_without_connecting() {
    let sandbox = Sandbox::new("check");
    sandbox.add_server();

    let output = sandbox.run(&["server", "site", "check", "newsroom", "--json"]);

    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document");
    assert_eq!(document["ok"], false);
    assert_eq!(document["error"]["kind"], "site_incomplete");
    assert!(stderr(&output).contains("newsroom"));
}

#[test]
fn article_settings_for_a_server_without_a_site_are_a_usage_error() {
    let sandbox = Sandbox::new("no-site");
    sandbox.add_server();

    let output = sandbox.run(&[
        "publish",
        "--input",
        &fixture("input.txt"),
        "--images-dir",
        &fixture("images"),
        "--server",
        "newsroom",
        "--category",
        "8",
        "--dry-run",
    ]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("newsroom"), "{}", stderr(&output));
}

#[test]
fn an_invalid_date_is_a_usage_error_naming_the_flag() {
    let sandbox = Sandbox::new("bad-date");
    sandbox.add_server();

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--joomla-root",
        "/var/www/html",
        "--site-url",
        "https://example.org/",
        "--category",
        "8",
        "--publish-up",
        "tomorrow",
    ]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("--publish-up"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn server_tags_are_set_by_repeating_the_flag_and_cleared_with_no_tags() {
    let sandbox = Sandbox::new("tags");
    sandbox.add_server();

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--joomla-root",
        "/var/www/html",
        "--site-url",
        "https://example.org/",
        "--category",
        "8",
        "--tag",
        "5",
        "--tag",
        "3",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        sandbox.servers()["servers"][0]["site"]["defaults"]["tags"],
        serde_json::json!([3, 5])
    );

    let output = sandbox.run(&["server", "site", "set", "newsroom", "--no-tags"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        sandbox.servers()["servers"][0]["site"]["defaults"]["tags"],
        serde_json::json!([])
    );

    let output = sandbox.run(&[
        "server",
        "site",
        "set",
        "newsroom",
        "--tag",
        "3",
        "--no-tags",
    ]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the two contradict each other"
    );
}
