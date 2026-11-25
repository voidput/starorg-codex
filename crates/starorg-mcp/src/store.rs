//! Datasets per channel and build, loaded lazily and cached on disk.
//!
//! A tool asks for the datasets it needs by name. The first time a build needs one it is
//! extracted by a worker process ([`crate::worker`]) into
//! `<cache>/<CHANNEL>/<build>-<fingerprint>/<dataset>.json`; afterwards it is read from
//! there. The fingerprint covers the archive's size and modification time and a hash of the
//! code that writes the datasets (`build.rs`, `DATASET_CODE`), so a patched `Data.p4k` or a
//! change to extraction never reads an old answer, while a release that leaves extraction
//! alone keeps the cache. Parsed
//! datasets stay in memory for the two most recently used builds (a diff needs two).
//! `measurements.json` in each build folder keeps what each extraction cost.

use crate::worker::{Extractor, Measurement};
use serde_json::Value;
use sha2::{Digest, Sha256};
use starorg_gamedata::search_index;
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Bumped when the cached layout or a dataset's meaning changes.
pub const CACHE_FORMAT: u32 = 1;

/// Builds kept on disk per channel (older ones are removed; the app cache folder's cap,
/// `starorg_core::storage`, can remove more).
pub const KEEP_BUILDS: usize = 2;

/// Parsed builds kept in memory.
const MEM_BUILDS: usize = 2;

/// The UEX files a store reads ([`Store::uex`]), as the app's "Fetch now" writes them
/// (`starorg_net::uex::FILES`).
pub const UEX_FILES: &[&str] = &[
    "uex-item-prices.json",
    "uex-commodity-prices.json",
    "uex-terminals.json",
];

/// What a file looked like on disk: modified time and size (`None` when missing). A
/// replaced file (Fetch now writes a temp file and renames it) has a new stamp.
pub type FileStamp = Option<(Option<std::time::SystemTime>, u64)>;

fn file_stamp(path: &Path) -> FileStamp {
    let m = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some((m.modified().ok(), m.len()))
}

/// The stamps of every [`UEX_FILES`] entry, in that order: a value derived from UEX files
/// is current while the stamp it was built under equals [`Store::uex_stamp`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UexStamp(pub Vec<FileStamp>);

/// One channel the server can answer for.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelSource {
    /// `LIVE`, `PTU`, ...
    pub name: String,
    pub p4k: PathBuf,
    /// Game version from `build_manifest.id`, e.g. `4.10.200.58746`.
    pub build: Option<String>,
    /// The channel folder in the game install (read only: the loose `global.ini`).
    pub dir: PathBuf,
    /// The app's data folder for this channel (owned blueprints); `None` before the
    /// install is registered.
    pub data_dir: Option<PathBuf>,
}

/// A tool failure the client sees as `isError` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for ToolError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

pub type ToolResult<T> = Result<T, ToolError>;

pub fn tool_err<T>(msg: impl Into<String>) -> ToolResult<T> {
    Err(ToolError(msg.into()))
}

/// Everything the server knows about where data comes from.
pub struct StoreConfig {
    pub channels: Vec<ChannelSource>,
    /// The app's default channel; else LIVE, else the first.
    pub default_channel: Option<String>,
    /// `<app cache>/mcp`.
    pub cache_dir: PathBuf,
    /// Optional UEX price files (`uex-item-prices.json`, `uex-terminals.json`): downloaded by
    /// the app when the user turns that on, or put there by the user; never fetched here.
    pub uex_dir: Option<PathBuf>,
    /// SHA-256 of every `global.ini` StarOrg Codex wrote (`outputs.json`), to tell our
    /// applied text from another tool's.
    pub our_outputs: Vec<String>,
    pub extractor: Arc<dyn Extractor>,
    /// The app cache folder's cap (`starorg_core::storage::enforce` on the folder above
    /// `cache_dir`, after each extraction); `None`: no cap (tests).
    pub cache_cap: Option<u64>,
}

type Typed = Arc<dyn Any + Send + Sync>;

