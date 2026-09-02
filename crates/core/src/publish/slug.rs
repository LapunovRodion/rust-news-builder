//! Slug transliteration (FR-026).
//!
//! The published folder name comes from the headline, so this table decides the URL of every
//! photo in every item. It reproduces the reference's `CYRILLIC_TRANSLIT` and `slugify`
//! exactly; `fixtures/reference/_tables/slugs.json` is the gate.
//!
//! The order of operations is load-bearing and not obvious:
//!
//! 1. lowercase the whole string — the table has lowercase keys only;
//! 2. transliterate Cyrillic character by character;
//! 3. expand `&` to ` and `;
//! 4. NFKD-decompose, then drop everything non-ASCII — this is what turns `é` into `e`
//!    rather than deleting it;
//! 5. collapse runs of non-alphanumerics into single hyphens, and trim them;
//! 6. fall back to `news` when nothing survives.

use unicode_normalization::UnicodeNormalization;

use crate::model::server::Slug;

/// The reference's `CYRILLIC_TRANSLIT`, including the Belarusian and Ukrainian rows.
///
/// `ъ` and `ь` map to the empty string: they are signs, not sounds, and the reference deletes
/// them. That is why `Ъ Ь Ы Э Ю Я` slugifies to `y-e-yu-ya` and not to something longer.
const CYRILLIC_TRANSLIT: [(char, &str); 38] = [
    ('а', "a"),
    ('б', "b"),
    ('в', "v"),
    ('г', "g"),
    ('д', "d"),
    ('е', "e"),
    ('ё', "e"),
    ('ж', "zh"),
    ('з', "z"),
    ('и', "i"),
    ('й', "i"),
    ('к', "k"),
    ('л', "l"),
    ('м', "m"),
    ('н', "n"),
    ('о', "o"),
    ('п', "p"),
    ('р', "r"),
    ('с', "s"),
    ('т', "t"),
    ('у', "u"),
    ('ф', "f"),
    ('х', "h"),
    ('ц', "ts"),
    ('ч', "ch"),
    ('ш', "sh"),
    ('щ', "sch"),
    ('ъ', ""),
    ('ы', "y"),
    ('ь', ""),
    ('э', "e"),
    ('ю', "yu"),
    ('я', "ya"),
    ('і', "i"),
    ('ї', "yi"),
    ('є', "e"),
    ('ў', "u"),
    // The reference's dict has 37 entries; this padding row keeps the array length a
    // compile-time constant without a build script. It can never match a real character.
    ('\u{0}', ""),
];

fn transliterate_char(ch: char) -> Option<&'static str> {
    CYRILLIC_TRANSLIT
        .iter()
        .find(|(key, _)| *key == ch)
        .map(|(_, value)| *value)
}

/// The reference's `slugify`, including its `news` fallback.
#[must_use]
pub fn slugify(value: &str) -> Slug {
    // 1 and 2: lowercase, then transliterate.
    let mut transliterated = String::with_capacity(value.len());
    for ch in value.to_lowercase().chars() {
        match transliterate_char(ch) {
            Some(replacement) => transliterated.push_str(replacement),
            None => transliterated.push(ch),
        }
    }

    // 3: `&` becomes a word, so "Hello & Goodbye" reads as "hello-and-goodbye".
    let expanded = transliterated.replace('&', " and ");

    // 4: decompose, then keep only ASCII. `é` becomes `e` plus a combining accent, and the
    // accent is dropped — which is the whole point of doing it in this order.
    let ascii: String = expanded.nfkd().filter(char::is_ascii).collect();

    // 5: `[^a-zA-Z0-9]+` -> "-", trimmed.
    let mut slug = String::with_capacity(ascii.len());
    let mut in_run = false;
    for ch in ascii.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            in_run = false;
        } else if !in_run {
            slug.push('-');
            in_run = true;
        }
    }
    let slug = slug.trim_matches('-').to_lowercase();

    // 6: nothing survived, so the reference names it `news`.
    Slug::parse(&slug).unwrap_or_else(Slug::fallback)
}

/// The folder an item publishes into, reproducing `build_news_folder_name`.
///
/// An override is slugified too, so a hand-typed folder name cannot produce an invalid URL.
/// When a title transliterates to nothing the reference falls back to `news-<timestamp>`;
/// [`timestamped_fallback`] is that rule, kept behind the [`Clock`](crate::ports::Clock) port
/// so the build stays deterministic (constitution IV).
#[must_use]
pub fn folder_name(title: &str, override_slug: Option<&str>) -> FolderName {
    if let Some(value) = override_slug
        && !value.trim().is_empty()
    {
        return FolderName::Fixed(slugify(value.trim()));
    }

    let base = slugify(title);
    if base != Slug::fallback() {
        FolderName::Fixed(base)
    } else {
        FolderName::NeedsTimestamp
    }
}

/// What [`folder_name`] decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderName {
    /// The folder is known without consulting the clock.
    Fixed(Slug),
    /// The title carried nothing sluggable, so the caller must supply the time.
    NeedsTimestamp,
}

