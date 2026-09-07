//! T081 — a file that is not an image is turned away by name, and adds nothing (FR-011).
//!
//! Two rejections, deliberately distinct:
//!
//! - **By name.** A `.pdf` never reaches the decoder. This is what makes dropping a folder of
//!   mixed files cheap, and it is what lets the message say `notes.pdf` rather than "image".
//! - **By content.** A file called `.jpg` that is not one is caught when it is probed. Names
//!   are a hint; the bytes decide.
//!
//! Either way, the item is unchanged and the editor is told which file and why (SC-007).

mod support;

use newsbuilder_core::error::Warning;
use newsbuilder_core::model::item::{NewsItem, PhotoIntake, add_photos};
use newsbuilder_core::model::photo::PhotoOrigin;
use newsbuilder_core::photo::extension_is_supported;

use support::jpeg_bytes;

fn bytes_intake(name: &str, bytes: Vec<u8>) -> PhotoIntake {
    PhotoIntake::from_bytes(name.to_owned(), bytes, PhotoOrigin::Dropped)
}

fn skipped_names(warnings: &[Warning]) -> Vec<&str> {
    warnings
        .iter()
        .filter_map(|w| match w {
            Warning::PhotoSkipped { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_non_image_is_rejected_by_name_and_adds_nothing() {
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![bytes_intake("notes.pdf", b"%PDF-1.7 not an image".to_vec())],
    );

    assert!(added.ids.is_empty());
    assert!(item.photos.is_empty(), "nothing was added to the item");
    assert_eq!(skipped_names(&added.warnings), vec!["notes.pdf"]);
}

#[test]
fn the_reason_names_the_file_and_says_what_was_wrong() {
    let mut item = NewsItem::new();
    let added = add_photos(&mut item, vec![bytes_intake("clip.mp4", vec![0u8; 64])]);
    let Some(Warning::PhotoSkipped { name, reason }) = added.warnings.first() else {
        panic!("expected one PhotoSkipped, got {:?}", added.warnings)
    };
    assert_eq!(name, "clip.mp4");
    assert!(
        !reason.is_empty(),
        "SC-007 wants a reason an editor can act on"
    );
}

#[test]
fn every_format_fr_011_lists_is_accepted_by_name() {
    for name in [
        "a.jpg", "a.jpeg", "a.JPG", "a.jfif", "a.JFIF", "a.jpe", "a.jif", "a.png", "a.webp",
        "a.gif", "a.bmp", "a.tif", "a.tiff",
    ] {
        assert!(extension_is_supported(name), "{name}");
    }
}

#[test]
fn everything_else_is_turned_away_by_name() {
    for name in [
        "notes.pdf",
        "clip.mp4",
        "archive.zip",
        "sheet.xlsx",
        "photo",
        "photo.jpg.txt",
        "photo.svg",
        // An image format, but not one this build decodes. It is refused by name like the
        // rest; only the sentence it is refused with differs.
        "IMG_0009.heic",
        "shot.avif",
    ] {
        assert!(!extension_is_supported(name), "{name}");
    }
}

#[test]
fn a_file_named_jpg_that_is_not_one_is_caught_when_it_is_read() {
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![bytes_intake(
            "pretend.jpg",
            b"this is plain text wearing a jpeg name".to_vec(),
        )],
    );
    assert!(added.ids.is_empty());
    assert!(item.photos.is_empty());
    assert_eq!(skipped_names(&added.warnings), vec!["pretend.jpg"]);
}

#[test]
fn one_bad_file_in_a_drop_does_not_cost_the_good_ones() {
    // FR-034: the drop keeps going. This is the case that decides whether an editor can drag a
    // whole folder onto the window or has to pick files one at a time.
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![
            bytes_intake("one.jpg", jpeg_bytes(60, 40, 1)),
            bytes_intake("notes.pdf", b"%PDF".to_vec()),
            bytes_intake("two.jpg", jpeg_bytes(60, 40, 2)),
        ],
    );

    assert_eq!(added.ids.len(), 2);
    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(names, vec!["one.jpg", "two.jpg"]);
    assert_eq!(skipped_names(&added.warnings), vec!["notes.pdf"]);
}

#[test]
fn a_rejected_file_does_not_consume_a_photo_id() {
    // Ids are the item's own bookkeeping; a rejected file must leave no trace at all, or the
    // published numbering would carry a gap nobody can explain.
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        vec![
            bytes_intake("notes.pdf", b"%PDF".to_vec()),
            bytes_intake("one.jpg", jpeg_bytes(60, 40, 1)),
        ],
    );
    let mut second = NewsItem::new();
    add_photos(
        &mut second,
        vec![bytes_intake("one.jpg", jpeg_bytes(60, 40, 1))],
    );
    assert_eq!(item.photos[0].id, second.photos[0].id);
}

#[test]
fn a_jfif_is_a_jpeg_and_is_accepted() {
    // `.jfif` is what Windows, Outlook and browsers call a JPEG when they save one. The bytes
    // are a JPEG, the decoder reads them, and only the name list stood in the way.
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![bytes_intake("scan.jfif", jpeg_bytes(60, 40, 7))],
    );

    assert_eq!(added.ids.len(), 1, "warnings: {:?}", added.warnings);
    assert!(skipped_names(&added.warnings).is_empty());
    assert_eq!(item.photos[0].file_name, "scan.jfif");
}

#[test]
fn a_jfif_publishes_as_jpg() {
    // The alias must not reach the published URL: the container is a JPEG, so the name is one.
    use newsbuilder_core::build::{BuildContext, EmbeddedBytes, build};
    use newsbuilder_core::model::item::{Block, Layout};
    use newsbuilder_core::model::server::Slug;

    let mut item = NewsItem::new();
    item.title = "День Конституции".to_owned();
    item.slug = Slug::parse("den-konstitutsii").expect("well formed");
    let added = add_photos(
        &mut item,
        vec![bytes_intake("scan.jfif", jpeg_bytes(60, 40, 7))],
    );
    item.body.push(Block::Placement {
        photos: vec![added.ids[0]],
        layout: Layout::FullWidth,
    });

    let output = build(
        &item,
        &BuildContext::preview(item.slug.clone()),
        &EmbeddedBytes,
    )
    .expect("the item builds");

    let names: Vec<&str> = output
        .processed
        .iter()
        .map(|p| p.file_name.as_str())
        .collect();
    assert_eq!(names, vec!["den-konstitutsii-01.jpg"]);
}

#[test]
fn a_heic_is_refused_by_the_format_it_is_rather_than_by_being_unrecognised() {
    // It *is* an image format; this build simply cannot decode one. Saying otherwise sends the
    // editor looking for a corrupt file instead of converting it (FR-011's "named reason").
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![bytes_intake("IMG_0009.HEIC", vec![0u8; 64])],
    );
    let Some(Warning::PhotoSkipped { name, reason }) = added.warnings.first() else {
        panic!("expected one PhotoSkipped, got {:?}", added.warnings)
    };

    assert_eq!(name, "IMG_0009.HEIC");
    assert!(
        !reason.contains("is not an image format"),
        "HEIC is an image format; the refusal must not claim otherwise: {reason}"
    );
    assert!(
        reason.to_ascii_lowercase().contains("heic"),
        "the reason should name the format: {reason}"
    );
    assert!(
        reason.contains("convert"),
        "the reason should say what to do instead: {reason}"
    );
}