#[derive(Default)]
struct Mem {
    json: HashMap<(PathBuf, String), Arc<Value>>,
    typed: HashMap<(PathBuf, &'static str), Typed>,
    /// The UEX stamp each [`Store::typed_uex`] value was built under.
    typed_uex: HashMap<(PathBuf, &'static str), UexStamp>,
    /// The dataset stamps each [`Store::typed_files`] value was built under.
    typed_files: HashMap<(PathBuf, &'static str), Vec<FileStamp>>,
    /// Build folders, most recent last.
    recent: Vec<PathBuf>,
    /// Modified time of each UEX file in `json` when it was read.
    uex_modified: HashMap<(PathBuf, String), FileStamp>,
}

/// Called with a line worth showing in the app's Activity (extractions).
pub type Notify = Arc<dyn Fn(String) + Send + Sync>;

pub struct Store {
    pub cfg: StoreConfig,
    mem: Mutex<Mem>,
    extracting: Mutex<()>,
    notify: Option<Notify>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Letters, digits, `.`, `-`, `_` only (a path segment that cannot escape the cache).
fn segment(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut cleaned = cleaned.trim_matches('.').to_string();
    while cleaned.contains("..") {
        cleaned = cleaned.replace("..", ".");
    }
    if cleaned.is_empty() {
        "unknown".into()
    } else {
        cleaned
    }
}

/// Suffix of a cached build folder's reading lock, a file beside the folder
/// (`<CHANNEL>/<folder>.reading.lock`): readers hold it shared while they read files there,
/// pruning takes it exclusively. Beside, not inside, so taking it never changes the folder
/// (its time orders [`Store::cached_builds`]) and the folder stays as written.
pub const READING_LOCK: &str = ".reading.lock";

/// The reading lock file of a cached build folder.
pub fn reading_lock(dir: &Path) -> PathBuf {
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(READING_LOCK);
    dir.with_file_name(name)
}

/// Take a shared reading lock of a cached build folder, `None` when the folder is gone or
/// is being pruned. Any number of readers hold it at once; pruning leaves a held folder
/// alone. Never creates the folder.
pub fn hold_build(dir: &Path) -> Option<starorg_core::lock::Lock> {
    if !dir.is_dir() {
        return None;
    }
    let held = starorg_core::lock::try_lock_shared(&reading_lock(dir)).ok()?;
    // Pruned between the check and the lock: nothing left to protect.
    dir.is_dir().then_some(held)
}

/// Mark a cached build folder as used now, for the storage cap (which goes by a folder's
/// newest file): its `measurements.json`, else its first dataset file.
pub fn mark_used(dir: &Path) {
    let m = dir.join("measurements.json");
    let file = if m.is_file() {
        Some(m)
    } else {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| is_dataset_file(n))
            .collect();
        names.sort();
        names.first().map(|n| dir.join(n))
    };
    if let Some(f) = file {
        starorg_core::fsutil::touch(&f);
    }
}

/// The game version of a build folder name (`<build>-<fingerprint>`): the name without its
/// 8-hex fingerprint, e.g. `4.10.200.58746-0a1b2c3d` -> `4.10.200.58746`.
pub fn build_version(folder: &str) -> &str {
    match folder.rsplit_once('-') {
        Some((v, fp))
            if !v.is_empty() && fp.len() == 8 && fp.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            v
        }
        _ => folder,
    }
}

/// Dotted numbers compared as numbers (`4.10` after `4.9`); a part that is not a number
/// counts as 0.
fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '_'])
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// A dataset file of a build folder: `game-*.json`, not the derived search index.
pub fn is_dataset_file(name: &str) -> bool {
    name.strip_suffix(".json")
        .is_some_and(|n| n.starts_with("game-") && n != search_index::NAME)
}

/// Why a cached build folder is not complete, `None` when it is: a dataset its
/// `measurements.json` says was extracted whose file is gone, or no dataset at all.
pub fn incomplete(dir: &Path) -> Option<String> {
    if !dir.is_dir() {
        return Some("the cache folder is gone".into());
    }
    let missing: Vec<String> = Store::measurements(dir)
        .into_keys()
        .filter(|d| d != search_index::NAME && !dir.join(format!("{d}.json")).is_file())
        .map(|d| format!("{d}.json"))
        .collect();
    if !missing.is_empty() {
        return Some(format!(
            "{} {} missing from the cache (removed after it was read)",
            missing.join(", "),
            if missing.len() == 1 { "is" } else { "are" }
        ));
    }
    let any = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_str().is_some_and(is_dataset_file));
    (!any).then(|| "nothing was extracted for this build before it was replaced".into())
}

