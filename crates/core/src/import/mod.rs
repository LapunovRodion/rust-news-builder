//! Turning a file's bytes into a [`NewsItem`].
//!
//! Pure: bytes in, data out. No port is consulted, so import is trivially testable and cannot
//! reach the filesystem or the network.
//!
//! Marker parsing happens **only here** (deviation D-9). Downstream, the editor works on
//! structure and never sees marker text.

pub mod docx;
pub mod plain;
pub mod title;

use std::sync::Arc;

use crate::error::{Result, Warning};
use crate::markers::{self, ParsedBlock};
use crate::model::item::{Block, EmbeddedMedia, Layout, NewsItem, SourceDocument, SourceFormat};
use crate::model::photo::{Adjustments, NaturalKey, Photo, PhotoId, PhotoOrigin, PhotoSource};
use crate::photo;
use crate::publish::slug::slugify;

/// What an import yields.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The detected headline, or `None` when none could be found (FR-004).
    pub title: Option<String>,
    /// The item, with its body and photos in place.
    pub item: NewsItem,
    /// Everything skipped or questioned along the way (FR-034).
    pub warnings: Vec<Warning>,
}

/// Reads a document.
///
/// `.docx` yields paragraphs in document order plus one [`Photo`] per embedded image, each
/// with a placement at its position (FR-002, FR-003). `.txt` and `.md` yield paragraphs and
/// whatever markers the text contains (FR-005) — but no photos, because a plain-text document
/// carries none.
pub fn import_document(bytes: &[u8], format: SourceFormat) -> Result<Imported> {
    match format {
        SourceFormat::Docx => import_docx(bytes),
        SourceFormat::Text => Ok(import_plain(bytes, false)),
        SourceFormat::Markdown => Ok(import_plain(bytes, true)),
    }
}

fn import_plain(bytes: &[u8], is_markdown: bool) -> Imported {
    // Invalid UTF-8 is replaced rather than refused: losing an editor's whole document over
    // one stray byte helps nobody, and the replacement character is visible in the preview.
    let raw = String::from_utf8_lossy(bytes);
    let (detected_title, body) = title::from_plain_text(&raw, is_markdown);

    let mut item = NewsItem::new();
    let (blocks, mut warnings) = blocks_from_body(&body);
    item.body = blocks;
    item.title = detected_title.clone().unwrap_or_default();
    item.slug = slugify(&item.title);
    item.normalise_paragraph_kinds();

    // A marker in a plain-text document names a photo the document does not carry. The
    // placement is kept so the editor can point it at a photo they drop in; the warning is
    // what tells them it needs one.
    warnings.extend(dangling_placement_warnings(&item));

    Imported {
        title: detected_title,
        item,
        warnings,
    }
}

fn import_docx(bytes: &[u8]) -> Result<Imported> {
    let content = docx::read(bytes)?;
    let (detected_title, body) = title::from_docx_paragraphs(&content.paragraphs);

    let mut item = NewsItem::new();
    item.title = detected_title.clone().unwrap_or_default();
    item.slug = slugify(&item.title);

    let mut warnings = content.warnings;

    // Photos first, so the placements below have ids to reference.
    let mut media_photo_ids: Vec<(usize, PhotoId)> = Vec::new();
    for (doc_order, media) in content.media.iter().enumerate() {
        match photo_from_media(&mut item, media, doc_order) {
            Ok(id) => media_photo_ids.push((media.after_paragraph, id)),
            Err(warning) => warnings.push(warning),
        }
    }

    let (blocks, marker_warnings) = blocks_from_body(&body);
    item.body = blocks;
    warnings.extend(marker_warnings);

    // Then the placements the document's own layout implies. Inserting from the last position
    // backwards keeps the earlier indices valid as the body grows.
    for (after_paragraph, id) in media_photo_ids.iter().rev() {
        let at = index_after_nth_paragraph(&item.body, *after_paragraph);
        item.body
            .insert(at, Block::placement(vec![*id], Layout::FullWidth));
    }

    item.normalise_paragraph_kinds();
    warnings.extend(dangling_placement_warnings(&item));

    let source = SourceDocument {
        path: std::path::PathBuf::new(),
        format: SourceFormat::Docx,
        embedded_media: content.media,
    };
    item.source = Some(source);

    Ok(Imported {
        title: detected_title,
        item,
        warnings,
    })
}

