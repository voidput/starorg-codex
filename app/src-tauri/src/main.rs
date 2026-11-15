// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Before tauri::Builder and before any thread starts (spec 10.4).
    starorg_strings_app::gpu::apply();
    starorg_strings_app::run();
}
