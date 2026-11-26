//! Datasets per channel and build, loaded lazily and cached on disk.
//!
//! A tool asks for the datasets it needs by name. The first time a build needs one it is
//! extracted by a worker process ([`crate::worker`]) into
//! `<cache>/<CHANNEL>/<build>-<fingerprint>/<dataset>.json` (or, in the app with the setting
//! on, taken from a verified prebuilt dump of the same build, [`crate::prebuilt`]);
//! afterwards it is read from there. The fingerprint covers the archive's size and
//! modification time and the cache format, so a patched `Data.p4k` gets a folder of its own.
//! The code that wrote each dataset is in its stamp ([`crate::stamps`]): a dataset is served
//! only while its stamp names this app's code for it, so an app update extracts again only
//! the datasets whose code changed and keeps the rest. Parsed datasets stay in memory for
//! the two most recently used builds (a diff needs two). `measurements.json` in each build
//! folder keeps what each extraction cost.
//!
//! After a dataset is written the channel's cache is tidied once per run ([`Store::tidy`]):
//! other folders of the same build (an older archive, or the layout before stamps), datasets
//! of other code in the current folder, and builds older than the previous one go.

use crate::stamps;
use crate::worker::{Extractor, Measurement};
use serde_json::Value;
use sha2::{Digest, Sha256};
use starorg_gamedata::search_index;
use std::any::Any;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Bumped when the cached layout or a dataset's meaning changes. 2: per-dataset stamps; the
/// folder fingerprint no longer carries the dataset code.
pub const CACHE_FORMAT: u32 = 2;

/// Builds kept on disk per channel: the current one and the previous one (older ones are
/// removed; the app cache folder's cap, `starorg_core::storage`, can remove more).
pub const KEEP_BUILDS: usize = 2;