/// Adds one embedded image to the item, or explains why it could not be added.
fn photo_from_media(
    item: &mut NewsItem,
    media: &EmbeddedMedia,
    doc_order: usize,
) -> std::result::Result<PhotoId, Warning> {
    let base = media
        .part_name
        .rsplit('/')
        .next()
        .unwrap_or(&media.part_name)
        .to_owned();

    let probe = photo::probe(&media.bytes, &base).map_err(|error| Warning::PhotoSkipped {
        name: base.clone(),
        reason: error.to_string(),
    })?;

    // Word stores parts under names like `image2.tmp`. The container is whatever the bytes
    // say it is, so the published name follows the sniffed format rather than the part's
    // extension — otherwise a perfectly good JPEG would publish as `.tmp`.
    let file_name = rename_to_sniffed_extension(&base, probe.format);
    let file_name = unique_file_name(item, file_name);

    let id = item.mint_photo_id();
    item.photos.push(Photo {
        id,
        origin: PhotoOrigin::Embedded { doc_order },
        source: PhotoSource::Bytes(Arc::clone(&media.bytes)),
        natural_key: NaturalKey::new(&file_name),
        file_name,
        dimensions: probe.dimensions,
        orientation: probe.orientation,
        adjust: Adjustments::NONE,
    });
    Ok(id)
}

fn rename_to_sniffed_extension(base: &str, format: image::ImageFormat) -> String {
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
    let extension = format.extensions_str().first().copied().unwrap_or("bin");
    format!("{stem}.{extension}")
}

/// Appends a numeric suffix until the name is free (INV-5).
fn unique_file_name(item: &NewsItem, wanted: String) -> String {
    if !item.photos.iter().any(|p| p.file_name == wanted) {
        return wanted;
    }
    let (stem, extension) = match wanted.rsplit_once('.') {
        Some((stem, extension)) => (stem.to_owned(), format!(".{extension}")),
        None => (wanted.clone(), String::new()),
    };
    for n in 2u32.. {
        let candidate = format!("{stem}-{n}{extension}");
        if !item.photos.iter().any(|p| p.file_name == candidate) {
            return candidate;
        }
    }
    wanted
}

/// Splits a normalised body into blocks, turning every marker into a placement.
///
/// A marker's 1-based index becomes [`PhotoId`] `n` directly. That mapping holds because
/// [`NewsItem::mint_photo_id`] hands out ids from one in insertion order, and import is the
/// only place photos are added in bulk — so the nth photo is always `PhotoId(n)`.
///
/// Markers are resolved even when the item holds no photo yet. A plain-text document carries
/// none by definition (the reference reads its images from a folder), and dropping the
/// placement would throw away the position the author chose. The placement is kept, and
/// [`dangling_placement_warnings`] reports it until [`attach_photos`] supplies the photos.
fn blocks_from_body(body: &str) -> (Vec<Block>, Vec<Warning>) {
    let (parsed, warnings) = markers::parse(body);
    let mut blocks = Vec::new();

    for element in parsed {
        match element {
            ParsedBlock::Text(text) => {
                blocks.extend(
                    plain::split_paragraphs(&text)
                        .into_iter()
                        .map(Block::paragraph),
                );
            }
            ParsedBlock::Marker(marker) => {
                let ids: Vec<PhotoId> = marker
                    .indices
                    .iter()
                    .filter(|index| **index >= 1)
                    .map(|index| PhotoId(*index as u64))
                    .collect();
                if !ids.is_empty() {
                    blocks.push(Block::placement(ids, marker.layout));
                }
            }
        }
    }

    (blocks, warnings)
}

/// Adds photos to a freshly imported item so its markers resolve (FR-005).
///
/// This is the second half of importing a `.txt` or `.md`: the document supplies the
/// positions, a folder or a drop supplies the photos. Sources are ordered by the reference's
/// natural-sort rule before ids are minted, so marker index *n* lands on the *n*th photo in
/// exactly the order the reference's `discover_images` would have produced.
///
/// Photos that cannot be read are skipped with a warning rather than failing the import
/// (FR-034), and skipping one does **not** renumber the rest — the numbering follows the
/// sorted position, so a broken file cannot silently shift every later marker.
///
/// Binding markers only makes sense on an item that has no photos yet. Attaching to one that
/// already has some — a Word document whose images came from the package, say — mints fresh
/// ids instead, because reusing the sorted position there would collide (INV-3).
pub fn attach_photos(
    item: &mut NewsItem,
    sources: Vec<(String, PhotoSource, Vec<u8>)>,
) -> Vec<Warning> {
    let binds_markers = item.photos.is_empty();
    let mut ordered = sources;
    ordered.sort_by(|a, b| NaturalKey::new(&a.0).cmp(&NaturalKey::new(&b.0)));

    let mut warnings = Vec::new();
    for (position, (file_name, source, bytes)) in ordered.into_iter().enumerate() {
        // When binding, the id is the sorted position rather than a running counter, so a
        // skipped photo leaves a gap instead of pulling every later marker onto the wrong
        // image.
        let id = if binds_markers {
            let id = PhotoId((position + 1) as u64);
            item.bump_photo_id_past(id);
            id
        } else {
            item.mint_photo_id()
        };

        if !crate::photo::extension_is_supported(&file_name) {
            warnings.push(Warning::PhotoSkipped {
                name: file_name,
                reason: "the file is not one of the image formats this tool handles".to_owned(),
            });
            continue;
        }

        match crate::photo::probe(&bytes, &file_name) {
            Ok(probe) => item.photos.push(Photo {
                id,
                origin: PhotoOrigin::Picked {
                    path: std::path::PathBuf::from(&file_name),
                },
                source,
                natural_key: NaturalKey::new(&file_name),
                file_name,
                dimensions: probe.dimensions,
                orientation: probe.orientation,
                adjust: Adjustments::NONE,
            }),
            Err(error) => warnings.push(Warning::PhotoSkipped {
                name: file_name,
                reason: error.to_string(),
            }),
        }
    }

    warnings.extend(dangling_placement_warnings(item));
    warnings
}

