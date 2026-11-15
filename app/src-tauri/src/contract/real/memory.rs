//! The memory limit of the Developer mode and MCP workers in the real backend (addendum
//! 7.17): `[dev] memory_limit_gb` in `config.toml`, read at every worker start, so a change
//! applies to the next worker (a running one keeps the cap it started with).

use super::{RealBackend, io};
use crate::contract::types::*;
use starorg_core::config::Config;
use starorg_core::memlimit;
use std::process::Command;

impl RealBackend {
    /// The limit in bytes for the next worker.
    pub(super) fn memory_limit(&self) -> Option<u64> {
        let cfg = Config::load(self.dirs()).unwrap_or_default();
        Some(memlimit::gb_bytes(memlimit::limit_gb(
            &cfg,
            memlimit::meminfo().total,
        )))
    }

    /// `cmd` under the limit (see `starorg_core::memlimit::apply`).
    pub(super) fn capped(&self, cmd: Command) -> Command {
        memlimit::apply(cmd, self.memory_limit())
    }

    /// The MCP server's per-dataset limit: read from `config.toml` at every extraction.
    pub(super) fn mcp_limit(&self) -> starorg_mcp::worker::LimitFn {
        let dirs = self.dirs().clone();
        std::sync::Arc::new(move || {
            let cfg = Config::load(&dirs).unwrap_or_default();
            Some(memlimit::gb_bytes(memlimit::limit_gb(
                &cfg,
                memlimit::meminfo().total,
            )))
        })
    }

    pub(super) fn memory_view(&self) -> MemoryStatus {
        let cfg = Config::load(self.dirs()).unwrap_or_default();
        MemoryStatus::build(
            memlimit::configured_gb(&cfg),
            memlimit::meminfo(),
            MemoryEnforcement::here(),
        )
    }

    pub(super) fn save_memory_limit(&self, req: MemoryLimitRequest) -> CmdResult<MemoryStatus> {
        let mut cfg = Config::load(self.dirs()).map_err(io)?;
        let b = memlimit::bounds(memlimit::meminfo().total);
        let gb = req.gb.map(|g| b.clamp(g));
        memlimit::set_configured_gb(&mut cfg, gb);
        cfg.save(self.dirs()).map_err(io)?;
        self.log(
            LogLevel::Info,
            Some(JobKind::Dev),
            None,
            &match gb {
                Some(g) => format!("Memory limit for Developer mode and MCP workers: {g} GB"),
                None => format!("Memory limit back to the default ({} GB)", b.default_gb),
            },
        );
        Ok(self.memory_view())
    }
}
