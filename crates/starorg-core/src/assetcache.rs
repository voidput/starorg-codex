//! The Developer mode asset cache (addendum 7.16): decoded sounds, exported models and
//! texture previews, so a second play or export of the same thing does not start the asset
//! worker again.
//!
//! Layout, under the cache folder (default `<app cache>/assets`, or `dev.asset_cache_dir` in
//! `config.toml`, or the CLI's `--cache-dir`):
//!
//! ```text
//! .starorg-asset-cache            marker: this folder is ours
//! <CHANNEL>/<build-key>/audio/    <media key>.ogg | .wem
//! <CHANNEL>/<build-key>/models/   <record>-<preset>-lod<n>.glb
//! <CHANNEL>/<build-key>/previews/ <path hash>.png
//! ```
//!
//! - **Keyed by build.** A game patch changes the build key; the first write for the new
//!   build removes that channel's other build folders.
//! - **Bounded.** After each write the oldest files (by modification time; a hit touches its
//!   file) are removed until the total is under the cap (2 GiB by default).
//! - **Only our files.** Eviction, clear and move touch nothing but the layout above, so a
//!   cache folder the user picked can hold other files safely.

use crate::config::Config;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Default size cap: 2 GiB.
pub const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The marker file at the cache root.
pub const MARKER: &str = ".starorg-asset-cache";
/// A file written but not yet moved into place (`<final>.part`).
pub const PART: &str = "part";
/// `.part` files older than this are leftovers of a stopped worker.
const STALE_PART: Duration = Duration::from_secs(3600);
/// The `config.toml` table and keys (`[dev] asset_cache_dir`, `asset_cache_max_mb`).
const TABLE: &str = "dev";
const KEY_DIR: &str = "asset_cache_dir";
const KEY_MAX: &str = "asset_cache_max_mb";

/// What a cached file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Audio,
    Models,
    Previews,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Audio, Kind::Models, Kind::Previews];

    pub fn dir(self) -> &'static str {
        match self {
            Kind::Audio => "audio",
            Kind::Models => "models",
            Kind::Previews => "previews",
        }
    }
}

/// Bytes and files in the cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub bytes: u64,
    pub files: u64,
}

/// The cache at one folder with one cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetCache {
    root: PathBuf,
    max_bytes: u64,
}

/// A folder or file name safe on every OS: `[A-Za-z0-9._-]`, at most 120 characters; anything
/// else (or a longer name) becomes a SHA-256 prefix of the original.
pub fn safe_name(s: &str) -> String {
    let ok = !s.is_empty()
        && s.len() <= 120
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        s.to_string()
    } else {
        crate::fsutil::sha256_hex(s.as_bytes())[..32].to_string()
    }
}

/// Every file of our layout under `root`: (path, size, modified).
fn walk(root: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let mut out = Vec::new();
    let dirs = |p: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect()
    };
    for ch in dirs(root) {
        for build in dirs(&ch) {
            for kind in Kind::ALL {
                let d = build.join(kind.dir());
                for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                    let Ok(m) = e.metadata() else { continue };
                    if m.is_file() {
                        let t = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        out.push((e.path(), m.len(), t));
                    }
                }
            }
        }
    }
    out
}

fn is_part(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == PART)
}

/// Remove `dir` and its parents up to (not including) `stop` while they are empty.
fn remove_empty_up(dir: &Path, stop: &Path) {
    let mut d = dir.to_path_buf();
    while d.starts_with(stop) && d != stop {
        if std::fs::remove_dir(&d).is_err() {
            break;
        }
        if !d.pop() {
            break;
        }
    }
}

