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
        "a.jpg", "a.jpeg", "a.JPG", "a.png", "a.webp", "a.gif", "a.bmp", "a.tif", "a.tiff",
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
