//! Obtain hints for `apply`, `build` and the launch hooks (spec obtain-hints): the MCP
//! server's datasets (item-tags, loot, item-sources, mining-spawns, commodities), extracted
//! on first use by this binary started as `mcp-extract-worker`, under the memory limit,
//! and cached in the same folder `starorg-codex mcp` and the app use.
//!
//! The launch hooks only read hints already built ([`Obtain::Stored`]): building them on a
//! new build takes minutes, which is the post-exit background job's work
//! (`hook_cmds::background_job`), never the hooks' budget.

use crate::GlobalOpts;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::{OBTAIN_LATER, ObtainFetch, ObtainProvider, ObtainSource};
use starorg_core::config::Config;
use starorg_core::discovery::Install;
use starorg_core::memlimit;
use starorg_core::model::ObtainHints;
use starorg_mcp::hints::ToolsObtain;
use starorg_mcp::worker::ProcessExtractor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How the hints may be had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Obtain {
    /// Extract and build what is missing; everything ends by the deadline, if any.
    Full(Option<Instant>),
    /// Only hints already built and stored; otherwise left for later (the launch hooks).
    Stored,
}

/// `--test-obtain-hints FILE`: hints as JSON, a read failure being the warning. With
/// `--test-slow-obtain-ms`, a stand-in for a slow first build: [`Obtain::Full`] sleeps that
/// long, then marks the hints built (`FILE.built`); [`Obtain::Stored`] serves them only once
/// marked.
struct FileObtain {
    file: PathBuf,
    slow: Option<Duration>,
    stored_only: bool,
}

impl FileObtain {
    fn built(&self) -> PathBuf {
        let mut p = self.file.clone().into_os_string();
        p.push(".built");
        PathBuf::from(p)
    }

    fn read(path: &Path) -> Result<ObtainHints, String> {
        std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<ObtainHints>(&b).map_err(|e| e.to_string()))
    }
}

impl ObtainSource for FileObtain {
    fn fetch(&self, _channel: &str) -> ObtainFetch {
        if let Some(d) = self.slow {
            if self.stored_only {
                if !self.built().is_file() {
                    return ObtainFetch {
                        hints: None,
                        warnings: vec![format!(
                            "WHERE TO GET left out: not built yet for this build; {OBTAIN_LATER}"
                        )],
                    };
                }
            } else {
                std::thread::sleep(d);
                let _ = std::fs::write(self.built(), b"");
            }
        }
        match Self::read(&self.file) {
            Ok(h) => ObtainFetch {
                hints: Some(h),
                warnings: Vec::new(),
            },
            Err(e) => ObtainFetch {
                hints: None,
                warnings: vec![format!(
                    "WHERE TO GET left out: {}: {e}",
                    self.file.display()
                )],
            },
        }
    }

    fn ready(&self, _channel: &str) -> bool {
        self.slow.is_none() || !self.stored_only || self.built().is_file()
    }
}

/// The obtain-hints source for an install, extracting what is missing ([`Obtain::Full`]
/// with `deadline`: every dataset extraction and the in-process loot pass end by then, all
/// of them together; what is cut short is left out and done on a later run). `None` for the
/// synthetic test game (`--test-stock-ini`) unless `--test-obtain-hints` gives a file.
pub fn provider(
    g: &GlobalOpts,
    install: &Install,
    dirs: &AppDirs,
    deadline: Option<Instant>,
) -> Option<ObtainProvider> {
    provider_as(g, install, dirs, Obtain::Full(deadline))
}

/// [`provider`] in either [`Obtain`] mode.
pub fn provider_as(
    g: &GlobalOpts,
    install: &Install,
    dirs: &AppDirs,
    mode: Obtain,
) -> Option<ObtainProvider> {
    if let Some(p) = g.test_obtain_hints() {
        return Some(ObtainProvider(Arc::new(FileObtain {
            file: p.to_path_buf(),
            slow: g.test_slow_obtain_ms().map(Duration::from_millis),
            stored_only: mode == Obtain::Stored,
        })));
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
    let deadline = match mode {
        Obtain::Full(d) => d,
        Obtain::Stored => None,
    };
    ex.deadline = deadline;
    let store_cfg = starorg_mcp::config_for(Some(install), dirs, Arc::new(ex));
    let mut store = starorg_mcp::store::Store::new(store_cfg);
    if !g.quiet && !g.json {
        store = store.with_notify(Arc::new(|m: String| eprintln!("{m}")));
    }
    let tools = Arc::new(starorg_mcp::tools::Tools::new(store));
    Some(match mode {
        Obtain::Full(d) => ToolsObtain::provider_until(tools, true, d),
        Obtain::Stored => ToolsObtain::provider_stored(tools),
    })
}
