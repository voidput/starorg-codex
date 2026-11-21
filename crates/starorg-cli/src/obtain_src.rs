//! Obtain hints for `apply`, `build` and the launch hooks (spec obtain-hints): the MCP
//! server's datasets (item-tags, loot, item-sources, mining-spawns, commodities), extracted
//! on first use by this binary started as `mcp-extract-worker`, under the memory limit,
//! and cached in the same folder `starorg-codex mcp` and the app use.

use crate::GlobalOpts;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::{ObtainFetch, ObtainProvider, ObtainSource};
use starorg_core::config::Config;
use starorg_core::discovery::Install;
use starorg_core::memlimit;
use starorg_core::model::ObtainHints;
use starorg_mcp::hints::ToolsObtain;
use starorg_mcp::worker::ProcessExtractor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// `--test-obtain-hints FILE`: hints as JSON, a read failure being the warning.
struct FileObtain(PathBuf);

impl ObtainSource for FileObtain {
    fn fetch(&self, _channel: &str) -> ObtainFetch {
        let read = std::fs::read(&self.0)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<ObtainHints>(&b).map_err(|e| e.to_string()));
        match read {
            Ok(h) => ObtainFetch {
                hints: Some(h),
                warnings: Vec::new(),
            },
            Err(e) => ObtainFetch {
                hints: None,
                warnings: vec![format!("WHERE TO GET left out: {}: {e}", self.0.display())],
            },
        }
    }
}

/// The obtain-hints source for an install. `timeout`: at most this long per dataset
/// extraction (the hooks' budget). `None` for the synthetic test game (`--test-stock-ini`)
/// unless `--test-obtain-hints` gives a file.
pub fn provider(
    g: &GlobalOpts,
    install: &Install,
    dirs: &AppDirs,
    timeout: Option<Duration>,
) -> Option<ObtainProvider> {
    if let Some(p) = g.test_obtain_hints() {
        return Some(ObtainProvider(Arc::new(FileObtain(p.to_path_buf()))));
    }
    if g.test_stock_ini().is_some() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let cfg = Config::load(dirs).unwrap_or_default();
    let flag = std::env::var("STARORG_MEMORY_LIMIT_GB").ok();
    let limit = memlimit::effective_limit(flag.as_deref(), None, &cfg, memlimit::meminfo().total)
        .ok()
        .flatten();
    let mut ex = ProcessExtractor::new(exe).with_limit(Arc::new(move || limit));
    if let Some(t) = timeout {
        ex.timeout = ex.timeout.min(t.max(Duration::from_secs(1)));
    }
    let store_cfg = starorg_mcp::config_for(Some(install), dirs, Arc::new(ex));
    let mut store = starorg_mcp::store::Store::new(store_cfg);
    if !g.quiet && !g.json {
        store = store.with_notify(Arc::new(|m: String| eprintln!("{m}")));
    }
    let tools = Arc::new(starorg_mcp::tools::Tools::new(store));
    Some(ToolsObtain::provider(tools, true))
}