impl AssetCache {
    pub fn new(root: impl Into<PathBuf>, max_bytes: u64) -> Self {
        Self {
            root: root.into(),
            max_bytes: max_bytes.max(1),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// `<root>/<CHANNEL>/<build>/<kind>/`
    pub fn kind_dir(&self, channel: &str, build: &str, kind: Kind) -> PathBuf {
        self.root
            .join(safe_name(&channel.to_ascii_uppercase()))
            .join(safe_name(build))
            .join(kind.dir())
    }

    /// Where `key.ext` lives (whether or not it is there).
    pub fn path(&self, channel: &str, build: &str, kind: Kind, key: &str, ext: &str) -> PathBuf {
        self.kind_dir(channel, build, kind)
            .join(format!("{}.{}", safe_name(key), safe_name(ext)))
    }

    /// The file for a worker to write first (`<final>.part`); [`AssetCache::commit`] moves it.
    pub fn part_path(final_path: &Path) -> PathBuf {
        let mut s = final_path.as_os_str().to_owned();
        s.push(".");
        s.push(PART);
        PathBuf::from(s)
    }

    /// A hit: the file, its modification time set to now (it is the newest for eviction).
    pub fn get(
        &self,
        channel: &str,
        build: &str,
        kind: Kind,
        key: &str,
        ext: &str,
    ) -> Option<PathBuf> {
        let p = self.path(channel, build, kind, key, ext);
        let m = std::fs::metadata(&p).ok()?;
        if !m.is_file() || m.len() == 0 {
            return None;
        }
        if let Ok(f) = std::fs::File::options().write(true).open(&p) {
            let _ = f.set_modified(SystemTime::now());
        }
        Some(p)
    }

    /// Get or make the folders for a write; writes the root marker.
    pub fn prepare(&self, channel: &str, build: &str, kind: Kind) -> io::Result<PathBuf> {
        let d = self.kind_dir(channel, build, kind);
        std::fs::create_dir_all(&d)?;
        let marker = self.root.join(MARKER);
        if !marker.exists() {
            std::fs::write(
                &marker,
                b"StarOrg Codex asset cache (Developer mode). Safe to delete.\n",
            )?;
        }
        Ok(d)
    }

    /// Move a finished file (usually a `.part`) into place, drop the channel's other builds
    /// and evict down to the cap. Files larger than a quarter of the cap are not kept: the
    /// source is removed and `None` comes back.
    pub fn commit(
        &self,
        channel: &str,
        build: &str,
        kind: Kind,
        key: &str,
        ext: &str,
        src: &Path,
    ) -> io::Result<Option<PathBuf>> {
        let len = std::fs::metadata(src)?.len();
        if len > self.max_bytes / 4 {
            let _ = std::fs::remove_file(src);
            return Ok(None);
        }
        self.prepare(channel, build, kind)?;
        let dst = self.path(channel, build, kind, key, ext);
        if crate::fsutil::rename_retry(src, &dst).is_err() {
            std::fs::copy(src, &dst)?;
            let _ = std::fs::remove_file(src);
        }
        if let Ok(f) = std::fs::File::options().write(true).open(&dst) {
            let _ = f.set_modified(SystemTime::now());
        }
        self.prune_builds(channel, build);
        self.evict()?;
        Ok(Some(dst))
    }

    /// Copy `src` (an export the user asked for) into the cache as well.
    pub fn put_copy(
        &self,
        channel: &str,
        build: &str,
        kind: Kind,
        key: &str,
        ext: &str,
        src: &Path,
    ) -> io::Result<Option<PathBuf>> {
        let len = std::fs::metadata(src)?.len();
        if len > self.max_bytes / 4 {
            return Ok(None);
        }
        self.prepare(channel, build, kind)?;
        let part = Self::part_path(&self.path(channel, build, kind, key, ext));
        std::fs::copy(src, &part)?;
        self.commit(channel, build, kind, key, ext, &part)
    }

    /// After a worker wrote a file straight into place: drop the channel's other builds and
    /// evict down to the cap.
    pub fn written(&self, channel: &str, build: &str) -> io::Result<()> {
        self.prune_builds(channel, build);
        self.evict().map(|_| ())
    }

    /// Remove the channel's folders for every build but `keep` (a game patch invalidates).
    pub fn prune_builds(&self, channel: &str, keep: &str) -> u64 {
        let ch = self.root.join(safe_name(&channel.to_ascii_uppercase()));
        let keep = safe_name(keep);
        let mut removed = 0;
        for e in std::fs::read_dir(&ch).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy() == keep {
                continue;
            }
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                removed += self.remove_build_dir(&e.path());
            }
        }
        removed
    }

