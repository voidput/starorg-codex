//! Experimental local MCP server: read-only Star Citizen game data tools for an MCP client
//! on this computer (Claude Code, Claude Desktop, ...). See `docs/mcp.md`.
//!
//! - [`protocol`]: JSON-RPC 2.0 + the MCP lifecycle and tool calls (no SDK);
//! - [`tools`]: the tools and their answers, over [`starorg_gamedata::query`];
//! - [`store`]: datasets per channel and build, extracted once by a worker process and
//!   cached as JSON under the app cache directory;
//! - [`worker`]: that extraction worker (`mcp-extract-worker`), with its own RSS measurement;
//! - [`owned`]: owned blueprints from the app's store, for `how_to_obtain`;
//! - [`sources`]: the app explorer's Sources card (loot containers, where an ore is mined);
//! - [`shops`]: the shop browser (what each UEX terminal sells, by category);
//! - [`hints`]: the same rows as obtain hints for `global.ini` descriptions;
//! - [`stdio`] (the CLI's `starorg-codex mcp`) and `http` (feature `http`, the app's
//!   loopback server) transports;
//! - [`rss`]: resident memory of this process.
//!
//! Read-only throughout: no tool writes game files or app settings, and nothing here opens
//! a network connection (the HTTP transport only listens on 127.0.0.1).

pub mod hints;
pub mod owned;
pub mod protocol;
pub mod rss;
pub mod shops;
pub mod sources;
pub mod stdio;
pub mod store;
pub mod tools;
pub mod worker;

#[cfg(feature = "http")]
pub mod http;

use starorg_core::appdirs::AppDirs;
use starorg_core::config::{self, Config, Outputs};
use starorg_core::discovery::{DiscoveryEnv, DiscoveryRequest, Install, discover};
use std::sync::Arc;
use store::{ChannelSource, StoreConfig};

/// The channels of an install, with the app data folder of each when the install is known.
pub fn channels_of(install: &Install, dirs: &AppDirs) -> Vec<ChannelSource> {
    let cfg = Config::load(dirs).unwrap_or_default();
    let outputs = Outputs::load(dirs).unwrap_or_default();
    let id = config::lookup_install(&cfg, &outputs, install).map(|(id, _)| id);
    install
        .channels
        .iter()
        .map(|c| ChannelSource {
            name: c.name.clone(),
            p4k: c.p4k.clone(),
            build: c.build.as_ref().map(|b| b.version.clone()),
            dir: c.dir.clone(),
            data_dir: id.as_ref().map(|id| dirs.channel_dir(id, &c.name)),
        })
        .collect()
}

/// Server configuration from the app folders and install discovery (the first install).
pub fn config_from_env(
    req: &DiscoveryRequest,
    dirs: &AppDirs,
    extractor: Arc<dyn worker::Extractor>,
) -> StoreConfig {
    let found = discover(req, &DiscoveryEnv::from_process());
    config_for(found.installs.first(), dirs, extractor)
}

/// Server configuration for one install (the app passes the install it shows).
pub fn config_for(
    install: Option<&Install>,
    dirs: &AppDirs,
    extractor: Arc<dyn worker::Extractor>,
) -> StoreConfig {
    let channels = install.map(|i| channels_of(i, dirs)).unwrap_or_default();
    let cfg = Config::load(dirs).unwrap_or_default();
    let outputs = Outputs::load(dirs).unwrap_or_default();
    let cache_dir = dirs.cache.join("mcp");
    StoreConfig {
        channels,
        default_channel: cfg.default_channel,
        uex_dir: Some(tools::uex_dir(&cache_dir)),
        cache_dir,
        our_outputs: outputs.entries.into_iter().map(|e| e.sha256).collect(),
        extractor,
    }
}

/// `new Date().toISOString()`, for logs.
pub fn iso_now() -> String {
    starorg_gamedata::output::iso_now()
}

/// A server over a store.
pub fn server(cfg: StoreConfig, notify: Option<store::Notify>) -> protocol::Server {
    let mut s = store::Store::new(cfg);
    if let Some(n) = notify {
        s = s.with_notify(n);
    }
    protocol::Server::new(tools::Tools::new(s))
}
