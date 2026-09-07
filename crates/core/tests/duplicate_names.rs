//! T080 — two photos with the same file name are both kept (INV-5).
//!
//! Dropping a folder of `IMG_0001.jpg` from two cameras is an ordinary thing to do, and losing
//! one of them silently is the worst possible answer. The item keeps both and makes the names
//! distinct; the published names were never the source names anyway.

mod support;

use newsbuilder_core::build::{BuildContext, EmbeddedBytes, build};
use newsbuilder_core::model::item::{
    Block, Layout, NewsItem, PhotoIntake, add_photos, remove_photo,
};
use newsbuilder_core::model::photo::PhotoOrigin;
use newsbuilder_core::model::server::Slug;

use support::jpeg_bytes;

fn intake(name: &str, seed: u32) -> PhotoIntake {
    PhotoIntake::from_bytes(
        name.to_owned(),
        jpeg_bytes(120, 90, seed),
        PhotoOrigin::Dropped,
    )
}

#[test]
fn two_photos_with_the_same_name_are_both_kept() {
    let mut item = NewsItem::new();
    item.title = "Duplicate Names".to_owned();
    let added = add_photos(
        &mut item,
        vec![intake("IMG_0001.jpg", 1), intake("IMG_0001.jpg", 2)],
    );

    assert_eq!(
        added.ids.len(),
        2,
        "neither was dropped: {:?}",
        added.warnings
    );
    assert_eq!(item.photos.len(), 2);
    assert!(item.file_names_are_unique(), "INV-5");
    assert!(item.photo_ids_are_unique(), "INV-3");
}

#[test]
fn the_second_keeps_the_stem_and_the_extension() {
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        vec![intake("IMG_0001.jpg", 1), intake("IMG_0001.jpg", 2)],
    );
    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(names[0], "IMG_0001.jpg");
    assert!(
        names[1].starts_with("IMG_0001-") && names[1].ends_with(".jpg"),
        "the suffixed name is still recognisably the same photo: {}",
        names[1]
    );
}

#[test]
fn a_third_and_a_fourth_keep_going_rather_than_colliding_with_the_second() {
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        (0..4).map(|n| intake("IMG_0001.jpg", n)).collect(),
    );
    assert_eq!(item.photos.len(), 4);
    assert!(item.file_names_are_unique(), "INV-5");
}

#[test]
fn both_publish_under_distinct_names() {
    let mut item = NewsItem::new();
    item.title = "Duplicate Names".to_owned();
    item.slug = Slug::parse("duplicate-names").expect("well formed");
    let added = add_photos(
        &mut item,
        vec![intake("IMG_0001.jpg", 1), intake("IMG_0001.jpg", 2)],
    );
    item.body = vec![Block::placement(added.ids.clone(), Layout::Row)];

    let ctx = BuildContext {
        public_base_url: Some(
            url::Url::parse("https://example.org/news/2026/03/").expect("a valid url"),
        ),
        slug: item.slug.clone(),
    };
    let out = build(&item, &ctx, &EmbeddedBytes).expect("builds");

    let published: Vec<&str> = out.processed.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(published.len(), 2);
    assert_ne!(
        published[0], published[1],
        "two files uploaded under one name would overwrite each other"
    );
    assert_eq!(
        published,
        vec!["duplicate-names-01.jpg", "duplicate-names-02.jpg"],
        "published names come from the title and the position, not from the source name"
    );
}

#[test]
fn a_name_freed_by_a_removal_becomes_available_again() {
    let mut item = NewsItem::new();
    let added = add_photos(
        &mut item,
        vec![intake("IMG_0001.jpg", 1), intake("IMG_0001.jpg", 2)],
    );
    remove_photo(&mut item, added.ids[0]);

    let more = add_photos(&mut item, vec![intake("IMG_0001.jpg", 3)]);
    assert_eq!(more.ids.len(), 1);
    assert_eq!(
        item.photo(more.ids[0]).map(|p| p.file_name.as_str()),
        Some("IMG_0001.jpg"),
        "the name is free, so no suffix is needed"
    );
    assert!(item.file_names_are_unique(), "INV-5");
}

#[test]
fn names_that_differ_only_in_case_are_treated_as_different() {
    // The remote is POSIX, where they are different files. Suffixing them would be an
    // invention, not a fix.
    let mut item = NewsItem::new();
    add_photos(
        &mut item,
        vec![intake("Photo.jpg", 1), intake("photo.jpg", 2)],
    );
    let names: Vec<&str> = item.photos.iter().map(|p| p.file_name.as_str()).collect();
    assert_eq!(names, vec!["Photo.jpg", "photo.jpg"]);
}