    /// Our files in one build folder, then the folders if empty.
    fn remove_build_dir(&self, build: &Path) -> u64 {
        let mut n = 0;
        for kind in Kind::ALL {
            let d = build.join(kind.dir());
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                if e.file_type().is_ok_and(|t| t.is_file())
                    && std::fs::remove_file(e.path()).is_ok()
                {
                    n += 1;
                }
            }
            let _ = std::fs::remove_dir(&d);
        }
        let _ = std::fs::remove_dir(build);
        n
    }

    pub fn usage(&self) -> Usage {
        walk(&self.root)
            .into_iter()
            .fold(Usage::default(), |u, (_, len, _)| Usage {
                bytes: u.bytes + len,
                files: u.files + 1,
            })
    }

    /// Remove the oldest files until the total is at most the cap; stale `.part` files go
    /// first. Returns the bytes removed.
    pub fn evict(&self) -> io::Result<u64> {
        let now = SystemTime::now();
        let mut files = walk(&self.root);
        let mut removed = 0;
        files.retain(|(p, len, t)| {
            let stale = is_part(p) && now.duration_since(*t).unwrap_or_default() > STALE_PART;
            if stale && std::fs::remove_file(p).is_ok() {
                removed += len;
                return false;
            }
            !is_part(p)
        });
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort_by_key(|f| f.2);
        for (p, len, _) in files {
            if total <= self.max_bytes {
                break;
            }
            if std::fs::remove_file(&p).is_ok() {
                total -= len;
                removed += len;
            }
        }
        Ok(removed)
    }

    /// Remove every cached file (our layout only) and the marker. Returns the usage removed.
    pub fn clear(&self) -> io::Result<Usage> {
        let mut u = Usage::default();
        for (p, len, _) in walk(&self.root) {
            if std::fs::remove_file(&p).is_ok() {
                u.bytes += len;
                u.files += 1;
                if let Some(d) = p.parent() {
                    remove_empty_up(d, &self.root);
                }
            }
        }
        let _ = std::fs::remove_file(self.root.join(MARKER));
        Ok(u)
    }

    /// Move our files to `to` (rename, or copy then remove across drives); the old folder keeps
    /// anything that is not ours. Returns what was moved.
    pub fn move_to(&self, to: &Path) -> io::Result<Usage> {
        let dest = AssetCache::new(to, self.max_bytes);
        let mut u = Usage::default();
        for (p, len, _) in walk(&self.root) {
            if is_part(&p) {
                continue;
            }
            let Ok(rel) = p.strip_prefix(&self.root) else {
                continue;
            };
            let target = to.join(rel);
            if let Some(d) = target.parent() {
                std::fs::create_dir_all(d)?;
            }
            if std::fs::rename(&p, &target).is_err() {
                std::fs::copy(&p, &target)?;
                std::fs::remove_file(&p)?;
            }
            u.bytes += len;
            u.files += 1;
            if let Some(d) = p.parent() {
                remove_empty_up(d, &self.root);
            }
        }
        if u.files > 0 {
            std::fs::create_dir_all(to)?;
            std::fs::write(
                to.join(MARKER),
                b"StarOrg Codex asset cache (Developer mode). Safe to delete.\n",
            )?;
        }
        let _ = std::fs::remove_file(self.root.join(MARKER));
        dest.evict()?;
        Ok(u)
    }
}

// ---------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------

/// `<app cache>/assets`
pub fn default_dir(dirs: &crate::appdirs::AppDirs) -> PathBuf {
    dirs.cache.join("assets")
}

fn dev_table(cfg: &Config) -> Option<&toml::Table> {
    cfg.extra.get(TABLE)?.as_table()
}

