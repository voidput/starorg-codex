//! Settings > Storage in the real backend: sizes per area (`starorg_core::storage`), the
//! cache folder's cap, "Clear asset cache", and the start-up housekeeping (leftover
//! extraction folders, the cap).
//!
//! Measuring walks every folder, so the sizes are kept for the session ([`Sizes`]) and
//! measured again only after something may have changed them: a job ended (apply, read,
//! import, restore, download, export), an asset call used the asset cache, the asset cache
//! was cleared, moved or switched, a backup was made or deleted, start-up housekeeping, or
//! "Measure again". Other programs writing there (the MCP server, a launch hook, the CLI) are
//! picked up after [`SIZES_TTL`].

use super::{RealBackend, err};
use crate::contract::assets::AssetBackend;
use crate::contract::types::*;
use starorg_core::assetcache::{self, AssetCache, Usage};
use starorg_core::config::Config;
use starorg_core::source::local::{WORK_STALE, sweep_work};
use starorg_core::storage;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Sizes measured by another program (not this app) show after at most this long.
pub const SIZES_TTL: Duration = Duration::from_secs(5 * 60);

/// Measured sizes for the session: the Storage report per asset cache root, and the asset
/// cache usage per folder. An entry counts while [`Sizes::changed`] was not called since it
/// was measured (`stamp`) and it is younger than [`SIZES_TTL`].
#[derive(Default)]
pub struct Sizes {
    stamp: AtomicU64,
    memo: Mutex<SizeMemo>,
}

#[derive(Default)]
struct SizeMemo {
    report: Option<(u64, Instant, PathBuf, storage::Report)>,
    usage: HashMap<PathBuf, (u64, Instant, Usage)>,
}

impl Sizes {
    /// Something may have written to or removed from the measured folders.
    pub fn changed(&self) {
        self.stamp.fetch_add(1, Ordering::SeqCst);
    }

    fn now(&self) -> u64 {
        self.stamp.load(Ordering::SeqCst)
    }

    fn memo(&self) -> std::sync::MutexGuard<'_, SizeMemo> {
        self.memo.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn report(&self, root: &std::path::Path) -> Option<storage::Report> {
        let g = self.now();
        let m = self.memo();
        let (rg, at, r, rep) = m.report.as_ref()?;
        (*rg == g && at.elapsed() < SIZES_TTL && r == root).then(|| rep.clone())
    }

    fn usage(&self, root: &std::path::Path) -> Option<Usage> {
        let g = self.now();
        let m = self.memo();
        let (ug, at, u) = m.usage.get(root)?;
        (*ug == g && at.elapsed() < SIZES_TTL).then_some(*u)
    }

    /// Keep what was measured since `g`, unless something changed meanwhile.
    fn keep_report(&self, g: u64, root: PathBuf, r: &storage::Report) {
        if self.now() != g {
            return;
        }
        let mut m = self.memo();
        // The report holds the asset cache's usage too: the cache row need not walk it.
        if let Some(a) = r
            .areas
            .iter()
            .find(|a| a.area == storage::Area::AssetExports)
        {
            m.usage.insert(root.clone(), (g, Instant::now(), a.usage));
        }
        m.report = Some((g, Instant::now(), root, r.clone()));
    }

    fn keep_usage(&self, g: u64, root: PathBuf, u: Usage) {
        if self.now() != g {
            return;
        }
        let mut m = self.memo();
        m.usage.retain(|_, (ug, _, _)| *ug == g);
        m.usage.insert(root, (g, Instant::now(), u));
    }
}

impl RealBackend {
    /// The measured sizes may be out of date now (see the module notes).
    pub(super) fn sizes_changed(&self) {
        self.assets.sizes.changed();
    }

    /// The Storage report, measured again only when needed. While an asset call uses the
    /// cache, every call measures (the numbers grow during an export).
    fn storage_report(&self, assets: &AssetCache) -> storage::Report {
        let busy = self.asset_cache_busy().is_some();
        let sizes = &self.assets.sizes;
        if !busy && let Some(r) = sizes.report(assets.root()) {
            return r;
        }
        let g = sizes.now();
        let r = storage::report(self.dirs(), assets);
        if !busy {
            sizes.keep_report(g, assets.root().to_path_buf(), &r);
        }
        r
    }

    /// Files and bytes in an asset cache folder, measured again only when needed.
    pub(super) fn asset_usage(&self, c: &AssetCache) -> Usage {
        let busy = self.asset_cache_busy().is_some();
        let sizes = &self.assets.sizes;
        if !busy && let Some(u) = sizes.usage(c.root()) {
            return u;
        }
        let g = sizes.now();
        let u = c.usage();
        if !busy {
            sizes.keep_usage(g, c.root().to_path_buf(), u);
        }
        u
    }

    /// "Measure again": forget what was measured, then measure.
    pub(super) fn storage_measure_now(&self) -> StorageStatus {
        self.sizes_changed();
        self.storage_view()
    }

    pub(super) fn storage_view(&self) -> StorageStatus {
        let cfg = Config::load(self.dirs()).unwrap_or_default();
        let assets = assetcache::resolve(self.dirs(), &cfg, None);
        let r = self.storage_report(&assets);
        let inside = assets.root().starts_with(&self.dirs().cache);
        StorageStatus::build(
            &r,
            storage::configured_cap(&cfg),
            assets.max_bytes(),
            inside,
            self.asset_cache_busy(),
        )
    }

    pub(super) fn storage_clear_asset_cache(&self) -> CmdResult<StorageStatus> {
        if let Some(why) = self.asset_cache_busy() {
            return Err(err(ErrorCode::Busy, why));
        }
        self.asset_cache_clear()?;
        Ok(self.storage_view())
    }

    /// At start (off the UI thread): remove `work/starorg-extract-*` leftovers older than
    /// an hour that no running load holds, then bring the cache folder under its cap.
    pub fn storage_housekeeping(&self) {
        let gone = sweep_work(&self.dirs().cache.join("work"), WORK_STALE);
        if !gone.is_empty() {
            self.log(
                LogLevel::Info,
                None,
                None,
                &format!(
                    "Removed {} leftover extraction folder(s) from an earlier run",
                    gone.len()
                ),
            );
        }
        let e = storage::enforce_configured(self.dirs(), &[]);
        self.sizes_changed();
        if !e.removed.is_empty() {
            self.log(
                LogLevel::Info,
                None,
                None,
                &format!(
                    "Cache folder over its {} MB cap: removed {} least recently used item(s), {} MB",
                    e.cap >> 20,
                    e.removed.len(),
                    (e.before - e.after) >> 20
                ),
            );
        }
    }
}