/// The body index just after the `n`th paragraph, counting from one. `n == 0` is the front.
fn index_after_nth_paragraph(blocks: &[Block], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut seen = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        if matches!(block, Block::Paragraph { .. }) {
            seen += 1;
            if seen == n {
                return index + 1;
            }
        }
    }
    blocks.len()
}

/// Warns about placements whose photos the item does not hold.
fn dangling_placement_warnings(item: &NewsItem) -> Vec<Warning> {
    let mut warnings = Vec::new();
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
    warnings
}

#[cfg(test)]
mod tests {
    use super::{blocks_from_body, import_document, index_after_nth_paragraph};
    use crate::error::Warning;
    use crate::model::item::{Block, Layout, SourceFormat};

    #[test]
    fn plain_text_import_keeps_marker_positions() {
        let imported = import_document(
            b"Title\n\nFirst.\n\n[image:1]\n\nSecond.\n",
            SourceFormat::Text,
        )
        .expect("plain text always imports");
        assert_eq!(imported.title.as_deref(), Some("Title"));
        assert_eq!(imported.item.body.len(), 3);
        assert!(matches!(imported.item.body[0], Block::Paragraph { .. }));
        assert!(imported.item.body[1].is_placement());
        assert!(matches!(imported.item.body[2], Block::Paragraph { .. }));
    }

    #[test]
    fn the_first_paragraph_is_the_lead() {
        let imported = import_document(
            b"Title\n\nFirst.\n\n[image:1]\n\nSecond.\n",
            SourceFormat::Text,
        )
        .expect("imports");
        use crate::model::item::ParagraphKind;
        assert!(matches!(
            imported.item.body[0],
            Block::Paragraph {
                kind: ParagraphKind::Lead,
                ..
            }
        ));
        assert!(matches!(
            imported.item.body[2],
            Block::Paragraph {
                kind: ParagraphKind::Body,
                ..
            }
        ));
    }

    #[test]
    fn a_marker_naming_a_photo_the_item_lacks_warns() {
        let imported = import_document(b"Title\n\n[image:7]\n", SourceFormat::Text)
            .expect("imports despite the dangling marker");
        assert!(
            imported
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::MissingPhoto { marker } if marker == "[image:7]")),
            "expected a MissingPhoto warning naming the marker, got {:?}",
            imported.warnings
        );
    }

    #[test]
    fn markdown_import_strips_the_heading_marker() {
        let imported =
            import_document(b"# Heading\n\nBody.\n", SourceFormat::Markdown).expect("imports");
        assert_eq!(imported.title.as_deref(), Some("Heading"));
        assert_eq!(imported.item.title, "Heading");
    }

    #[test]
    fn the_slug_is_derived_from_the_title() {
        let imported = import_document(
            "День Конституции\n\nТекст.\n".as_bytes(),
            SourceFormat::Text,
        )
        .expect("imports");
        assert_eq!(imported.item.slug.as_str(), "den-konstitutsii");
    }

    #[test]
    fn a_row_marker_keeps_its_layout_and_order() {
        let (blocks, _) = blocks_from_body("[images:2,1]");
        let Some(Block::Placement { photos, layout }) = blocks.first() else {
            unreachable!("expected one placement, got {blocks:?}")
        };
        assert_eq!(*layout, Layout::Row);
        assert_eq!(photos.iter().map(|p| p.0).collect::<Vec<_>>(), vec![2, 1]);
    }

    #[test]
    fn insertion_points_count_paragraphs_only() {
        let blocks = vec![
            Block::paragraph("one"),
            Block::placement(vec![crate::model::photo::PhotoId(1)], Layout::FullWidth),
            Block::paragraph("two"),
        ];
        assert_eq!(index_after_nth_paragraph(&blocks, 0), 0);
        assert_eq!(index_after_nth_paragraph(&blocks, 1), 1);
        assert_eq!(index_after_nth_paragraph(&blocks, 2), 3);
        // Past the end lands at the end rather than out of bounds.
        assert_eq!(index_after_nth_paragraph(&blocks, 9), 3);
    }

    #[test]
    fn an_unreadable_docx_is_an_error_not_a_panic() {
        let result = import_document(b"this is not a zip archive", SourceFormat::Docx);
        assert!(result.is_err());
    }
}
