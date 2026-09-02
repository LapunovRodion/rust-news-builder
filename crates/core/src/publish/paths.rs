//! Remote paths and public URLs (FR-027).
//!
//! Two rules, both reproducing the reference, both pinned by the `paths.json` goldens:
//!
//! - the remote folder is `remote_base_path` with its trailing slashes removed, then `/`, then
//!   the item's folder;
//! - the public base is `public_base_url` with its trailing slashes removed, then `/`, then
//!   the item's folder, then a final `/`.
//!
//! Exactly one slash joins each pair, however many the configuration carried.

use crate::model::server::Slug;

/// Trailing slashes removed. `"/a/b//"` becomes `"/a/b"`; `"/"` and `"//"` become `""`.
fn trim_trailing_slashes(value: &str) -> &str {
    value.trim_end_matches('/')
}

/// The remote directory an item publishes into.
///
/// Reproduces `build_news_remote_path` composed with `posix_join`: an empty base yields the
/// folder alone, which is a relative path the SFTP session resolves against the login
/// directory.
#[must_use]
pub fn remote_folder(remote_base_path: &str, folder: &Slug) -> String {
    let base = trim_trailing_slashes(remote_base_path);
    if base.is_empty() {
        folder.to_string()
    } else {
        format!("{base}/{folder}")
    }
}

/// The full remote path of one published file.
#[must_use]
pub fn remote_file(remote_base_path: &str, folder: &Slug, file_name: &str) -> String {
    let directory = remote_folder(remote_base_path, folder);
    if directory.is_empty() {
        file_name.to_owned()
    } else {
        format!("{directory}/{file_name}")
    }
}

/// The public URL base for an item, always ending in a slash.
///
/// Reproduces `build_news_public_base_url`. The reference reaches this through `urljoin`, but
/// because the folder is a bare relative segment the result is a plain concatenation — which
/// the `paths.json` goldens confirm across bases with none, one, and two trailing slashes.
#[must_use]
pub fn public_base(public_base_url: &str, folder: &Slug) -> String {
    format!("{}/{folder}/", trim_trailing_slashes(public_base_url))
}

/// The public URL of one published file.
#[must_use]
pub fn public_url(public_base_url: &str, folder: &Slug, file_name: &str) -> String {
    format!("{}{file_name}", public_base(public_base_url, folder))
}

#[cfg(test)]
mod tests {
    use super::{public_base, public_url, remote_file, remote_folder};
    use crate::model::server::Slug;

    fn folder() -> Slug {
        Slug::parse("den-konstitutsii-respubliki-belarus").expect("a well-formed slug")
    }

    #[test]
    fn a_base_without_a_trailing_slash_gains_exactly_one() {
        assert_eq!(
            remote_folder("/var/www/html/news", &folder()),
            "/var/www/html/news/den-konstitutsii-respubliki-belarus"
        );
        assert_eq!(
            public_base("https://example.org/news/2026/03", &folder()),
            "https://example.org/news/2026/03/den-konstitutsii-respubliki-belarus/"
        );
    }

    #[test]
    fn trailing_slashes_do_not_accumulate() {
        // All three bases are pinned by the paths.json goldens.
        for base in [
            "/var/www/html/news",
            "/var/www/html/news/",
            "/var/www/html/news//",
        ] {
            assert_eq!(
                remote_folder(base, &folder()),
                "/var/www/html/news/den-konstitutsii-respubliki-belarus",
                "base {base:?}"
            );
        }
        for base in [
            "https://example.org/news/2026/03",
            "https://example.org/news/2026/03/",
            "https://example.org/news/2026/03//",
        ] {
            assert_eq!(
                public_base(base, &folder()),
                "https://example.org/news/2026/03/den-konstitutsii-respubliki-belarus/",
                "base {base:?}"
            );
        }
    }

    #[test]
    fn a_relative_remote_base_stays_relative() {
        assert_eq!(
            remote_folder("news", &folder()),
            "news/den-konstitutsii-respubliki-belarus"
        );
    }

    #[test]
    fn an_empty_remote_base_yields_the_folder_alone() {
        assert_eq!(
            remote_folder("", &folder()),
            "den-konstitutsii-respubliki-belarus"
        );
        assert_eq!(
            remote_folder("/", &folder()),
            "den-konstitutsii-respubliki-belarus"
        );
    }

    #[test]
    fn a_bare_host_base_url_still_gets_one_slash() {
        assert_eq!(
            public_base("https://example.org/", &folder()),
            "https://example.org/den-konstitutsii-respubliki-belarus/"
        );
    }

    #[test]
    fn file_paths_and_urls_join_with_one_slash() {
        assert_eq!(
            remote_file("/var/www/html/news/", &folder(), "x-01.jpg"),
            "/var/www/html/news/den-konstitutsii-respubliki-belarus/x-01.jpg"
        );
        assert_eq!(
            public_url("https://example.org/news/2026/03", &folder(), "x-01.jpg"),
            "https://example.org/news/2026/03/den-konstitutsii-respubliki-belarus/x-01.jpg"
        );
    }
}
