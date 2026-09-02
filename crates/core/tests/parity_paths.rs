//! Remote path and public URL parity (T048, FR-027).
//!
//! The capture ran every case through the reference's path builders with five base-path and
//! base-URL pairs — with no trailing slash, with one, with two, relative, and empty — because
//! that is where a hand-written join goes wrong.

mod support;

use newsbuilder_core::model::server::Slug;
use newsbuilder_core::publish::paths;
use pretty_assertions::assert_eq;
use serde_json::Value;

fn variants(case: &str) -> Option<(Slug, Vec<Value>)> {
    let raw = std::fs::read_to_string(support::golden_dir(case).join("paths.json")).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    if value
        .get("news_folder_is_timestamped")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return None;
    }
    let folder = Slug::parse(value.get("news_folder")?.as_str()?)?;
    Some((folder, value.get("variants")?.as_array()?.clone()))
}

#[test]
fn every_captured_path_variant_matches() {
    let mut checked = 0usize;
    let mut mismatches = Vec::new();

    for case in support::all_cases() {
        let Some((folder, variants)) = variants(&case) else {
            continue;
        };
        for variant in variants {
            let base_path = variant["remote_base_path"].as_str().expect("a string");
            let base_url = variant["public_base_url"].as_str().expect("a string");
            let expected_path = variant["remote_path"].as_str().expect("a string");
            let expected_url = variant["public_url_base"].as_str().expect("a string");

            let produced_path = paths::remote_folder(base_path, &folder);
            let produced_url = paths::public_base(base_url, &folder);
            if produced_path != expected_path {
                mismatches.push(format!(
                    "{case}: remote_folder({base_path:?}) expected {expected_path:?}, got {produced_path:?}"
                ));
            }
            if produced_url != expected_url {
                mismatches.push(format!(
                    "{case}: public_base({base_url:?}) expected {expected_url:?}, got {produced_url:?}"
                ));
            }
            checked += 1;
        }
    }

    assert!(
        checked > 20,
        "expected the captured variants to be present, checked {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "path construction diverged:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn published_urls_in_the_fragment_match_the_captured_base() {
    // The join that actually reaches the CMS: base, folder, file name, one slash between each.
    for case in support::fragment_comparable_cases() {
        let Some((folder, _)) = variants(&case) else {
            continue;
        };
        let output = support::build_case(&case).unwrap_or_else(|| panic!("`{case}` should build"));
        for processed in &output.processed {
            let expected =
                paths::public_url(support::PUBLIC_BASE_URL, &folder, &processed.file_name);
            assert!(
                output.fragment.contains(&expected),
                "`{case}` should reference {expected}"
            );
        }
    }
}

#[test]
fn a_file_lands_under_its_item_folder_on_the_server() {
    let folder = Slug::parse("den-konstitutsii").expect("well formed");
    assert_eq!(
        paths::remote_file(
            support::REMOTE_BASE_PATH,
            &folder,
            "den-konstitutsii-01.jpg"
        ),
        "/var/www/html/news/den-konstitutsii/den-konstitutsii-01.jpg"
    );
}
