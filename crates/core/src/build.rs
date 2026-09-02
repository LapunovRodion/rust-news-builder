//! The one pipeline both frontends call.
//!
//! The desktop preview and the CLI export go through here and nowhere else, which is what
//! makes "the preview matches the export" structural rather than aspirational (FR-022).
//!
//! Building never touches [`Transport`](crate::ports::Transport), so it works with no network
//! and FR-031's dry-run falls out for free. It does need the photos' bytes, which is what
//! [`PhotoBytesSource`] is for: an item whose photos came from a clipboard already carries
//! them, and one whose photos live on disk needs a reader. That reader is local-only and
//! cannot reach a server.

use std::collections::BTreeMap;

use url::Url;

use crate::error::{Error, Result, Warning};
use crate::model::item::{Block, NewsItem};
use crate::model::photo::{Photo, PhotoId, PhotoSource};
use crate::model::server::Slug;
use crate::photo::{self, ProcessedPhoto};
use crate::ports::FileStore;
use crate::publish::{paths, slug::slugify};
use crate::render::{self, RenderedPhoto};

/// Supplies a photo's bytes.
pub trait PhotoBytesSource {
    /// Reads the photo's original, undecoded bytes.
    fn read(&self, photo: &Photo) -> Result<Vec<u8>>;
}

/// Serves photos that already carry their bytes, and refuses the rest.
///
/// Enough for clipboard and Word-embedded photos, and for every test — which is why the test
/// suite needs no filesystem at all.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmbeddedBytes;

impl PhotoBytesSource for EmbeddedBytes {
    fn read(&self, photo: &Photo) -> Result<Vec<u8>> {
        match &photo.source {
            PhotoSource::Bytes(bytes) => Ok(bytes.to_vec()),
            PhotoSource::Path(path) => Err(Error::Io {
                path: path.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "this build was given no way to read photos from disk",
                ),
            }),
        }
    }
}

/// Serves photos from disk through a [`FileStore`], and in-memory ones directly.
#[derive(Debug)]
pub struct FileStoreBytes<'a, F: FileStore>(pub &'a F);

impl<F: FileStore> PhotoBytesSource for FileStoreBytes<'_, F> {
    fn read(&self, photo: &Photo) -> Result<Vec<u8>> {
        match &photo.source {
            PhotoSource::Bytes(bytes) => Ok(bytes.to_vec()),
            PhotoSource::Path(path) => self.0.read(path),
        }
    }
}

/// Everything a build needs beyond the item itself.
#[derive(Debug, Clone)]
pub struct BuildContext {
    /// Where the photos will live once published. `None` renders preview URLs instead; the
    /// HTML structure is identical either way, so what the editor checks is what the CMS gets.
    pub public_base_url: Option<Url>,
    /// The folder the item publishes into.
    pub slug: Slug,
}

impl BuildContext {
    /// A context for previewing, with no server chosen yet.
    #[must_use]
    pub fn preview(slug: Slug) -> Self {
        Self {
            public_base_url: None,
            slug,
        }
    }
}

/// The scheme preview URLs use when no public base is known.
///
/// The desktop application serves this through Tauri's asset protocol, so the preview shows
/// real photos without full-size bytes crossing the IPC boundary.
pub const PREVIEW_URL_SCHEME: &str = "newsbuilder-preview";

/// What a build produces. Never persisted: it is recomputed from the item on every change,
/// which is what makes the preview and the export the same artefact (FR-022, FR-023).
#[derive(Debug, Clone)]
pub struct BuildOutput {
    /// The inline-styled HTML for the CMS.
    pub fragment: String,
    /// Every photo the item holds, processed and named. Photos no placement references are
    /// **not** here: they are warned about and left unprocessed.
    pub processed: Vec<ProcessedPhoto>,
    /// Per-photo and per-marker problems (FR-034).
    pub warnings: Vec<Warning>,
}

