// A console window behind a desktop application on Windows is noise, not a feature.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    newsbuilder_desktop_lib::run();
}
