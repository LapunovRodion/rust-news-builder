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

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use url::Url;

use crate::error::{Error, Result, Warning};
use crate::model::appearance::ImageBudget;
use crate::model::item::{Block, NewsItem};
use crate::model::photo::{Adjustments, Orientation, Photo, PhotoId, PhotoSource};
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

// =============================================================================================
// The rebuild cache (T102, SC-008)
// =============================================================================================

/// Everything about a photo that decides its published bytes.
///
/// Deliberately *not* the published stem: the stem comes from the title and the photo's
/// position, and changes neither the pixels nor the encoder's choices — only the file's name.
/// Leaving it out is what keeps retitling an item as cheap as editing a paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ProcessKey {
    id: PhotoId,
    /// The *source* name, which decides the output container and so the extension.
    source_name: String,
    orientation: Orientation,
    adjust: Adjustments,
    budget: ImageBudget,
}

/// One photo, encoded, with the parts of the result that survive a rename.
#[derive(Debug, Clone)]
struct Encoded {
    /// Shared, so a rebuild copies a pointer per photo rather than half a megabyte.
    bytes: Arc<[u8]>,
    /// The published extension, including the dot. Decided by the container, which is why a
    /// `.gif` source comes back `.jpg`.
    extension: String,
    dimensions: (u32, u32),
    quality: Option<u8>,
    /// `Some(floor)` when the quality search hit its floor with the photo still over budget,
    /// so the warning can be re-issued on a cache hit rather than quietly disappearing.
    over_budget_floor: Option<u8>,
}

/// Encoded photos kept between rebuilds, so a text edit re-encodes nothing (SC-008, T102).
///
/// Processing a photo is a decode, a resize, and a quality-ladder search — tens of milliseconds
/// each, and a thirty-photo item does thirty of them. The preview rebuilds on every keystroke,
/// so without this every keystroke pays for all thirty. With it, an edit that touched only text
/// pays for none: the fragment is re-rendered from the cached bytes, which is microseconds.
///
/// **Keyed by [`PhotoId`], which is only unique within one item.** A cache held across a change
/// of item must be [`cleared`](ProcessCache::clear) — `SessionState::replace` in the desktop
/// crate is where that happens. [`build`] sidesteps the question entirely by using a fresh one
/// per call.
#[derive(Debug, Default)]
pub struct ProcessCache {
    entries: HashMap<ProcessKey, Encoded>,
    encoded: usize,
    reused: usize,
}

impl ProcessCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets everything. Required whenever the item changes identity.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.encoded = 0;
        self.reused = 0;
    }

    /// How many photos the last build had to encode. Zero after a text-only edit is the
    /// property SC-008 rests on, and is what `preview_latency.rs` asserts.
    #[must_use]
    pub fn encoded_last_build(&self) -> usize {
        self.encoded
    }

    /// How many photos the last build served from here.
    #[must_use]
    pub fn reused_last_build(&self) -> usize {
        self.reused
    }
}

/// One placed photo and what the build has decided about it, before any encoding happens.
#[derive(Debug)]
struct PhotoPlan<'a> {
    subject: &'a Photo,
    /// Its position in the whole item, 1-based — including the photos no placement uses.
    number: usize,
    /// The published name without its extension.
    stem: String,
    key: ProcessKey,
    /// Present when the cache already holds this exact encode.
    cached: Option<Encoded>,
    /// Present when the photo's bytes could not be read, so it is reported and passed over.
    skipped: Option<Warning>,
}

/// What one photo's encode produced: the entry to cache, and anything it wanted to say that is
/// *not* the size-budget warning (that one is a property of the encode and is replayed instead).
type EncodeResult = Result<(Encoded, Vec<Warning>)>;

