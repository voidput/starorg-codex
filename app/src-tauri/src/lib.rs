//! StarOrg Codex desktop app: a thin Tauri 2 shell over `starorg-core` (spec 3.1, 9).

pub mod commands;
pub mod contract;
pub mod gpu;
pub mod mcp;
pub mod update;

#[cfg(test)]
mod contrast_tests;

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
        .invoke_handler(contract_handler![
            commands::list_channels,
            commands::app_info
        ])
}

/// The backend the app runs with: the real one over starorg-core, or the in-memory mock with
/// the non-default `mock-backend` feature. The other values receive the native dialogs and
/// the update opener/installer once the app exists.
fn backend() -> (BackendState, contract::real::DialogSlot, update::HostSlot) {
    let slot = contract::real::DialogSlot::default();
    #[cfg(feature = "mock-backend")]
    {
        // Slow enough to see progress in the window, fast enough not to annoy.
        let b = contract::mock::MockBackend::new(std::time::Duration::from_millis(250));
        (BackendState::new(b), slot, update::HostSlot::default())
    }
    #[cfg(not(feature = "mock-backend"))]
    {
        let mut cfg = contract::real::RealConfig::from_env()
            .unwrap_or_else(|e| panic!("StarOrg Codex cannot start: {e}"));
        cfg.dialogs = slot.clone();
        let b = contract::real::RealBackend::new(cfg);
        let host = b.update_host();
        (BackendState::new(b), slot, host)
    }
}

/// Start the GUI. [`gpu::apply`] must already have run (see `main.rs`).
pub fn run() {
    let (state, dialogs, update_host) = backend();
    let app = builder(tauri::Builder::default(), DiscoveryConfig::default(), state)
        .plugin(tauri_plugin_dialog::init());
    // Used from Rust only (update::host); the webview has no updater permission. The key
    // comes from updater-public-key.txt, not from tauri.conf.json. Not in the Store
    // flavour: the Store updates that build, and nothing there may replace the app.
    #[cfg(not(feature = "store"))]
    let app = app.plugin(
        tauri_plugin_updater::Builder::new()
            .pubkey(update::host::UPDATER_PUBKEY.trim())
            .build(),
    );
    app.setup(move |app| {
        dialogs.set(contract::real::TauriDialogs(app.handle().clone()));
        update_host.set(update::host::TauriUpdateHost(app.handle().clone()));
        Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while building StarOrg Codex")
    .run(|app, event| {
        // The local MCP server stops with the app (docs/mcp.md).
        if let tauri::RunEvent::Exit = event {
            use tauri::Manager;
            let _ = app
                .state::<BackendState>()
                .0
                .set_mcp_enabled(contract::types::McpToggle { enabled: false });
        }
    });
}

/// `starorg-strings-app extract-worker --p4k <P> --out <DIR> [--ini-only] [--keyrefs]`: the extraction
/// worker the real backend starts (spec 3.3), the same protocol as the CLI's hidden
/// subcommand. Returns the exit code, or `None` when the arguments are not a worker call.
pub fn worker_main(args: &[std::ffi::OsString]) -> Option<i32> {
    use std::path::{Path, PathBuf};
    // `dev-worker --p4k <P>` (or `--dcb <F> [--ini <F>]`): the Developer mode explorer
    // (addendum 7.2). Read only; exports go only to paths from a native save dialog.
    if args.get(1).map(|a| a.as_os_str()) == Some("dev-worker".as_ref()) {
        return Some(match starorg_data::devworker::parse_args(&args[2..]) {
            Ok(input) => starorg_data::devworker::run(&input),
            Err(e) => {
                eprintln!("dev-worker: {e}");
                2
            }
        });
    }
    // The local MCP server's extraction worker: one dataset per process (docs/mcp.md).
    if let Some(code) = starorg_mcp::worker::worker_main(args) {
        return Some(code);
    }
    if args.get(1).map(|a| a.as_os_str()) != Some("extract-worker".as_ref()) {
        return None;
    }
    let mut p4k: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut ini_only = false;
    let mut keyrefs = false;
    let mut it = args[2..].iter();
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--p4k") => p4k = it.next().map(PathBuf::from),
            Some("--out") => out = it.next().map(PathBuf::from),
            Some("--ini-only") => ini_only = true,
            Some("--keyrefs") => keyrefs = true,
            _ => {
                eprintln!("extract-worker: unknown argument {}", a.to_string_lossy());
                return Some(2);
            }
        }
    }
    let (Some(p4k), Some(out)) = (p4k, out) else {
        eprintln!("extract-worker: needs --p4k and --out");
        return Some(2);
    };
    // The worker writes only below --out; never inside the game or its prefix.
    let out_abs = starorg_core::fsutil::canonical_lenient(&out);
    let p4k_abs = starorg_core::fsutil::canonical_lenient(&p4k);
    let mut protected: Vec<PathBuf> = p4k_abs
        .parent()
        .filter(|d| d.join("build_manifest.id").is_file())
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    protected.extend(starorg_core::discovery::enclosing_wine_prefix(&p4k_abs));
    if let Some(r) = protected.iter().find(|r| out_abs.starts_with(r)) {
        eprintln!(
            "extract-worker: --out {} is inside {}; refusing",
            out_abs.display(),
            r.display()
        );
        return Some(2);
    }
    Some(starorg_data::worker::run_opts(
        &p4k,
        &out,
        starorg_data::worker::TestHook::None,
        starorg_data::extract::Options {
            ini_only,
            keyrefs: keyrefs && !ini_only,
        },
    ))
}
