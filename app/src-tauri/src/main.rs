// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The real backend extracts Data.p4k in a child process: this binary, started as
    // `extract-worker` (spec 3.3). No window, no GPU setup for it.
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(code) = starorg_strings_app::worker_main(&args) {
        std::process::exit(code);
    }
    // Before tauri::Builder and before any thread starts (spec 10.4).
    starorg_strings_app::gpu::apply();
    starorg_strings_app::run();
}