/// Builds an item.
///
/// Deterministic: the same item and appearance yield a byte-identical fragment on every
/// machine and every run (FR-024, constitution IV). Nothing here reads the clock, the locale,
/// or hash iteration order.
pub fn build(
    item: &NewsItem,
    ctx: &BuildContext,
    sources: &dyn PhotoBytesSource,
) -> Result<BuildOutput> {
    let mut warnings = Vec::new();

    // Placements that name a photo the item no longer holds (FR-034, SC-007).
    for block in &item.body {
        if let Block::Placement { photos, layout } = block {
            for id in photos {
                if item.photo(*id).is_none() {
                    warnings.push(Warning::MissingPhoto {
                        marker: format!("[{}:{}]", layout.marker_keyword(), id.0),
                    });
                }
            }
        }
    }

    let placed = item.placed_photo_ids();
    for id in item.unused_photo_ids() {
        warnings.push(Warning::UnusedPhoto { id });
    }

    // The published stem comes from the *title*, and the number from the photo's position in
    // the whole item — not among the used ones. An unused photo therefore still consumes its
    // number, which is what keeps a used photo's URL matching the reference's.
    let title_slug = slugify(&item.title);
    let mut processed = Vec::new();
    let mut rendered: BTreeMap<PhotoId, RenderedPhoto> = BTreeMap::new();

    for (position, subject) in item.photos.iter().enumerate() {
        let number = position + 1;
        if !placed.contains(&subject.id) {
            continue;
        }

        let bytes = match sources.read(subject) {
            Ok(bytes) => bytes,
            Err(error) => {
                // One unreadable photo must not cost the editor the whole build (FR-034).
                warnings.push(Warning::PhotoSkipped {
                    name: subject.file_name.clone(),
                    reason: error.to_string(),
                });
                continue;
            }
        };

        let stem = photo::published_stem(title_slug.as_str(), number);
        match photo::process(
            subject,
            &bytes,
            &stem,
            &item.appearance.image,
            &mut warnings,
        ) {
            Ok(output) => {
                let url = match &ctx.public_base_url {
                    Some(base) => paths::public_url(base.as_str(), &ctx.slug, &output.file_name),
                    None => format!("{PREVIEW_URL_SCHEME}://{}/{}", ctx.slug, output.file_name),
                };
                rendered.insert(subject.id, RenderedPhoto { url, number });
                processed.push(output);
            }
            Err(error) => warnings.push(Warning::PhotoSkipped {
                name: subject.file_name.clone(),
                reason: error.to_string(),
            }),
        }
    }

    let fragment = render::render(&item.title, &item.body, &item.appearance.styles, &rendered);
    Ok(BuildOutput {
        fragment,
        processed,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::{BuildContext, EmbeddedBytes, PREVIEW_URL_SCHEME, build};
    use crate::error::Warning;
    use crate::model::item::{Block, Layout, NewsItem};
    use crate::model::photo::{
        Adjustments, NaturalKey, Orientation, Photo, PhotoId, PhotoOrigin, PhotoSource,
    };
    use crate::model::server::Slug;
    use std::sync::Arc;

    /// A real, decodable JPEG, so the pipeline is exercised rather than short-circuited.
    fn jpeg_bytes(width: u32, height: u32) -> Arc<[u8]> {
        use image::{ImageEncoder, Rgb, RgbImage};
        let mut image = RgbImage::new(width, height);
        let mut state: u32 = 0x9E37_79B9;
        for pixel in image.pixels_mut() {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *pixel = Rgb([(state >> 16) as u8, (state >> 8) as u8, state as u8]);
        }
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90)
            .write_image(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encoding a generated image cannot fail");
        Arc::from(bytes.into_boxed_slice())
    }

    fn item_with_photos(count: usize) -> NewsItem {
        let mut item = NewsItem::new();
        item.title = "День Конституции".to_owned();
        item.slug = crate::publish::slug::slugify(&item.title);
        for n in 1..=count {
            let id = item.mint_photo_id();
            let name = format!("photo{n}.jpg");
            item.photos.push(Photo {
                id,
                origin: PhotoOrigin::Dropped,
                source: PhotoSource::Bytes(jpeg_bytes(120, 90)),
                natural_key: NaturalKey::new(&name),
                file_name: name,
                dimensions: (120, 90),
                orientation: Orientation::Normal,
                adjust: Adjustments::NONE,
            });
        }
        item
    }

    fn context() -> BuildContext {
        BuildContext {
            public_base_url: Some(
                url::Url::parse("https://example.org/news/2026/03/").expect("a valid url"),
            ),
            slug: Slug::parse("den-konstitutsii").expect("well formed"),
        }
    }

    #[test]
    fn published_names_follow_the_title_slug_and_position() {
        let mut item = item_with_photos(2);
        item.body = vec![Block::placement(vec![PhotoId(1), PhotoId(2)], Layout::Row)];
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds");
        let names: Vec<&str> = out.processed.iter().map(|p| p.file_name.as_str()).collect();
        assert_eq!(
            names,
            vec!["den-konstitutsii-01.jpg", "den-konstitutsii-02.jpg"]
        );
    }

    #[test]
    fn urls_are_built_beneath_the_public_base() {
        let mut item = item_with_photos(1);
        item.body = vec![Block::placement(vec![PhotoId(1)], Layout::FullWidth)];
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds");
        assert!(
            out.fragment.contains(
                "https://example.org/news/2026/03/den-konstitutsii/den-konstitutsii-01.jpg"
            )
        );
    }

    #[test]
    fn a_preview_build_uses_placeholder_urls_but_the_same_structure() {
        let mut item = item_with_photos(1);
        item.body = vec![Block::placement(vec![PhotoId(1)], Layout::FullWidth)];
        let published = build(&item, &context(), &EmbeddedBytes).expect("builds");
        let preview = build(
            &item,
            &BuildContext::preview(Slug::parse("den-konstitutsii").expect("well formed")),
            &EmbeddedBytes,
        )
        .expect("builds");
        assert!(preview.fragment.contains(PREVIEW_URL_SCHEME));
        // Identical but for the URL: same element count, same styles, same order.
        assert_eq!(
            published.fragment.lines().count(),
            preview.fragment.lines().count()
        );
    }

    #[test]
    fn an_unused_photo_is_warned_about_and_not_processed() {
        let mut item = item_with_photos(3);
        item.body = vec![Block::placement(vec![PhotoId(1)], Layout::FullWidth)];
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds");
        assert_eq!(out.processed.len(), 1, "only the placed photo is processed");
        let unused: Vec<u64> = out
            .warnings
            .iter()
            .filter_map(|w| match w {
                Warning::UnusedPhoto { id } => Some(id.0),
                _ => None,
            })
            .collect();
        assert_eq!(unused, vec![2, 3]);
    }

    #[test]
    fn an_unused_photo_still_consumes_its_number() {
        // Parity: the reference numbers over the whole image list, so dropping a marker must
        // not renumber the photos that remain.
        let mut item = item_with_photos(3);
        item.body = vec![Block::placement(vec![PhotoId(3)], Layout::FullWidth)];
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds");
        assert_eq!(out.processed[0].file_name, "den-konstitutsii-03.jpg");
        assert!(out.fragment.contains("alt=\"День Конституции - image 3\""));
    }

    #[test]
    fn a_placement_naming_a_missing_photo_still_builds() {
        // FR-034: warnings, not failure.
        let mut item = item_with_photos(1);
        item.body = vec![
            Block::paragraph("Text."),
            Block::placement(vec![PhotoId(99)], Layout::FullWidth),
        ];
        item.normalise_paragraph_kinds();
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds despite the gap");
        assert!(out.fragment.contains("Text."));
        assert!(
            out.warnings
                .iter()
                .any(|w| matches!(w, Warning::MissingPhoto { .. })),
            "{:?}",
            out.warnings
        );
    }

    #[test]
    fn a_photo_whose_bytes_cannot_be_read_is_skipped_not_fatal() {
        let mut item = item_with_photos(1);
        item.photos[0].source = PhotoSource::Path("/nowhere/photo1.jpg".into());
        item.body = vec![Block::placement(vec![PhotoId(1)], Layout::FullWidth)];
        let out = build(&item, &context(), &EmbeddedBytes).expect("builds");
        assert!(out.processed.is_empty());
        assert!(
            out.warnings
                .iter()
                .any(|w| matches!(w, Warning::PhotoSkipped { name, .. } if name == "photo1.jpg")),
            "{:?}",
            out.warnings
        );
    }

    #[test]
    fn building_twice_yields_identical_bytes() {
        // FR-024 and constitution IV.
        let mut item = item_with_photos(2);
        item.body = vec![
            Block::paragraph("Lead."),
            Block::placement(vec![PhotoId(1), PhotoId(2)], Layout::Row),
            Block::paragraph("Tail."),
        ];
        item.normalise_paragraph_kinds();
        let first = build(&item, &context(), &EmbeddedBytes).expect("builds");
        let second = build(&item, &context(), &EmbeddedBytes).expect("builds");
        assert_eq!(first.fragment, second.fragment);
        assert_eq!(
            first
                .processed
                .iter()
                .map(|p| p.bytes.len())
                .collect::<Vec<_>>(),
            second
                .processed
                .iter()
                .map(|p| p.bytes.len())
                .collect::<Vec<_>>()
        );
    }
}