/// `[dev] asset_cache_dir` from `config.toml`, when set.
pub fn configured_dir(cfg: &Config) -> Option<PathBuf> {
    dev_table(cfg)?
        .get(KEY_DIR)?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Set (`Some`) or reset to the default (`None`) the cache folder in `config.toml`.
pub fn set_configured_dir(cfg: &mut Config, dir: Option<&Path>) {
    let t = cfg
        .extra
        .entry(TABLE)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !t.is_table() {
        *t = toml::Value::Table(toml::Table::new());
    }
    let t = t.as_table_mut().expect("table");
    match dir {
        Some(d) => {
            t.insert(KEY_DIR.into(), toml::Value::String(d.display().to_string()));
        }
        None => {
            t.remove(KEY_DIR);
        }
    }
    if t.is_empty() {
        cfg.extra.remove(TABLE);
    }
}

/// `[dev] asset_cache_max_mb`, else 2 GiB.
pub fn configured_max(cfg: &Config) -> u64 {
    dev_table(cfg)
        .and_then(|t| t.get(KEY_MAX))
        .and_then(|v| v.as_integer())
        .filter(|&mb| mb > 0)
        .map(|mb| mb as u64 * 1024 * 1024)
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// The cache: `override_dir` (CLI `--cache-dir`), else the setting, else the default.
pub fn resolve(
    dirs: &crate::appdirs::AppDirs,
    cfg: &Config,
    override_dir: Option<&Path>,
) -> AssetCache {
    let root = override_dir
        .map(Path::to_path_buf)
        .or_else(|| configured_dir(cfg))
        .unwrap_or_else(|| default_dir(dirs));
    AssetCache::new(root, configured_max(cfg))
}

/// The folders exports and the cache never go into: each game folder and its Wine prefix
/// (the same rule as the Developer mode export guard).
pub fn protected_roots(game_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for g in game_dirs {
        let c = crate::fsutil::canonical_lenient(g);
        v.extend(crate::discovery::enclosing_wine_prefix(&c));
        v.push(c);
    }
    v
}

/// The protected folder `path` lies in, if any.
pub fn inside_protected(path: &Path, protected: &[PathBuf]) -> Option<PathBuf> {
    let abs = crate::fsutil::canonical_lenient(path);
    protected.iter().find(|p| abs.starts_with(p)).cloned()
}

/// A cache folder that passed [`validate_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirCheck {
    pub path: PathBuf,
    /// On another drive than the app's cache folder (fine; shown as a note).
    pub other_drive: bool,
}

#[cfg(unix)]
fn device(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    p.ancestors()
        .find_map(|a| std::fs::metadata(a).ok())
        .map(|m| m.dev())
}

#[cfg(not(unix))]
fn device(p: &Path) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    use std::path::Component;
    let abs = std::path::absolute(p).ok()?;
    match abs.components().next()? {
        Component::Prefix(x) => {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            x.as_os_str()
                .to_string_lossy()
                .to_ascii_lowercase()
                .hash(&mut h);
            Some(h.finish())
        }
        _ => None,
    }
}

/// A folder the cache may use: absolute, not inside a game folder or its Wine prefix, and
/// writable (it is created, a probe file written and removed).
pub fn validate_dir(dir: &Path, protected: &[PathBuf], default: &Path) -> Result<DirCheck, String> {
    if !dir.is_absolute() {
        return Err(format!("{} is not an absolute path", dir.display()));
    }
    if dir.exists() && !dir.is_dir() {
        return Err(format!("{} is a file, not a folder", dir.display()));
    }
    if let Some(p) = inside_protected(dir, protected) {
        return Err(format!(
            "{} is inside {}; the asset cache never goes into the game folder",
            dir.display(),
            p.display()
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let probe = dir.join(format!(".starorg-probe-{}", crate::fsutil::random_id(6)));
    std::fs::write(&probe, b"probe")
        .map_err(|e| format!("{} is not writable: {e}", dir.display()))?;
    let _ = std::fs::remove_file(&probe);
    let other_drive = match (device(dir), device(default)) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    };
    Ok(DirCheck {
        path: dir.to_path_buf(),
        other_drive,
    })
}

/// Standard base64 (with padding), for small binary replies to the webview.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests;
