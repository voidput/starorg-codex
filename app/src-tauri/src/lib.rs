//! StarOrg Strings desktop app: a thin Tauri 2 shell over `starorg-core` (spec 3.1, 9).

pub mod commands;
pub mod gpu;

use commands::DiscoveryConfig;

/// The app with its state and commands, generic over the runtime so tests can use
/// `tauri::test::mock_builder()`.
pub fn builder<R: tauri::Runtime>(
    b: tauri::Builder<R>,
    discovery: DiscoveryConfig,
) -> tauri::Builder<R> {
    b.manage(discovery).invoke_handler(tauri::generate_handler![
        commands::list_channels,
        commands::app_info
    ])
}

/// Start the GUI. [`gpu::apply`] must already have run (see `main.rs`).
pub fn run() {
    builder(tauri::Builder::default(), DiscoveryConfig::default())
        .run(tauri::generate_context!())
        .expect("error while running StarOrg Strings");
}