/// A dataset with no stamp is left alone this long before tidying removes it (another
/// process may have just written it and not stamped it yet).
const UNSTAMPED_GRACE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// A dataset stamped by other code is left alone this long (another version of the app or
/// the command-line tool may share the folder and still use it).
const OTHER_CODE_GRACE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Another folder of the current build (another install of it) stays while used this
/// recently.
pub const KEEP_UNUSED: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

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
    /// Prebuilt datasets tried before an extraction (the app, while its setting is on);
    /// `None`: always extract (the CLI, the MCP server, tests).
    pub prebuilt: Option<Arc<dyn crate::prebuilt::Prebuilt>>,
    /// Serve datasets an earlier run took from a dump (the user's setting, `config.toml`
    /// `[data] prebuilt_dumps`), when `prebuilt` is `None`; with a `prebuilt` its switch
    /// decides. Off: they are extracted locally.
    pub serve_dumps: bool,
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
    /// Dataset files whose stamp was checked in full: their size and SHA-256 then.
    verified: HashMap<PathBuf, (u64, Option<std::time::SystemTime>, String)>,
    /// Build folders tidied in this run ([`Store::tidy`]).
    tidied: HashSet<PathBuf>,
    /// Lines said once per run (no dump published for a build, ...).
    said: HashSet<String>,
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

    /// The archive's identity: size, modification time and cache format. The code that
    /// writes each dataset is in its stamp, not here ([`crate::stamps`]).
    pub fn fingerprint(p4k: &Path) -> String {
        let meta = std::fs::metadata(p4k).ok();
        let len = meta.as_ref().map_or(0, std::fs::Metadata::len);
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let mut h = Sha256::new();
        h.update(format!("{len}:{mtime}:{CACHE_FORMAT}"));
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

    /// Remove a cached build folder unless a reader holds it (an older build in the app's
    /// Diff: it stays until the next tidy). `true` when it went.
    fn remove_build(p: &Path) -> bool {
        // Held across the removal, so no reader starts meanwhile.
        let lock = reading_lock(p);
        let Ok(held) = starorg_core::lock::try_lock(&lock) else {
            return false;
        };
        let removed = std::fs::remove_dir_all(p).is_ok();
        // Released first: Windows does not delete a file that is still open. A reader that
        // locks it meanwhile finds the folder gone (`hold_build` checks again).
        drop(held);
        if removed {
            let _ = std::fs::remove_file(&lock);
        }
        removed
    }

    /// When a build folder was last written or read: the newest of the folder and its
    /// files (reading marks a file used, [`mark_used`]).
    fn last_used(dir: &Path) -> std::time::SystemTime {
        let t = |m: std::io::Result<std::fs::Metadata>| {
            m.and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH)
        };
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| t(e.metadata()))
            .fold(t(std::fs::metadata(dir)), std::cmp::max)
    }

    fn older_than(t: std::time::SystemTime, age: std::time::Duration) -> bool {
        t.elapsed().is_ok_and(|e| e > age)
    }

    /// Tidy the channel's dataset cache around `current` (its current build folder), once
    /// per run, under the extraction lock:
    ///
    /// - other folders of the same build go when they hold no stamp (the layout before
    ///   stamps) or were not used for [`KEEP_UNUSED`] (another install of the same build on
    ///   this computer keeps its folder while it is in use);
    /// - of the other builds, the most recently used folder stays (the previous build, for
    ///   Diff) and the rest go;
    /// - in `current`, datasets whose stamp names other code go once a day old (another
    ///   version of the app may share the folder), unstamped ones and leftover temp files
    ///   after an hour.
    ///
    /// Folders being read are skipped. Only `<CHANNEL>/<build>-<fingerprint>` folders of this
    /// cache are touched: never the UEX folder, other channels or anything else. Says what it
    /// removed.
    pub fn tidy(&self, ch: &ChannelSource, current: &Path) {
        if !lock(&self.mem).tidied.insert(current.to_path_buf()) {
            return;
        }
        let _g = lock(&self.extracting);
        let label = |n: &str| format!("{} {n}", ch.name);
        let cur_name = current
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let cur_version = build_version(&cur_name).to_string();
        let mut gone: Vec<String> = Vec::new();
        let mut others: Vec<(std::time::SystemTime, String, PathBuf)> = self
            .older_builds(ch)
            .into_iter()
            // Not a `<build>-<fingerprint>` folder: not ours to judge.
            .filter(|(n, _)| build_version(n) != n)
            .map(|(n, p)| (Self::last_used(&p), n, p))
            .collect();
        others.sort_by_key(|x| std::cmp::Reverse(x.0));
        let mut kept_previous = false;
        for (used, n, p) in others {
            let why = if build_version(&n) == cur_version {
                let stamped = std::fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().ends_with(stamps::EXT));
                if !stamped {
                    "an older copy of this build"
                } else if Self::older_than(used, KEEP_UNUSED) {
                    "another copy of this build, unused"
                } else {
                    continue;
                }
            } else if !kept_previous {
                kept_previous = true;
                continue;
            } else {
                "older than the previous build"
            };
            if Self::remove_build(&p) {
                gone.push(format!("{} ({why})", label(&n)));
            }
        }
        // Datasets of other code in the current folder; stamps without their dataset;
        // leftover temp files.
        let mut stale: Vec<String> = Vec::new();
        let names: Vec<String> = std::fs::read_dir(current)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect();
        let age = |f: &Path| {
            std::fs::metadata(f)
                .and_then(|m| m.modified())
                .unwrap_or_else(|_| std::time::SystemTime::now())
        };
        for n in &names {
            let file = current.join(n);
            if n.contains(".tmp-") {
                if Self::older_than(age(&file), UNSTAMPED_GRACE) {
                    let _ = std::fs::remove_file(&file);
                }
                continue;
            }
            let Some(d) = n.strip_suffix(".json").filter(|d| d.starts_with("game-")) else {
                continue;
            };
            let grace = match stamps::read(current, d) {
                Some(s) if s.code == stamps::expected_code(current, d, &s.source) => continue,
                Some(_) => OTHER_CODE_GRACE,
                None => UNSTAMPED_GRACE,
            };
            if !Self::older_than(age(&file), grace) {
                continue;
            }
            if std::fs::remove_file(&file).is_ok() {
                let _ = std::fs::remove_file(stamps::path(current, d));
                stale.push(d.to_string());
            }
        }
        for n in &names {
            if let Some(d) = n.strip_suffix(stamps::EXT)
                && !current.join(format!("{d}.json")).exists()
            {
                let _ = std::fs::remove_file(current.join(n));
            }
        }
        if !stale.is_empty() {
            Self::forget_measurements(current, &stale);
            gone.push(format!(
                "{} from {} (made by other code)",
                stale.join(", "),
                label(&cur_name)
            ));
        }
        for g in &gone {
            self.say(format!("Dataset cache: removed {g}"));
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

    fn write_measurements(dir: &Path, all: &BTreeMap<String, Measurement>) {
        // Temp file and rename: a run stopped half-way never leaves a torn file.
        if let Ok(text) = serde_json::to_string_pretty(all) {
            let tmp = dir.join(format!("measurements.json.tmp-{}", std::process::id()));
            if std::fs::write(&tmp, text).is_err()
                || starorg_core::fsutil::rename_retry(&tmp, &dir.join("measurements.json")).is_err()
            {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    fn record_measurement(dir: &Path, m: &Measurement) {
        let mut all = Self::measurements(dir);
        all.insert(m.dataset.clone(), m.clone());
        Self::write_measurements(dir, &all);
    }

    /// Drop `datasets` from a folder's `measurements.json` (their files were removed).
    fn forget_measurements(dir: &Path, datasets: &[String]) {
        let mut all = Self::measurements(dir);
        let before = all.len();
        all.retain(|k, _| !datasets.contains(k));
        if all.len() != before {
            Self::write_measurements(dir, &all);
        }
    }

    /// Whether datasets taken from a dump may be served: the prebuilt source's switch, else
    /// the configured one ([`StoreConfig::serve_dumps`]).
    pub fn dumps_allowed(&self) -> bool {
        match &self.cfg.prebuilt {
            Some(p) => p.enabled(),
            None => self.cfg.serve_dumps,
        }
    }

    /// `<dir>/<dataset>.json` may be served: its stamp's key is the one expected now and it
    /// describes this file ([`stamps::current`]); a dataset taken from a dump also needs the
    /// prebuilt setting on and its tag not [`crate::prebuilt::known_bad`]. A file checked in
    /// full once in this run is checked by size, modified time and stamp after that.
    pub fn is_current(&self, dir: &Path, dataset: &str) -> bool {
        let file = dir.join(format!("{dataset}.json"));
        let Some(s) = stamps::keyed(dir, dataset) else {
            return false;
        };
        if let Some(tag) = s.dump_tag()
            && (!self.dumps_allowed() || crate::prebuilt::known_bad(tag, dataset).is_some())
        {
            return false;
        }
        let Some(meta) = std::fs::metadata(&file)
            .ok()
            .filter(std::fs::Metadata::is_file)
        else {
            return false;
        };
        if meta.len() != s.bytes {
            return false;
        }
        let mtime = meta.modified().ok();
        if lock(&self.mem).verified.get(&file) == Some(&(s.bytes, mtime, s.sha256.clone())) {
            return true;
        }
        match stamps::current(dir, dataset) {
            Ok(s) => {
                lock(&self.mem)
                    .verified
                    .insert(file, (s.bytes, mtime, s.sha256));
                true
            }
            Err(_) => false,
        }
    }

    /// Mark a served dataset file used (the storage cap's LRU order), keeping its check.
    fn mark_file_used(&self, path: &Path) {
        starorg_core::fsutil::touch(path);
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if let Some(v) = lock(&self.mem).verified.get_mut(path) {
            v.1 = mtime;
        }
    }

    /// Where a cached dataset came from (its stamp's `source`: `local`, `dump:<tag>`,
    /// `derived`), while it is current.
    pub fn source_of(&self, dir: &Path, dataset: &str) -> Option<String> {
        self.is_current(dir, dataset)
            .then(|| stamps::read(dir, dataset).map(|s| s.source))
            .flatten()
    }

    /// Stamp a dataset just written; `expect`: the SHA-256 of what was written (another
    /// process's file is never stamped).
    fn stamp(
        &self,
        dir: &Path,
        dataset: &str,
        source: &str,
        expect: Option<&str>,
    ) -> ToolResult<()> {
        let s = stamps::write(dir, dataset, source, expect)
            .map_err(|e| ToolError(format!("{dataset}: writing its stamp: {e}")))?;
        let mtime = std::fs::metadata(dir.join(format!("{dataset}.json")))
            .and_then(|m| m.modified())
            .ok();
        lock(&self.mem).verified.insert(
            dir.join(format!("{dataset}.json")),
            (s.bytes, mtime, s.sha256),
        );
        Ok(())
    }

    fn say_once(&self, key: String, msg: String) {
        if lock(&self.mem).said.insert(key) {
            self.say(msg);
        }
    }

    /// Make sure a current `<dir>/<dataset>.json` exists for the channel's current build:
    /// taken from a prebuilt dump when one is configured and has it, else extracted.
    fn ensure(&self, ch: &ChannelSource, dir: &Path, dataset: &str) -> ToolResult<()> {
        let r = self.ensure_file(ch, dir, dataset);
        if r.is_ok() {
            self.tidy(ch, dir);
        }
        r
    }

    /// A downloaded dataset, filled and checked: the SHA-256 of the file to stamp. Redacted
    /// prose is filled from the build's own string table (extracted locally, never from a
    /// dump); without it the download is not used.
    fn finish_download(
        &self,
        ch: &ChannelSource,
        dir: &Path,
        dataset: &str,
        fetched: &crate::prebuilt::Fetched,
    ) -> Result<String, String> {
        let file = dir.join(format!("{dataset}.json"));
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if stamps::sha256(&bytes) != fetched.sha256 {
            return Err("replaced by another process meanwhile".into());
        }
        let mut doc: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        drop(bytes);
        if crate::prebuilt::redacted(&doc).is_none() {
            return Ok(fetched.sha256.clone());
        }
        let strings = self
            .json_in(ch, dir, starorg_gamedata::strings::NAME, true)
            .map_err(|e| format!("the string table to fill its descriptions: {e}"))?;
        let mut loc = starorg_gamedata::localization::Localization::new();
        for (k, v) in strings
            .get("strings")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if let Some(v) = v.as_str() {
                loc.set(k, v);
            }
        }
        drop(strings);
        crate::prebuilt::fill_prose(&mut doc, dataset, &loc);
        let text = starorg_gamedata::output::to_js_json(&doc);
        let sha = stamps::sha256(text.as_bytes());
        let tmp = dir.join(format!("{dataset}.json.tmp-fill-{}", std::process::id()));
        std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
        starorg_core::fsutil::rename_retry(&tmp, &file).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })?;
        Ok(sha)
    }

    fn ensure_file(&self, ch: &ChannelSource, dir: &Path, dataset: &str) -> ToolResult<()> {
        if self.is_current(dir, dataset) {
            return Ok(());
        }
        let file = dir.join(format!("{dataset}.json"));
        if dataset == search_index::NAME {
            self.ensure_file(ch, dir, "game-item-tags")?;
            self.ensure_file(ch, dir, "game-ships")?;
            let _g = lock(&self.extracting);
            if !self.is_current(dir, dataset) {
                search_index::write_search_index(dir)
                    .map_err(|e| ToolError(format!("building the search index: {e}")))?;
                self.stamp(dir, dataset, stamps::DERIVED, None)?;
            }
            return Ok(());
        }
        std::fs::create_dir_all(dir).map_err(|e| ToolError(format!("{}: {e}", dir.display())))?;
        let build = ch.build.as_deref().unwrap_or("unknown build");
        // A prebuilt copy first, outside the extraction lock (a slow download never holds up
        // another dataset's extraction); its file is renamed into place whole.
        let started = std::time::Instant::now();
        let mut dump = None;
        // Never the string table: published with name keys only, and what fills a download.
        if let (Some(p), Some(b)) = (&self.cfg.prebuilt, ch.build.as_deref())
            && dataset != starorg_gamedata::strings::NAME
            && p.enabled()
        {
            match p
                .fetch(&ch.name, b, dataset, dir)
                .and_then(|f| Ok((f.tag.clone(), self.finish_download(ch, dir, dataset, &f)?)))
            {
                Ok(got) => dump = Some(got),
                Err(why) => self.say_once(
                    format!("prebuilt:{}:{build}", ch.name),
                    format!(
                        "MCP: prebuilt game data for {} {build} not used ({dataset}: {why}); reading Data.p4k",
                        ch.name
                    ),
                ),
            }
        }
        let _g = lock(&self.extracting);
        if self.is_current(dir, dataset) {
            return Ok(());
        }
        let (m, source, sha) = match dump {
            Some((tag, sha)) => {
                let m = Measurement {
                    dataset: dataset.to_string(),
                    seconds: (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
                    peak_rss_bytes: None,
                    bytes: std::fs::metadata(&file).map_or(0, |m| m.len()),
                    warnings: 0,
                    sha256: Some(sha.clone()),
                };
                self.say(format!(
                    "MCP: {dataset} for {} ({build}) downloaded from the prebuilt dump {tag} in {:.1} s",
                    ch.name, m.seconds
                ));
                (m, format!("{}{tag}", stamps::DUMP_PREFIX), Some(sha))
            }
            None => {
                self.say(format!(
                    "MCP: extracting {dataset} for {} ({build}), once per build",
                    ch.name
                ));
                let m = self
                    .cfg
                    .extractor
                    .extract(&ch.p4k, dataset, dir)
                    .map_err(|e| ToolError(format!("{dataset}: {e}")))?;
                self.say(format!(
                    "MCP: {dataset} ready in {:.1} s{}",
                    m.seconds,
                    m.peak_rss_bytes
                        .map(|b| format!(", worker peak {} MB", b / (1 << 20)))
                        .unwrap_or_default()
                ));
                let sha = m.sha256.clone();
                (m, stamps::LOCAL.to_string(), sha)
            }
        };
        if !file.is_file() {
            return tool_err(format!("{dataset}: the worker wrote no file"));
        }
        self.stamp(dir, dataset, &source, sha.as_deref())?;
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
        let path = self.served(ch, dir, dataset, extract)?;
        let v = Self::read_json(&path)?;
        // Last used, for the storage cap's LRU order.
        self.mark_file_used(&path);
        lock(&self.mem).json.insert(key, v.clone());
        Ok(v)
    }

    /// The file of a dataset in a given build folder, filled first when missing and
    /// `extract` ([`Self::json_in`]), but not parsed or kept in memory: for a big dataset
    /// read once into a typed value ([`Self::typed`]).
    pub fn file_in(
        &self,
        ch: &ChannelSource,
        dir: &Path,
        dataset: &str,
        extract: bool,
    ) -> ToolResult<PathBuf> {
        self.touch(dir);
        let path = self.served(ch, dir, dataset, extract)?;
        self.mark_file_used(&path);
        Ok(path)
    }

    /// The path of a dataset that may be served from `dir`: made current first when
    /// `extract`, else only when its stamp is current already (an older build's folder holds
    /// what was extracted then, by code that may since have changed).
    fn served(
        &self,
        ch: &ChannelSource,
        dir: &Path,
        dataset: &str,
        extract: bool,
    ) -> ToolResult<PathBuf> {
        if extract {
            self.ensure(ch, dir, dataset)?;
        }
        let path = dir.join(format!("{dataset}.json"));
        let folder = dir.file_name().unwrap_or_default().to_string_lossy();
        if !path.is_file() {
            return tool_err(format!("{dataset} is not cached for {folder}"));
        }
        if !self.is_current(dir, dataset) {
            return tool_err(format!(
                "{dataset} of {folder} was extracted by other code than this app's; it is not used"
            ));
        }
        Ok(path)
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
                sha256: None,
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
            prebuilt: None,
            serve_dumps: true,
        })
    }

    #[test]
    fn only_a_dataset_whose_stamp_names_other_code_is_extracted_again() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let s = store(dir.path(), fake.clone());
        let live = s.channel(Some("LIVE")).unwrap();
        s.json(live, "game-loot").unwrap();
        s.json(live, "game-mining").unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 2);
        let d = s.build_dir(live);
        assert_eq!(stamps::read(&d, "game-loot").unwrap().source, stamps::LOCAL);
        assert_eq!(s.source_of(&d, "game-loot").as_deref(), Some("local"));
        // An app update that changed the loot code only: its stamp names the old code.
        let mut old = stamps::read(&d, "game-loot").unwrap();
        old.code = "0123456789abcdef".into();
        std::fs::write(
            stamps::path(&d, "game-loot"),
            serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        let s2 = store(dir.path(), fake.clone());
        let live2 = s2.channel(Some("LIVE")).unwrap();
        // The old stamp is not served from an older build's read-only path either.
        assert!(
            s2.json_in(live2, &d, "game-loot", false)
                .unwrap_err()
                .0
                .contains("other code")
        );
        s2.json(live2, "game-mining").unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 2, "mining kept");
        s2.json(live2, "game-loot").unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 3, "loot extracted again");
        assert_eq!(
            stamps::read(&d, "game-loot").unwrap().code,
            stamps::code_of("game-loot")
        );
        // A file without a stamp (the layout before stamps) is never served.
        std::fs::write(d.join("game-law.json"), "{}").unwrap();
        assert!(s2.json_in(live2, &d, "game-law", false).is_err());
        s2.json(live2, "game-law").unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 4);
    }

    struct FakeDump {
        calls: AtomicUsize,
        answer: Result<(), String>,
        on: std::sync::atomic::AtomicBool,
        body: &'static str,
    }
    impl FakeDump {
        fn new(answer: Result<(), String>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                answer,
                on: true.into(),
                body: r#"{"x": 2}"#,
            }
        }
    }
    impl crate::prebuilt::Prebuilt for FakeDump {
        fn enabled(&self) -> bool {
            self.on.load(Ordering::SeqCst)
        }
        fn fetch(
            &self,
            _: &str,
            build: &str,
            dataset: &str,
            out: &Path,
        ) -> Result<crate::prebuilt::Fetched, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.answer.clone()?;
            std::fs::write(out.join(format!("{dataset}.json")), self.body).unwrap();
            Ok(crate::prebuilt::Fetched {
                tag: build.to_string(),
                sha256: stamps::sha256(self.body.as_bytes()),
            })
        }
    }

    #[test]
    fn a_prebuilt_dataset_is_stamped_as_such_and_a_refused_one_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let mut s = store(dir.path(), fake.clone());
        let dump = Arc::new(FakeDump::new(Ok(())));
        s.cfg.prebuilt = Some(dump.clone());
        let said = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = said.clone();
        let s = s.with_notify(Arc::new(move |m| log.lock().unwrap().push(m)));
        let live = s.channel(Some("LIVE")).unwrap();
        assert_eq!(s.json(live, "game-weapons").unwrap()["x"], 2);
        assert_eq!(fake.0.load(Ordering::SeqCst), 0, "not extracted");
        let d = s.build_dir(live);
        assert_eq!(
            s.source_of(&d, "game-weapons").as_deref(),
            Some("dump:4.10.200.58746")
        );
        assert!(
            said.lock()
                .unwrap()
                .iter()
                .any(|m| m.contains("downloaded"))
        );
        // Refused (no dump, a bad checksum, offline): extracted locally, said once per build.
        let mut s2 = store(dir.path(), fake.clone());
        s2.cfg.prebuilt = Some(Arc::new(FakeDump::new(Err("no network".into()))));
        let log = said.clone();
        let s2 = s2.with_notify(Arc::new(move |m| log.lock().unwrap().push(m)));
        let live2 = s2.channel(Some("LIVE")).unwrap();
        assert_eq!(s2.json(live2, "game-loot").unwrap()["x"], 1);
        assert_eq!(s2.json(live2, "game-mining").unwrap()["x"], 1);
        assert_eq!(fake.0.load(Ordering::SeqCst), 2);
        assert_eq!(s2.source_of(&d, "game-loot").as_deref(), Some("local"));
        let refusals = said
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.contains("no network") && m.contains("reading Data.p4k"))
            .count();
        assert_eq!(refusals, 1, "one line per build, not per dataset");
        // The downloaded one is still served, not fetched again.
        s2.json(live2, "game-weapons").unwrap();
        assert_eq!(dump.calls.load(Ordering::SeqCst), 1);
        // The switch off: a dataset taken from a dump is not served; it is extracted.
        dump.on.store(false, Ordering::SeqCst);
        assert!(
            s.json_in(live, &d, "game-weapons", false).is_ok(),
            "in memory"
        );
        assert!(!s.is_current(&d, "game-weapons"));
        let s3 = store(dir.path(), fake.clone());
        let mut cfg_off = s3;
        cfg_off.cfg.serve_dumps = false;
        let live3 = cfg_off.channel(Some("LIVE")).unwrap();
        assert_eq!(cfg_off.json(live3, "game-weapons").unwrap()["x"], 1);
        assert_eq!(
            cfg_off.source_of(&d, "game-weapons").as_deref(),
            Some("local")
        );
    }

    #[test]
    fn a_dump_marked_bad_later_is_no_longer_served_and_redacted_prose_is_filled() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let s = store(dir.path(), fake.clone());
        let live = s.channel(Some("LIVE")).unwrap();
        let d = s.build_dir(live);
        std::fs::create_dir_all(&d).unwrap();
        // Taken before KNOWN_BAD named it (4.10.200.58746 game-ships).
        std::fs::write(d.join("game-ships.json"), "{}").unwrap();
        stamps::write(&d, "game-ships", "dump:4.10.200.58746", None).unwrap();
        assert!(!s.is_current(&d, "game-ships"));
        s.json(live, "game-ships").unwrap();
        assert_eq!(s.source_of(&d, "game-ships").as_deref(), Some("local"));
        // A redacted dump is filled from the locally extracted string table and marked.
        let mut s = store(dir.path(), fake.clone());
        let mut dump = FakeDump::new(Ok(()));
        dump.body = r#"{"_redacted": ["description"], "missions": [{"descriptionKey": "k"}]}"#;
        s.cfg.prebuilt = Some(Arc::new(dump));
        let live = s.channel(Some("LIVE")).unwrap();
        let v = s.json(live, "game-missions").unwrap();
        assert_eq!(v["_filled"]["fields"][0], "description");
        assert!(
            d.join("game-strings-english.json").is_file(),
            "read locally first"
        );
        assert!(s.is_current(&d, "game-missions"));
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
    fn incomplete_says_why_and_tidying_keeps_two_builds_and_skips_one_being_read() {
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
                sha256: None,
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
        // Held: tidying leaves it; released: it goes.
        let fake = Arc::new(Fake(AtomicUsize::new(0)));
        let said = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = said.clone();
        let s = store(dir.path(), fake).with_notify(Arc::new(move |m| log.lock().unwrap().push(m)));
        let live = s.channel(Some("LIVE")).unwrap();
        let ch = s.channel_cache(live);
        let current = s.build_dir(live);
        let again = ch.join("4.10.200.58746-deadbeef");
        let prev = ch.join("4.9.0.1-00000000");
        let prev_copy = ch.join("4.9.0.1-11111111");
        let older = ch.join("4.8.0.1-00000000");
        let odd = ch.join("notes");
        let ptu = dir.path().join("cache/PTU/4.0.0.1-00000000");
        let uex = dir.path().join("cache/uex");
        // Another install of the same build, in use: stays.
        let twin = ch.join("4.10.200.58746-0badc0de");
        for d in [
            &current, &again, &prev, &prev_copy, &older, &odd, &ptu, &uex, &twin,
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(twin.join("game-ships.json"), "{}").unwrap();
        stamps::write(&twin, "game-ships", stamps::LOCAL, None).unwrap();
        let ago = |s| std::time::SystemTime::now() - std::time::Duration::from_secs(s);
        set_dir_mtime(&older, ago(7200));
        set_dir_mtime(&prev_copy, ago(5400));
        set_dir_mtime(&prev, ago(3600));
        // The current folder: one current dataset, one of other code, one unstamped for
        // hours, one just written and not stamped yet, a lone stamp, the hints file.
        let put = |n: &str| std::fs::write(current.join(n), "{}").unwrap();
        for n in [
            "game-ships.json",
            "game-loot.json",
            "game-mining.json",
            "game-law.json",
            "game-ammo.json",
            "obtain-hints.json",
            "game-cargo.json.tmp-123",
            "game-salvage.json.tmp-456",
        ] {
            put(n);
        }
        stamps::write(&current, "game-ships", stamps::LOCAL, None).unwrap();
        for d in ["game-loot", "game-mining"] {
            let mut other = stamps::write(&current, d, stamps::LOCAL, None).unwrap();
            other.code = "ffffffffffffffff".into();
            std::fs::write(
                stamps::path(&current, d),
                serde_json::to_vec(&other).unwrap(),
            )
            .unwrap();
        }
        let set_age = |n: &str, secs| {
            std::fs::File::options()
                .write(true)
                .open(current.join(n))
                .unwrap()
                .set_modified(ago(secs))
                .unwrap();
        };
        // Other code a day and more old goes; younger (another app version still using the
        // folder) stays. Unstamped for hours goes. An old temp file goes, a fresh one stays.
        set_age("game-loot.json", 2 * 24 * 3600);
        set_age("game-law.json", 2 * 3600);
        set_age("game-cargo.json.tmp-123", 2 * 3600);
        std::fs::write(stamps::path(&current, "game-gone"), "{}").unwrap();
        Store::record_measurement(
            &current,
            &Measurement {
                dataset: "game-loot".into(),
                seconds: 1.0,
                peak_rss_bytes: None,
                bytes: 2,
                warnings: 0,
                sha256: None,
            },
        );
        // A reader holds the oldest build.
        let held = hold_build(&older).unwrap();
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
        s.tidy(live, &current);
        assert!(
            !again.exists(),
            "an unstamped copy of the current build goes"
        );
        assert!(
            twin.is_dir(),
            "another install's folder of this build, in use, stays"
        );
        assert!(
            current.join("game-mining.json").is_file(),
            "other code, but recent"
        );
        assert!(!current.join("game-cargo.json.tmp-123").exists());
        assert!(current.join("game-salvage.json.tmp-456").exists());
        assert!(prev.is_dir(), "the previous build stays");
        assert!(
            !prev_copy.exists(),
            "one folder of the previous build is kept"
        );
        assert!(older.is_dir(), "being read");
        assert!(
            odd.is_dir() && ptu.is_dir() && uex.is_dir(),
            "not ours to remove"
        );
        assert!(current.join("game-ships.json").is_file());
        assert!(
            current.join("game-ammo.json").is_file(),
            "young, maybe not stamped yet"
        );
        assert!(current.join("obtain-hints.json").is_file());
        for gone in [
            "game-loot.json",
            "game-loot.stamp",
            "game-law.json",
            "game-gone.stamp",
        ] {
            assert!(!current.join(gone).exists(), "{gone}");
        }
        assert!(!Store::measurements(&current).contains_key("game-loot"));
        {
            let said = said.lock().unwrap();
            assert!(
                said.iter()
                    .any(|m| m.contains("4.10.200.58746-deadbeef (an older copy of this build)")),
                "{said:?}"
            );
            assert!(said.iter().any(|m| m.contains("game-law, game-loot") || m.contains("game-loot, game-law")), "{said:?}");
        }
        // Once per run.
        drop(held);
        s.tidy(live, &current);
        assert!(
            older.is_dir(),
            "the second reader still holds it, and tidied already"
        );
        drop(second);
        // A child another test starts can share the just-dropped lock for an instant
        // (between fork and exec): tidy again until it is free.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            lock(&s.mem).tidied.clear();
            s.tidy(live, &current);
            if !older.exists() || std::time::Instant::now() > until {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!older.exists());
        assert_eq!(
            s.cached_builds(live).len(),
            KEEP_BUILDS + 2,
            "current, previous, the twin install's and the folder that is not a build"
        );
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
            "the removed folder's lock goes with it: {left:?}"
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
