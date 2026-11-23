//! Settings > Storage in the real backend: sizes per area (`starorg_core::storage`), the
//! cache folder's cap, "Clear asset cache", and the start-up housekeeping (leftover
//! extraction folders, the cap).

use super::{RealBackend, err};
use crate::contract::assets::AssetBackend;
use crate::contract::types::*;
use starorg_core::assetcache;
use starorg_core::config::Config;
use starorg_core::source::local::{WORK_STALE, sweep_work};
use starorg_core::storage;

impl RealBackend {
    pub(super) fn storage_view(&self) -> StorageStatus {
        let cfg = Config::load(self.dirs()).unwrap_or_default();
        let assets = assetcache::resolve(self.dirs(), &cfg, None);
        let r = storage::report(self.dirs(), &assets);
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
