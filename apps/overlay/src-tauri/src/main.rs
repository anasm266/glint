// Hide the Windows console in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    overlay_app_lib::run();
}
