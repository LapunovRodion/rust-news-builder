//! A purpose-built reader over the Word package (research R3).
//!
//! Three things are needed and nothing else: the paragraph text in document order, the position
//! of each drawing relative to those paragraphs, and the bytes of each referenced media part.
//! That is a bounded walk — `w:body` → `w:p` → `w:r` → `w:drawing` → `a:blip/@r:embed` →
//! relationship id → `word/media/*` — and owning it gives exact control over ordering, which
//! determinism depends on (constitution IV).
//!
//! Positions are what the reference cannot do: it takes its images from a folder. Turning a
//! drawing's document position into a placement is FR-003, and deviation D-6 records it.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;

use crate::error::{Error, Result, Warning};
use crate::model::item::EmbeddedMedia;

const NS_W: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const NS_A: &[u8] = b"http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &[u8] = b"http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";

/// What a Word package yields.
#[derive(Debug, Clone)]
pub struct DocxContent {
    /// Paragraph text in document order, unnormalised and including the blank paragraphs a
    /// drawing sits in. Callers normalise; the reader does not decide what is empty.
    pub paragraphs: Vec<String>,
    /// Every embedded image, in document order, with the position that becomes a placement.
    pub media: Vec<EmbeddedMedia>,
    /// Constructs outside the feature's scope, skipped rather than failed (FR-034).
    pub warnings: Vec<Warning>,
}

/// Reads a `.docx` package.
pub fn read(bytes: &[u8]) -> Result<DocxContent> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::DocumentUnreadable {
            detail: format!("the file is not a readable .docx package: {e}"),
        })?;

    let document_xml =
        read_part(&mut archive, "word/document.xml")?.ok_or_else(|| Error::DocumentUnreadable {
            detail: "the package has no word/document.xml, so it is not a Word document".to_owned(),
        })?;

    // Relationships are optional: a document with no drawings has no rels part in principle.
    let relationships = match read_part(&mut archive, "word/_rels/document.xml.rels")? {
        Some(part) => parse_relationships(&part)?,
        None => BTreeMap::new(),
    };

    let walk = walk_document(&document_xml)?;
    let mut warnings = walk.warnings;
    let mut media = Vec::new();

    for drawing in walk.drawings {
        let Some(target) = relationships.get(&drawing.rel_id) else {
            warnings.push(Warning::PhotoSkipped {
                name: drawing.rel_id.clone(),
                reason: "the document references an image relationship that the package does not \
                         define"
                    .to_owned(),
            });
            continue;
        };
        let part_name = resolve_media_part(target);
        match read_part(&mut archive, &part_name)? {
            Some(payload) => media.push(EmbeddedMedia {
                rel_id: drawing.rel_id,
                part_name,
                bytes: Arc::from(payload.into_boxed_slice()),
                after_paragraph: drawing.after_paragraph,
            }),
            None => warnings.push(Warning::PhotoSkipped {
                name: part_name.clone(),
                reason: "the document references an image part that is missing from the package"
                    .to_owned(),
            }),
        }
    }

    Ok(DocxContent {
        paragraphs: walk.paragraphs,
        media,
        warnings,
    })
}

fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => {
            return Err(Error::DocumentUnreadable {
                detail: format!("could not open `{name}` inside the package: {e}"),
            });
        }
    };
    let mut payload = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut payload)
        .map_err(|e| Error::DocumentUnreadable {
            detail: format!("could not read `{name}` inside the package: {e}"),
        })?;
    Ok(Some(payload))
}

/// Maps a relationship target onto a package part name.
fn resolve_media_part(target: &str) -> String {
    let cleaned = target.trim_start_matches("./");
    if cleaned.starts_with('/') {
        cleaned.trim_start_matches('/').to_owned()
    } else if cleaned.starts_with("word/") {
        cleaned.to_owned()
    } else {
        format!("word/{cleaned}")
    }
}

