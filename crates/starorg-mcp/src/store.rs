//! Datasets per channel and build, loaded lazily and cached on disk.
//!
//! A tool asks for the datasets it needs by name. The first time a build needs one it is
//! extracted by a worker process ([`crate::worker`]) into
//! `<cache>/<CHANNEL>/<build>-<fingerprint>/<dataset>.json`; afterwards it is read from
//! there. The fingerprint covers the archive's size and modification time and this crate's
//! version, so a patched `Data.p4k` or an app update never reads an old answer. Parsed
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

/// Builds kept on disk per channel (older ones are removed).
pub const KEEP_BUILDS: usize = 3;

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
}

type Typed = Arc<dyn Any + Send + Sync>;

#[derive(Default)]
struct Mem {
    json: HashMap<(PathBuf, String), Arc<Value>>,
    typed: HashMap<(PathBuf, &'static str), Typed>,
    /// The UEX stamp each [`Store::typed_uex`] value was built under.
    typed_uex: HashMap<(PathBuf, &'static str), UexStamp>,
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

    /// The archive's identity: size, modification time, cache format, crate version.
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
            env!("CARGO_PKG_VERSION")
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

    fn prune_old_builds(&self, ch: &ChannelSource, current: &Path) {
        for (_, p) in self.cached_builds(ch).into_iter().skip(KEEP_BUILDS) {
            if p != current {
                let _ = std::fs::remove_dir_all(&p);
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
        if let Ok(text) = serde_json::to_string_pretty(&all) {
            let _ = std::fs::write(dir.join("measurements.json"), text);
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