impl Store {
    pub fn new(cfg: StoreConfig) -> Self {
        Self {
            cfg,
            mem: Mutex::default(),
            extracting: Mutex::new(()),
            notify: None,
        }
    }

    pub fn with_notify(mut self, notify: Notify) -> Self {
        self.notify = Some(notify);
        self
    }

    fn say(&self, msg: String) {
        if let Some(n) = &self.notify {
            n(msg);
        }
    }

    pub fn channel_names(&self) -> Vec<String> {
        self.cfg.channels.iter().map(|c| c.name.clone()).collect()
    }

    pub fn default_channel(&self) -> Option<&ChannelSource> {
        let by = |n: &str| {
            self.cfg
                .channels
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(n))
        };
        self.cfg
            .default_channel
            .as_deref()
            .and_then(by)
            .or_else(|| by("LIVE"))
            .or_else(|| self.cfg.channels.first())
    }

    /// The named channel, or the default one.
    pub fn channel(&self, name: Option<&str>) -> ToolResult<&ChannelSource> {
        match name.map(str::trim).filter(|n| !n.is_empty()) {
            None => self
                .default_channel()
                .ok_or_else(|| ToolError("no Star Citizen channel found on this machine".into())),
            Some(n) => self
                .cfg
                .channels
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(n))
                .ok_or_else(|| {
                    ToolError(format!(
                        "no channel {n} (have: {})",
                        self.channel_names().join(", ")
                    ))
                }),
        }
    }

    /// `<cache>/<CHANNEL>`.
    pub fn channel_cache(&self, ch: &ChannelSource) -> PathBuf {
        self.cfg
            .cache_dir
            .join(segment(&ch.name.to_ascii_uppercase()))
    }

    /// The archive's identity: size, modification time, cache format and the hash of the
    /// dataset code (not the app version: a release alone does not extract again).
    pub fn fingerprint(p4k: &Path) -> String {
        let meta = std::fs::metadata(p4k).ok();
        let len = meta.as_ref().map_or(0, std::fs::Metadata::len);
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let mut h = Sha256::new();
        h.update(format!(
            "{len}:{mtime}:{CACHE_FORMAT}:{}",
            env!("DATASET_CODE")
        ));
        h.finalize()[..4]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// `<cache>/<CHANNEL>/<build>-<fingerprint>` for the channel's current archive.
    pub fn build_dir(&self, ch: &ChannelSource) -> PathBuf {
        let build = segment(ch.build.as_deref().unwrap_or("unknown"));
        self.channel_cache(ch)
            .join(format!("{build}-{}", Self::fingerprint(&ch.p4k)))
    }

    /// Build folders cached for a channel, newest first: `(folder name, path)`.
    pub fn cached_builds(&self, ch: &ChannelSource) -> Vec<(String, PathBuf)> {
        let mut v: Vec<(std::time::SystemTime, String, PathBuf)> =
            std::fs::read_dir(self.channel_cache(ch))
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| {
                    let t = e
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    (t, e.file_name().to_string_lossy().into_owned(), e.path())
                })
                .collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.0));
        v.into_iter().map(|(_, n, p)| (n, p)).collect()
    }

    /// The channel's cached builds other than its current one ([`Self::build_dir`]), newest
    /// version first (folders of one version: most recently written first). Read only:
    /// nothing is extracted into them.
    pub fn older_builds(&self, ch: &ChannelSource) -> Vec<(String, PathBuf)> {
        let current = self.build_dir(ch);
        let mut v: Vec<(String, PathBuf)> = self
            .cached_builds(ch)
            .into_iter()
            .filter(|(n, p)| *p != current && !n.contains(".tmp-"))
            .collect();
        // Stable: within one version the newest-written order of `cached_builds` stays.
        v.sort_by_key(|(n, _)| std::cmp::Reverse(version_key(build_version(n))));
        v
    }

    fn prune_old_builds(&self, ch: &ChannelSource, current: &Path) {
        for (_, p) in self.cached_builds(ch).into_iter().skip(KEEP_BUILDS) {
            if p == current {
                continue;
            }
            // A folder read right now (an older build in the app's Diff) stays; the next
            // new build prunes it. Held across the removal, so no reader starts meanwhile.
            let lock = reading_lock(&p);
            let Ok(held) = starorg_core::lock::try_lock(&lock) else {
                continue;
            };
            let removed = std::fs::remove_dir_all(&p).is_ok();
            // Released first: Windows does not delete a file that is still open. A reader
            // that locks it meanwhile finds the folder gone (`hold_build` checks again).
            drop(held);
            if removed {
                let _ = std::fs::remove_file(&lock);
            }
        }
        // Reading locks whose folder is gone (removed by the storage cap, say).
        for e in std::fs::read_dir(self.channel_cache(ch))
            .into_iter()
            .flatten()
            .flatten()
        {
            let lock = e.path();
            let Some(folder) = e
                .file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(READING_LOCK))
                .map(|n| lock.with_file_name(n))
            else {
                continue;
            };
            if lock.is_file()
                && !folder.exists()
                && let Ok(held) = starorg_core::lock::try_lock(&lock)
            {
                drop(held);
                let _ = std::fs::remove_file(&lock);
            }
        }
    }

    fn touch(&self, dir: &Path) {
        let mut m = lock(&self.mem);
        m.recent.retain(|d| d != dir);
        m.recent.push(dir.to_path_buf());
        while m.recent.len() > MEM_BUILDS {
            let old = m.recent.remove(0);
            m.json.retain(|(d, _), _| *d != old);
            m.typed.retain(|(d, _), _| *d != old);
            m.typed_uex.retain(|(d, _), _| *d != old);
            m.typed_files.retain(|(d, _), _| *d != old);
        }
    }

    pub fn measurements(dir: &Path) -> BTreeMap<String, Measurement> {
        std::fs::read(dir.join("measurements.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn record_measurement(dir: &Path, m: &Measurement) {
        let mut all = Self::measurements(dir);
        all.insert(m.dataset.clone(), m.clone());
        // Temp file and rename: a run stopped half-way never leaves a torn file.
        if let Ok(text) = serde_json::to_string_pretty(&all) {
            let tmp = dir.join(format!("measurements.json.tmp-{}", std::process::id()));
            if std::fs::write(&tmp, text).is_err()
                || std::fs::rename(&tmp, dir.join("measurements.json")).is_err()
            {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    /// Make sure `<dir>/<dataset>.json` exists for the channel's current build.
    fn ensure(&self, ch: &ChannelSource, dir: &Path, dataset: &str) -> ToolResult<()> {
        let file = dir.join(format!("{dataset}.json"));
        if file.is_file() {
            return Ok(());
        }
        if dataset == search_index::NAME {
            self.ensure(ch, dir, "game-item-tags")?;
            self.ensure(ch, dir, "game-ships")?;
            let _g = lock(&self.extracting);
            if !file.is_file() {
                search_index::write_search_index(dir)
                    .map_err(|e| ToolError(format!("building the search index: {e}")))?;
            }
            return Ok(());
        }
        let _g = lock(&self.extracting);
        if file.is_file() {
            return Ok(());
        }
        let fresh = !dir.exists();
        std::fs::create_dir_all(dir).map_err(|e| ToolError(format!("{}: {e}", dir.display())))?;
        if fresh {
            self.prune_old_builds(ch, dir);
        }
        self.say(format!(
            "MCP: extracting {dataset} for {} ({}), once per build",
            ch.name,
            ch.build.as_deref().unwrap_or("unknown build")
        ));
        let m = self
            .cfg
            .extractor
            .extract(&ch.p4k, dataset, dir)
            .map_err(|e| ToolError(format!("{dataset}: {e}")))?;
        Self::record_measurement(dir, &m);
        if let (Some(cap), Some(app_cache)) = (self.cfg.cache_cap, self.cfg.cache_dir.parent()) {
            let e = starorg_core::storage::enforce(app_cache, cap, &[dir.to_path_buf()]);
            if !e.removed.is_empty() {
                self.say(format!(
                    "Storage cap {} MB: removed {} older folder(s), {} MB",
                    cap >> 20,
                    e.removed.len(),
                    (e.before - e.after) >> 20
                ));
            }
        }
        self.say(format!(
            "MCP: {dataset} ready in {:.1} s{}",
            m.seconds,
            m.peak_rss_bytes
                .map(|b| format!(", worker peak {} MB", b / (1 << 20)))
                .unwrap_or_default()
        ));
        if !file.is_file() {
            return tool_err(format!("{dataset}: the worker wrote no file"));
        }
        Ok(())
    }

    fn read_json(path: &Path) -> ToolResult<Arc<Value>> {
        let bytes =
            std::fs::read(path).map_err(|e| ToolError(format!("{}: {e}", path.display())))?;
        serde_json::from_slice(&bytes)
            .map(Arc::new)
            .map_err(|e| ToolError(format!("{}: {e}", path.display())))
    }

    /// A dataset of the channel's current build, extracted on first use.
    pub fn json(&self, ch: &ChannelSource, dataset: &str) -> ToolResult<Arc<Value>> {
        let dir = self.build_dir(ch);
        self.json_in(ch, &dir, dataset, true)
    }

    /// A dataset in a given build folder. `extract`: fill it when missing (current build
    /// only; an older cached build has only what was extracted then).
    pub fn json_in(
        &self,
        ch: &ChannelSource,
        dir: &Path,
        dataset: &str,
        extract: bool,
    ) -> ToolResult<Arc<Value>> {
        self.touch(dir);
        let key = (dir.to_path_buf(), dataset.to_string());
        if let Some(v) = lock(&self.mem).json.get(&key) {
            return Ok(v.clone());
        }
        if extract {
            self.ensure(ch, dir, dataset)?;
        }
        let path = dir.join(format!("{dataset}.json"));
        if !path.is_file() {
            return tool_err(format!(
                "{dataset} is not cached for {}",
                dir.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let v = Self::read_json(&path)?;
        // Last used, for the storage cap's LRU order.
        starorg_core::fsutil::touch(&path);
        lock(&self.mem).json.insert(key, v.clone());
        Ok(v)
    }

    /// A value derived from datasets, built once per build folder.
    pub fn typed<T: Any + Send + Sync>(
        &self,
        dir: &Path,
        name: &'static str,
        build: impl FnOnce() -> ToolResult<T>,
    ) -> ToolResult<Arc<T>> {
        self.touch(dir);
        let key = (dir.to_path_buf(), name);
        if let Some(v) = lock(&self.mem).typed.get(&key)
            && let Ok(t) = v.clone().downcast::<T>()
        {
            return Ok(t);
        }
        let v = Arc::new(build()?);
        lock(&self.mem).typed.insert(key, v.clone());
        Ok(v)
    }

    /// The current [`UexStamp`] (file metadata only; empty without a UEX folder).
    pub fn uex_stamp(&self) -> UexStamp {
        match self.cfg.uex_dir.as_ref() {
            Some(dir) => UexStamp(UEX_FILES.iter().map(|f| file_stamp(&dir.join(f))).collect()),
            None => UexStamp::default(),
        }
    }

    /// [`Self::typed`] for a value derived from UEX files (shop rows, obtain hints, a shops
    /// index): built again once any UEX file changed since ([`Self::uex_stamp`]), so a
    /// "Fetch now" while this store runs counts at the next call, no restart needed.
    pub fn typed_uex<T: Any + Send + Sync>(
        &self,
        dir: &Path,
        name: &'static str,
        build: impl FnOnce() -> ToolResult<T>,
    ) -> ToolResult<Arc<T>> {
        let stamp = self.uex_stamp();
        let key = (dir.to_path_buf(), name);
        let current = lock(&self.mem).typed_uex.get(&key) == Some(&stamp);
        if !current {
            let mut m = lock(&self.mem);
            m.typed.remove(&key);
            m.typed_uex.remove(&key);
        }
        let v = self.typed(dir, name, build)?;
        lock(&self.mem).typed_uex.insert(key, stamp);
        Ok(v)
    }

    /// [`Self::typed`] for a value derived from some of the build's datasets that may be
    /// extracted (or extracted again) while this store runs: built again once any of
    /// `datasets` (`game-mining`, ...) appeared, went or changed (modified time and size)
    /// in `dir` since.
    pub fn typed_files<T: Any + Send + Sync>(
        &self,
        dir: &Path,
        name: &'static str,
        datasets: &[&str],
        build: impl FnOnce() -> ToolResult<T>,
    ) -> ToolResult<Arc<T>> {
        let stamp: Vec<FileStamp> = datasets
            .iter()
            .map(|d| file_stamp(&dir.join(format!("{d}.json"))))
            .collect();
        let key = (dir.to_path_buf(), name);
        let current = lock(&self.mem).typed_files.get(&key) == Some(&stamp);
        if !current {
            let mut m = lock(&self.mem);
            m.typed.remove(&key);
            m.typed_files.remove(&key);
        }
        let v = self.typed(dir, name, build)?;
        lock(&self.mem).typed_files.insert(key, stamp);
        Ok(v)
    }

    /// Optional UEX file ([`UEX_FILES`]), never fetched here. The app may replace it while
    /// this store runs (Settings -> "Download UEX market data"), so a file whose modified
    /// time or size changed is read again.
    pub fn uex(&self, file: &str) -> Option<Arc<Value>> {
        let dir = self.cfg.uex_dir.as_ref()?;
        let path = dir.join(file);
        let modified = file_stamp(&path);
        modified?;
        let key = (dir.clone(), file.to_string());
        {
            let m = lock(&self.mem);
            if let Some(v) = m.json.get(&key)
                && m.uex_modified.get(&key) == Some(&modified)
            {
                return Some(v.clone());
            }
        }
        let v = Self::read_json(&path).ok()?;
        let mut m = lock(&self.mem);
        m.json.insert(key.clone(), v.clone());
        m.uex_modified.insert(key, modified);
        Some(v)
    }

    /// Datasets parsed in memory right now (for `status`).
    pub fn loaded(&self) -> Vec<String> {
        let m = lock(&self.mem);
        let mut v: Vec<String> = m
            .json
            .keys()
            .map(|(d, n)| {
                format!(
                    "{}/{n}",
                    d.file_name().unwrap_or_default().to_string_lossy()
                )
            })
            .collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Set a folder's modified time. Windows opens a folder only with write access and
    /// `FILE_FLAG_BACKUP_SEMANTICS`; a plain `File::open` is refused there (os error 5).
    fn set_dir_mtime(dir: &Path, t: std::time::SystemTime) {
        let mut o = std::fs::File::options();
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.write(true).custom_flags(0x0200_0000);
        }
        #[cfg(not(windows))]
        o.read(true);
        o.open(dir).unwrap().set_modified(t).unwrap();
    }

    struct Fake(AtomicUsize);
    impl Extractor for Fake {
        fn extract(&self, _p4k: &Path, dataset: &str, out: &Path) -> Result<Measurement, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::fs::write(out.join(format!("{dataset}.json")), r#"{"x": 1}"#).unwrap();
            Ok(Measurement {
                dataset: dataset.into(),
                seconds: 0.1,
                peak_rss_bytes: Some(1 << 20),
                bytes: 8,
                warnings: 0,
            })
        }
    }

    fn store(dir: &Path, fake: Arc<Fake>) -> Store {
        let p4k = dir.join("LIVE/Data.p4k");
        std::fs::create_dir_all(p4k.parent().unwrap()).unwrap();
        // Written once: a second store over the same dir must see the same archive (the
        // fingerprint keys on its modified second; rewriting it could cross a second).
        if !p4k.exists() {
            std::fs::write(&p4k, b"p4k").unwrap();
        }
        Store::new(StoreConfig {
            channels: vec![
                ChannelSource {
                    name: "LIVE".into(),
                    p4k: p4k.clone(),
                    build: Some("4.10.200.58746".into()),
                    dir: p4k.parent().unwrap().into(),
                    data_dir: None,
                },
                ChannelSource {
                    name: "PTU".into(),
                    p4k,
                    build: Some("../../evil".into()),
                    dir: dir.into(),
                    data_dir: None,
                },
            ],
            default_channel: Some("PTU".into()),
            cache_dir: dir.join("cache"),
            uex_dir: None,
            our_outputs: vec![],
            extractor: fake,
            cache_cap: None,
        })
    }

    #[test]
    fn extracts_once_then_serves_from_disk_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let s = store(dir.path(), fake.clone());
        let live = s.channel(Some("live")).unwrap();
        assert_eq!(s.json(live, "game-missions").unwrap()["x"], 1);
        assert_eq!(s.json(live, "game-missions").unwrap()["x"], 1);
        assert_eq!(fake.0.load(Ordering::SeqCst), 1);
        let d = s.build_dir(live);
        assert!(d.starts_with(dir.path().join("cache").join("LIVE")));
        assert!(Store::measurements(&d).contains_key("game-missions"));
        // A second store over the same cache reads the file, no new extraction.
        let s2 = store(dir.path(), fake.clone());
        s2.json(s2.channel(Some("LIVE")).unwrap(), "game-missions")
            .unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_replaced_uex_file_is_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(dir.path(), Arc::new(Fake(AtomicUsize::new(0))));
        let uex = dir.path().join("uex");
        std::fs::create_dir_all(&uex).unwrap();
        s.cfg.uex_dir = Some(uex.clone());
        assert!(s.uex("uex-terminals.json").is_none());
        let f = uex.join("uex-terminals.json");
        std::fs::write(&f, r#"{"n": 1}"#).unwrap();
        assert_eq!(s.uex("uex-terminals.json").unwrap()["n"], 1);
        std::fs::write(&f, r#"{"n": 2}"#).unwrap();
        // Same second on a coarse clock: set a later time so the change is seen.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(s.uex("uex-terminals.json").unwrap()["n"], 2);
    }

    #[test]
    fn uex_derived_values_are_built_again_once_a_uex_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(dir.path(), Arc::new(Fake(AtomicUsize::new(0))));
        let uex = dir.path().join("uex");
        std::fs::create_dir_all(&uex).unwrap();
        s.cfg.uex_dir = Some(uex.clone());
        let builds = AtomicUsize::new(0);
        let get = || {
            *s.typed_uex(
                dir.path(),
                "x",
                || Ok(builds.fetch_add(1, Ordering::SeqCst)),
            )
            .unwrap()
        };
        assert_eq!(get(), 0);
        assert_eq!(get(), 0, "unchanged files: kept");
        // A file appears, then is replaced: built again each time.
        let f = uex.join("uex-commodity-prices.json");
        std::fs::write(&f, "{}").unwrap();
        assert_eq!(get(), 1);
        assert_eq!(get(), 1);
        std::fs::write(&f, "{\"a\": 1}").unwrap();
        assert_eq!(get(), 2, "a new size counts even within the same second");
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(get(), 3);
        std::fs::remove_file(&f).unwrap();
        assert_eq!(get(), 4);
        // Plain typed values do not care.
        let n = AtomicUsize::new(0);
        let plain = || {
            *s.typed(dir.path(), "y", || Ok(n.fetch_add(1, Ordering::SeqCst)))
                .unwrap()
        };
        assert_eq!(plain(), 0);
        std::fs::write(&f, "{}").unwrap();
        assert_eq!(plain(), 0);
        assert_eq!(s.uex_stamp().0.len(), UEX_FILES.len());
    }

    #[test]
    fn older_builds_leave_out_the_current_one_newest_version_first() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path(), Arc::new(Fake(AtomicUsize::new(0))));
        let live = s.channel(Some("LIVE")).unwrap();
        let current = s.build_dir(live);
        let ch = s.channel_cache(live);
        for d in [
            current.clone(),
            ch.join("4.9.0.100-0a1b2c3d"),
            ch.join("4.10.197.2878-deadbeef"),
            ch.join("x.tmp-1"),
        ] {
            std::fs::create_dir_all(&d).unwrap();
        }
        let names: Vec<String> = s.older_builds(live).into_iter().map(|b| b.0).collect();
        assert_eq!(names, ["4.10.197.2878-deadbeef", "4.9.0.100-0a1b2c3d"]);
        assert_eq!(build_version("4.10.197.2878-deadbeef"), "4.10.197.2878");
        assert_eq!(build_version("unknown"), "unknown");
        assert_eq!(build_version("a-b"), "a-b");
    }

    #[test]
    fn incomplete_says_why_and_pruning_skips_a_folder_being_read() {
        let dir = tempfile::tempdir().unwrap();
        let b = dir.path().join("b");
        std::fs::create_dir_all(&b).unwrap();
        assert!(incomplete(&b).unwrap().contains("nothing was extracted"));
        std::fs::write(b.join("game-ships.json"), "{}").unwrap();
        assert_eq!(incomplete(&b), None);
        Store::record_measurement(
            &b,
            &Measurement {
                dataset: "game-weapons".into(),
                seconds: 1.0,
                peak_rss_bytes: None,
                bytes: 2,
                warnings: 0,
            },
        );
        assert!(
            incomplete(&b)
                .unwrap()
                .starts_with("game-weapons.json is missing")
        );
        assert!(incomplete(&dir.path().join("gone")).is_some());
        assert!(is_dataset_file("game-ships.json"));
        assert!(!is_dataset_file("game-search-index.json"));
        assert!(!is_dataset_file("measurements.json"));
        // Held: pruning leaves it; released: it goes.
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let s = store(dir.path(), fake);
        let live = s.channel(Some("LIVE")).unwrap();
        let ch = s.channel_cache(live);
        let old = ch.join("4.9.0.1-00000000");
        let older = ch.join("4.8.0.1-00000000");
        std::fs::create_dir_all(&older).unwrap();
        std::fs::create_dir_all(&old).unwrap();
        let ago = |s| std::time::SystemTime::now() - std::time::Duration::from_secs(s);
        set_dir_mtime(&older, ago(7200));
        set_dir_mtime(&old, ago(3600));
        // Both older folders held: none goes.
        let held = [hold_build(&older).unwrap(), hold_build(&old).unwrap()];
        let second = hold_build(&older).expect("readers share the lock");
        assert!(hold_build(&dir.path().join("none")).is_none());
        assert_eq!(
            reading_lock(&older),
            ch.join("4.8.0.1-00000000.reading.lock")
        );
        assert_eq!(
            std::fs::read_dir(&older).unwrap().count(),
            0,
            "the lock is beside the folder, not in it"
        );
        let current = s.build_dir(live);
        std::fs::create_dir_all(&current).unwrap();
        s.prune_old_builds(live, &current);
        assert!(older.is_dir() && old.is_dir(), "being read");
        drop(held);
        s.prune_old_builds(live, &current);
        assert!(older.is_dir(), "the second reader still holds it");
        assert_eq!(s.cached_builds(live).len(), KEEP_BUILDS + 1);
        drop(second);
        // A child another test starts can share the just-dropped lock for an instant
        // (between fork and exec): prune again until it is free.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            s.prune_old_builds(live, &current);
            if s.cached_builds(live).len() == KEEP_BUILDS || std::time::Instant::now() > until {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(s.cached_builds(live).len(), KEEP_BUILDS);
        assert!(current.is_dir());
        let left: Vec<String> = std::fs::read_dir(&ch)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(READING_LOCK))
            .collect();
        let kept: Vec<String> = s.cached_builds(live).into_iter().map(|b| b.0).collect();
        assert!(
            left.iter()
                .all(|n| kept.iter().any(|k| *n == format!("{k}{READING_LOCK}"))),
            "the pruned folder's lock goes with it: {left:?}"
        );
    }

    #[test]
    fn a_reading_lock_keeps_the_prune_order() {
        // Reading the older build must not make it look newer than the current one.
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path(), Arc::new(Fake(AtomicUsize::new(0))));
        let live = s.channel(Some("LIVE")).unwrap();
        let ch = s.channel_cache(live);
        let old = ch.join("4.8.0.1-00000000");
        std::fs::create_dir_all(&old).unwrap();
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        set_dir_mtime(&old, past);
        let current = s.build_dir(live);
        std::fs::create_dir_all(&current).unwrap();
        let _held = hold_build(&old).unwrap();
        let order: Vec<PathBuf> = s.cached_builds(live).into_iter().map(|b| b.1).collect();
        assert_eq!(order, [current, old]);
    }

    #[test]
    fn mark_used_touches_a_file_with_or_without_measurements() {
        let dir = tempfile::tempdir().unwrap();
        let b = dir.path().join("b");
        std::fs::create_dir_all(&b).unwrap();
        let f = b.join("game-ships.json");
        std::fs::write(&f, "{}").unwrap();
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(past)
            .unwrap();
        mark_used(&b);
        let t = std::fs::metadata(&f).unwrap().modified().unwrap();
        assert!(
            t > past,
            "no measurements.json: the dataset file is touched"
        );
        assert!(!b.join("measurements.json").exists(), "nothing is created");
    }

    #[test]
    fn default_channel_and_safe_paths() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(dir.path(), Arc::new(Fake(AtomicUsize::new(0))));
        assert_eq!(s.channel(None).unwrap().name, "PTU");
        assert!(s.channel(Some("TECH-PREVIEW")).is_err());
        let ptu = s.channel(Some("PTU")).unwrap();
        let d = s.build_dir(ptu);
        assert!(d.starts_with(dir.path().join("cache").join("PTU")));
        assert!(!d.to_string_lossy().contains(".."));
    }
}
