//! The News Builder desktop application.
//!
//! A Tauri shell over `newsbuilder-core`. The Rust side owns the item and every rule; the
//! webview presents it and puts the fragment the core rendered into a sandboxed iframe
//! (constitution principle II, FR-022).
//!
//! The binary is a thin shell over this library so the command layer can be compiled and
//! tested without starting a window.

pub mod commands;
pub mod error;
pub mod state;

use std::sync::Mutex;

use tauri::Manager;

use state::{Session, SessionState, preview_dir, thumbnail_dir};

/// Builds and runs the application.
///
/// # Panics
///
/// Only if Tauri itself cannot start — a missing webview runtime, or a configuration the build
/// script already rejected. There is no window to report it in at that point, so failing loudly
/// is the honest answer.
pub fn run() {
    // A subscriber belongs in the frontends, not in the library every test links (T020). The
    // filter is `info` unless `RUST_LOG` says otherwise; nothing that reaches it can format a
    // `Secret`, which renders as `[redacted]` through both formatters.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Thumbnails and the preview's photos live under the application's own cache
            // directory, which is what `assetProtocol.scope` in `tauri.conf.json` grants the
            // webview read access to. A path outside it is served 403 and shows as nothing.
            let cache = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("newsbuilder"));
            std::fs::create_dir_all(thumbnail_dir(&cache)).ok();
            std::fs::create_dir_all(preview_dir(&cache)).ok();
            app.manage(Session(Mutex::new(SessionState::new(&cache))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::item::open_document,
            commands::item::new_item,
            commands::item::set_title,
            commands::item::set_body,
            commands::item::set_slug,
            commands::item::build_preview,
            commands::item::export_fragment,
            commands::item::copy_fragment,
            commands::item::is_dirty,
            commands::photo::add_photos_from_paths,
            commands::photo::add_photo_from_clipboard,
            commands::photo::remove_photo,
            commands::photo::reorder_photos,
            commands::photo::rename_photo,
            commands::photo::set_crop,
            commands::photo::rotate_photo,
            commands::photo::suggest_crop_for,
            commands::placement::insert_placement,
            commands::placement::move_placement,
            commands::placement::remove_placement,
            commands::placement::add_to_placement,
            commands::placement::set_layout,
            commands::arrange::arrange_auto,
            commands::arrange::set_one_placement,
            commands::publish::list_servers,
            commands::publish::save_server,
            commands::publish::delete_server,
            commands::publish::set_credential,
            commands::publish::delete_credential,
            commands::publish::secret_store_available,
            commands::publish::set_session_credential,
            commands::publish::clear_session_credential,
            commands::publish::publish,
            commands::publish::check_site,
            commands::site::get_article_settings,
            commands::site::set_article_settings,
            commands::site::get_intro_image,
            commands::site::set_intro_image,
        ])
        .run(tauri::generate_context!())
        .expect("the application could not start");
}