/// Reads `word/_rels/document.xml.rels` into `id -> target`, skipping external targets.
fn parse_relationships(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let mut reader = quick_xml::NsReader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut out = BTreeMap::new();

    loop {
        let (ns, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(xml_error)?;
        match event {
            Event::Eof => break,
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let is_relationship = tag.local_name().as_ref() == b"Relationship"
                    && matches!(ns, ResolveResult::Bound(n) if n.as_ref() == NS_PKG_R);
                if !is_relationship {
                    continue;
                }
                let mut id = None;
                let mut target = None;
                let mut external = false;
                for attribute in tag.attributes().flatten() {
                    match attribute.key.as_ref() {
                        b"Id" => id = attribute.unescape_value().ok().map(|v| v.into_owned()),
                        b"Target" => {
                            target = attribute.unescape_value().ok().map(|v| v.into_owned());
                        }
                        b"TargetMode" => {
                            external = attribute.value.as_ref() == b"External";
                        }
                        _ => {}
                    }
                }
                if let (Some(id), Some(target), false) = (id, target, external) {
                    out.insert(id, target);
                }
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(out)
}

/// A drawing found during the walk, before its bytes are resolved.
struct Drawing {
    rel_id: String,
    after_paragraph: usize,
}

struct Walk {
    paragraphs: Vec<String>,
    drawings: Vec<Drawing>,
    warnings: Vec<Warning>,
}

/// Walks `word/document.xml` once, in document order.
///
/// Content inside a `w:tbl` is skipped, because `python-docx`'s `document.paragraphs` — the
/// reference's own reading path — does not see it either. Skipping it silently would lose an
/// editor's table without telling them, so it raises a warning instead (FR-034).
fn walk_document(bytes: &[u8]) -> Result<Walk> {
    let mut reader = quick_xml::NsReader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();

    let mut paragraphs: Vec<String> = Vec::new();
    let mut drawings: Vec<Drawing> = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();

    let mut table_depth = 0usize;
    let mut reported_table = false;
    let mut in_paragraph = false;
    let mut in_text = false;
    let mut paragraph_text = String::new();
    // Drawings found in the paragraph currently open, held until its text is known: a drawing
    // inside a paragraph that carries text belongs *after* that paragraph.
    let mut pending: Vec<String> = Vec::new();
    // How many paragraphs with text have been emitted so far. This is the index a placement
    // lands at, which is why blank paragraphs must not advance it.
    let mut text_paragraphs = 0usize;

    loop {
        let (ns, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(xml_error)?;
        let in_word =
            |ns: &ResolveResult<'_>| matches!(ns, ResolveResult::Bound(n) if n.as_ref() == NS_W);

        match event {
            Event::Eof => break,

            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let local = tag.local_name();
                let name = local.as_ref();
                let empty = matches!(event, Event::Empty(_));

                if in_word(&ns) {
                    match name {
                        b"tbl" => {
                            if !empty {
                                table_depth += 1;
                            }
                            if !reported_table {
                                warnings.push(Warning::UnsupportedDocumentFeature {
                                    what: "a table".to_owned(),
                                });
                                reported_table = true;
                            }
                            continue;
                        }
                        b"footnoteReference" | b"endnoteReference" => {
                            note_once(&mut warnings, "a footnote or endnote");
                            continue;
                        }
                        b"commentReference" => {
                            note_once(&mut warnings, "a comment");
                            continue;
                        }
                        b"ins" | b"del" => {
                            note_once(&mut warnings, "a tracked change");
                            continue;
                        }
                        _ => {}
                    }
                }

                if table_depth > 0 {
                    continue;
                }

                if in_word(&ns) {
                    match name {
                        b"p" if !empty => {
                            in_paragraph = true;
                            paragraph_text.clear();
                            pending.clear();
                        }
                        b"t" if !empty => in_text = true,
                        b"tab" => paragraph_text.push('\t'),
                        b"br" | b"cr" => paragraph_text.push('\n'),
                        _ => {}
                    }
                } else if matches!(ns, ResolveResult::Bound(n) if n.as_ref() == NS_A)
                    && name == b"blip"
                {
                    for attribute in tag.attributes().flatten() {
                        let (key_ns, key_local) = reader.resolve_attribute(attribute.key);
                        let is_embed = key_local.as_ref() == b"embed"
                            && matches!(key_ns, ResolveResult::Bound(n) if n.as_ref() == NS_R);
                        if is_embed && let Ok(value) = attribute.unescape_value() {
                            pending.push(value.into_owned());
                        }
                    }
                }
            }

            Event::End(ref tag) => {
                let local = tag.local_name();
                let name = local.as_ref();

                if in_word(&ns) && name == b"tbl" {
                    table_depth = table_depth.saturating_sub(1);
                    continue;
                }
                if table_depth > 0 {
                    continue;
                }
                if !in_word(&ns) {
                    continue;
                }
                match name {
                    b"t" => in_text = false,
                    b"p" => {
                        in_paragraph = false;
                        let has_text = !paragraph_text.trim().is_empty();
                        if has_text {
                            text_paragraphs += 1;
                        }
                        paragraphs.push(std::mem::take(&mut paragraph_text));
                        for rel_id in pending.drain(..) {
                            drawings.push(Drawing {
                                rel_id,
                                after_paragraph: text_paragraphs,
                            });
                        }
                    }
                    _ => {}
                }
            }

            Event::Text(ref text) if in_paragraph && in_text && table_depth == 0 => {
                let value = text.unescape().map_err(xml_error)?;
                paragraph_text.push_str(&value);
            }

            _ => {}
        }
        buffer.clear();
    }

    Ok(Walk {
        paragraphs,
        drawings,
        warnings,
    })
}

fn note_once(warnings: &mut Vec<Warning>, what: &str) {
    let already = warnings
        .iter()
        .any(|w| matches!(w, Warning::UnsupportedDocumentFeature { what: w } if w == what));
    if !already {
        warnings.push(Warning::UnsupportedDocumentFeature {
            what: what.to_owned(),
        });
    }
}

fn xml_error(e: impl std::fmt::Display) -> Error {
    Error::DocumentUnreadable {
        detail: format!("the document's XML is malformed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::resolve_media_part;

    #[test]
    fn relationship_targets_resolve_to_package_parts() {
        assert_eq!(
            resolve_media_part("media/image1.jpg"),
            "word/media/image1.jpg"
        );
        assert_eq!(
            resolve_media_part("./media/image1.jpg"),
            "word/media/image1.jpg"
        );
        assert_eq!(
            resolve_media_part("/word/media/image1.jpg"),
            "word/media/image1.jpg"
        );
        assert_eq!(
            resolve_media_part("word/media/image1.jpg"),
            "word/media/image1.jpg"
        );
    }
}