/// Encodes every photo that missed the cache, in parallel, keyed by its index in the plan.
///
/// `std::thread::scope` rather than a thread-pool crate: the work is one batch of independent
/// jobs at one point in the program, so there is nothing for a pool to amortise, and a domain
/// crate that spawns a global pool on its users' behalf takes a decision that is theirs.
///
/// Falls back to doing the work on this thread for a single photo, where a thread costs more
/// than it saves.
fn encode_all(
    plan: &[PhotoPlan<'_>],
    jobs: Vec<(usize, Vec<u8>)>,
    budget: &ImageBudget,
) -> BTreeMap<usize, EncodeResult> {
    if jobs.len() <= 1 {
        return jobs
            .into_iter()
            .map(|(index, bytes)| (index, encode_one(&plan[index], &bytes, budget)))
            .collect();
    }

    let lanes = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(jobs.len());

    // Round-robin rather than contiguous chunks: photos next to each other in an item tend to
    // be the same size, so contiguous chunks would leave one lane with all the big ones.
    let mut lane_jobs: Vec<Vec<(usize, Vec<u8>)>> = (0..lanes).map(|_| Vec::new()).collect();
    for (nth, job) in jobs.into_iter().enumerate() {
        lane_jobs[nth % lanes].push(job);
    }

    let mut done: BTreeMap<usize, EncodeResult> = BTreeMap::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = lane_jobs
            .into_iter()
            .map(|lane| {
                scope.spawn(move || {
                    lane.into_iter()
                        .map(|(index, bytes)| (index, encode_one(&plan[index], &bytes, budget)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            // A panicking lane would be a bug in the encoder, not an input problem. Losing that
            // photo's entry leaves it out of the map, which pass 4 already handles.
            if let Ok(results) = handle.join() {
                done.extend(results);
            }
        }
    });
    done
}

/// One photo: decode, adjust, scale, and walk the quality ladder.
fn encode_one(plan: &PhotoPlan<'_>, bytes: &[u8], budget: &ImageBudget) -> EncodeResult {
    let mut encode_warnings = Vec::new();
    let output = photo::process(
        plan.subject,
        bytes,
        &plan.stem,
        budget,
        &mut encode_warnings,
    )?;

    let over_budget_floor = encode_warnings.iter().find_map(|warning| match warning {
        Warning::SizeBudgetUnreachable { floor_quality, .. } => Some(*floor_quality),
        _ => None,
    });
    // Anything else `process` had to say is about the photo rather than about this encode, so
    // it is reported once and not replayed on a later cache hit.
    let rest = encode_warnings
        .into_iter()
        .filter(|warning| !matches!(warning, Warning::SizeBudgetUnreachable { .. }))
        .collect();

    Ok((
        Encoded {
            extension: output.file_name[plan.stem.len()..].to_owned(),
            bytes: Arc::from(output.bytes.into_boxed_slice()),
            dimensions: output.dimensions,
            quality: output.quality,
            over_budget_floor,
        },
        rest,
    ))
}

/// Builds an item.
///
/// Deterministic: the same item and appearance yield a byte-identical fragment on every
/// machine and every run (FR-024, constitution IV). Nothing here reads the clock, the locale,
/// or hash iteration order.
///
/// Every photo is encoded from scratch. That is what a one-shot caller — the CLI, a publish —
/// wants; a caller that rebuilds repeatedly wants [`build_with_cache`] instead.
pub fn build(
    item: &NewsItem,
    ctx: &BuildContext,
    sources: &dyn PhotoBytesSource,
) -> Result<BuildOutput> {
    build_with_cache(item, ctx, sources, &mut ProcessCache::new())
}

/// Builds an item, re-encoding only the photos whose bytes would actually differ (T102).
///
/// Byte-for-byte identical to [`build`] for the same item — the cache changes what is
/// recomputed, never what is produced. `preview_latency.rs` asserts that equivalence directly,
/// because a cache that quietly changed the output would break FR-024 and the parity suite
/// would not see it.
pub fn build_with_cache(
    item: &NewsItem,
    ctx: &BuildContext,
    sources: &dyn PhotoBytesSource,
    cache: &mut ProcessCache,
) -> Result<BuildOutput> {
    let mut warnings = Vec::new();
    cache.encoded = 0;
    cache.reused = 0;

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

    // Pass 1 — decide, in item order, what each placed photo needs. Nothing expensive here.
    let mut plan: Vec<PhotoPlan<'_>> = Vec::new();
    for (position, subject) in item.photos.iter().enumerate() {
        let number = position + 1;
        if !placed.contains(&subject.id) {
            continue;
        }

        let stem = photo::published_stem(title_slug.as_str(), number);
        let key = ProcessKey {
            id: subject.id,
            source_name: subject.file_name.clone(),
            orientation: subject.orientation,
            adjust: subject.adjust,
            budget: item.appearance.image,
        };
        // A hit means the photo has not changed in any way that reaches the encoder, so neither
        // its bytes are read nor the quality ladder is walked.
        let cached = cache.entries.get(&key).cloned();
        if cached.is_some() {
            cache.reused += 1;
        }
        plan.push(PhotoPlan {
            subject,
            number,
            stem,
            key,
            cached,
            skipped: None,
        });
    }

    // Pass 2 — read the source bytes of everything that missed.
    //
    // Sequential, and deliberately so: `sources` is a `&dyn` that cannot be shared across
    // threads without a `Sync` bound the implementors would then have to carry, and reading is
    // a memory copy rather than the expensive part. The expensive part is pass 3.
    let mut to_encode: Vec<(usize, Vec<u8>)> = Vec::new();
    for (index, entry) in plan.iter_mut().enumerate() {
        if entry.cached.is_some() {
            continue;
        }
        match sources.read(entry.subject) {
            Ok(bytes) => to_encode.push((index, bytes)),
            // One unreadable photo must not cost the editor the whole build (FR-034).
            Err(error) => {
                entry.skipped = Some(Warning::PhotoSkipped {
                    name: entry.subject.file_name.clone(),
                    reason: error.to_string(),
                });
            }
        }
    }

    // Pass 3 — encode, across threads.
    //
    // This is the whole cost of a cold rebuild: a decode, a resize, and a quality-ladder search
    // per photo, around a tenth of a second each. Thirty of those in sequence is roughly four
    // seconds, which is four times SC-008's budget; the photos are entirely independent of one
    // another, so the fix is to stop doing them one at a time (T102).
    //
    // Determinism survives (FR-024): results are collected against the index they came from and
    // read back in item order, so the fragment, the photo order and the warning order are
    // exactly what a single-threaded run produces. `preview_latency.rs` asserts that directly.
    let encoded = encode_all(&plan, to_encode, &item.appearance.image);

    // Pass 4 — assemble, in item order.
    for (index, entry) in plan.into_iter().enumerate() {
        if let Some(warning) = entry.skipped {
            warnings.push(warning);
            continue;
        }

        let encoded = match entry.cached {
            Some(hit) => hit,
            None => match encoded.get(&index) {
                Some(Ok((entry_bytes, extra))) => {
                    warnings.extend(extra.iter().cloned());
                    cache.encoded += 1;
                    cache.entries.insert(entry.key, entry_bytes.clone());
                    entry_bytes.clone()
                }
                Some(Err(error)) => {
                    warnings.push(Warning::PhotoSkipped {
                        name: entry.subject.file_name.clone(),
                        reason: error.to_string(),
                    });
                    continue;
                }
                // Unreachable: pass 2 either queued the photo or marked it skipped.
                None => continue,
            },
        };

        // Composed from the cache entry either way, so a hit and a miss cannot drift apart.
        let file_name = format!("{}{}", entry.stem, encoded.extension);
        if let Some(floor_quality) = encoded.over_budget_floor {
            warnings.push(Warning::SizeBudgetUnreachable {
                name: file_name.clone(),
                floor_quality,
                achieved: encoded.bytes.len() as u64,
            });
        }

        let url = match &ctx.public_base_url {
            Some(base) => paths::public_url(base.as_str(), &ctx.slug, &file_name),
            None => format!("{PREVIEW_URL_SCHEME}://{}/{}", ctx.slug, file_name),
        };
        rendered.insert(
            entry.subject.id,
            RenderedPhoto {
                url,
                number: entry.number,
            },
        );
        processed.push(ProcessedPhoto {
            id: entry.subject.id,
            file_name,
            bytes: encoded.bytes.to_vec(),
            dimensions: encoded.dimensions,
            quality: encoded.quality,
        });
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
