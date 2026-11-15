//! StarOrg Strings desktop app: a thin Tauri 2 shell over `starorg-core` (spec 3.1, 9).

pub mod commands;
pub mod contract;
pub mod gpu;

use commands::DiscoveryConfig;
use contract::BackendState;

/// The app with its state and commands, generic over the runtime so tests can use
/// `tauri::test::mock_builder()`.
pub fn builder<R: tauri::Runtime>(
    b: tauri::Builder<R>,
    discovery: DiscoveryConfig,
    backend: BackendState,
) -> tauri::Builder<R> {
    b.manage(discovery)
        .manage(backend)
        .invoke_handler(contract_handler![commands::list_channels, commands::app_info])
}

/// The backend the app runs with. Mock until M1/M2 land (feature `mock-backend`).
fn backend() -> BackendState {
    #[cfg(feature = "mock-backend")]
    {
        // Slow enough to see progress in the window, fast enough not to annoy.
        BackendState::new(contract::mock::MockBackend::new(std::time::Duration::from_millis(250)))
    }
    #[cfg(not(feature = "mock-backend"))]
    {
        compile_error!("no real contract::Backend yet: build with the mock-backend feature");
    }
}

/// Start the GUI. [`gpu::apply`] must already have run (see `main.rs`).
pub fn run() {
    builder(tauri::Builder::default(), DiscoveryConfig::default(), backend())
        .run(tauri::generate_context!())
        .expect("error while running StarOrg Strings");
}