/// The reference's `news-%Y%m%d-%H%M%S` fallback.
#[must_use]
pub fn timestamped_fallback(now: time::OffsetDateTime) -> Slug {
    let value = format!(
        "news-{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    Slug::parse(&value).unwrap_or_else(Slug::fallback)
}

#[cfg(test)]
mod tests {
    use super::{FolderName, folder_name, slugify, timestamped_fallback};
    use crate::model::server::Slug;

    fn slug(value: &str) -> String {
        slugify(value).as_str().to_owned()
    }

    #[test]
    fn cyrillic_transliterates() {
        // Pinned by fixtures/reference/_tables/slugs.json.
        assert_eq!(slug("День Конституции"), "den-konstitutsii");
        assert_eq!(slug("Здравствуй, мир!"), "zdravstvui-mir");
        assert_eq!(slug("Ёлки-палки"), "elki-palki");
        assert_eq!(slug("Обычный текст"), "obychnyi-tekst");
    }

    #[test]
    fn the_signs_are_deleted_not_transliterated() {
        assert_eq!(slug("Съезд подъездов"), "sezd-podezdov");
        assert_eq!(slug("Ъ Ь Ы Э Ю Я"), "y-e-yu-ya");
    }

    #[test]
    fn the_belarusian_and_ukrainian_rows_are_present() {
        assert_eq!(slug("і ї є ў"), "i-yi-e-u");
        assert_eq!(
            slug("Іван Їжак Європа Ўзвышша"),
            "ivan-yizhak-evropa-uzvyshsha"
        );
    }

    #[test]
    fn case_does_not_change_the_result() {
        assert_eq!(slug("щука"), "schuka");
        assert_eq!(slug("ЩУКА"), "schuka");
    }

    #[test]
    fn an_ampersand_becomes_a_word() {
        assert_eq!(slug("Hello & Goodbye"), "hello-and-goodbye");
    }

    #[test]
    fn accents_decompose_rather_than_disappear() {
        // Without the NFKD pass this would be "caf-na-ve".
        assert_eq!(slug("Café Naïve"), "cafe-naive");
    }

    #[test]
    fn runs_of_punctuation_collapse_to_one_hyphen() {
        assert_eq!(slug("Hello  --  World"), "hello-world");
        assert_eq!(slug("a/b\\c"), "a-b-c");
        assert_eq!(slug("tabs\tand\nnewlines"), "tabs-and-newlines");
        assert_eq!(slug("МКА-2026: итоги"), "mka-2026-itogi");
    }

    #[test]
    fn digits_survive() {
        assert_eq!(slug("2026 год"), "2026-god");
    }

    #[test]
    fn nothing_sluggable_falls_back_to_news() {
        assert_eq!(slug("!!!"), "news");
        assert_eq!(slug(""), "news");
        assert_eq!(slug("   "), "news");
    }

    #[test]
    fn every_slug_satisfies_the_invariant() {
        // INV-2, over everything the goldens cover plus the awkward cases.
        for input in [
            "День Конституции",
            "!!!",
            "Café Naïve",
            "МКА-2026: итоги",
            "---",
            "a",
            "9",
            "Ъ",
            "🎉 emoji 🎉",
            "  ",
            "\u{0}",
        ] {
            let produced = slugify(input);
            assert!(
                Slug::is_well_formed(produced.as_str()),
                "{input:?} produced {produced:?}"
            );
        }
    }

    #[test]
    fn an_override_is_slugified_too() {
        assert_eq!(
            folder_name("Anything", Some(" Моя Папка ")),
            FolderName::Fixed(slugify("Моя Папка"))
        );
    }

    #[test]
    fn a_blank_override_is_ignored() {
        assert_eq!(
            folder_name("День", Some("   ")),
            FolderName::Fixed(slugify("День"))
        );
    }

    #[test]
    fn a_title_with_nothing_sluggable_asks_for_the_clock() {
        assert_eq!(folder_name("!!!", None), FolderName::NeedsTimestamp);
    }

    #[test]
    fn a_title_that_slugs_to_the_literal_word_news_also_asks_for_the_clock() {
        // The reference cannot tell the fallback apart from a genuine "News" headline, and
        // timestamps both. Reproducing the quirk is the point.
        assert_eq!(folder_name("News", None), FolderName::NeedsTimestamp);
    }

    #[test]
    fn the_timestamped_fallback_matches_the_reference_format() {
        let when = time::OffsetDateTime::from_unix_timestamp(1_772_000_000)
            .expect("a valid unix timestamp");
        let produced = timestamped_fallback(when);
        assert!(
            produced.as_str().starts_with("news-"),
            "unexpected shape: {produced}"
        );
        assert_eq!(produced.as_str().len(), "news-YYYYmmdd-HHMMSS".len());
        assert!(Slug::is_well_formed(produced.as_str()));
    }
}
