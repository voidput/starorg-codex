//! Published LIVE builds from `voidput/sc-gamedata-dumps` for Developer mode's Diff
//! (addendum §1, rescoped 2026-11-17: dumps are Diff-only, apply reads `Data.p4k` locally).
//!
//! - **Diff only.** A dump is never turned into `GameData` and never feeds an apply. The app
//!   lists the published LIVE builds ([`live_releases`]), downloads one on request
//!   ([`fetch_verified`]) and shows its `data/*.json` files as a dataset source, so Diff can
//!   compare an old LIVE build with a new one.
//! - **Opt-in.** `starorg-core` links no HTTP client: the transport is a trait
//!   ([`DumpTransport`]) implemented in `starorg-net`, and the caller asks it only while the
//!   user's switch is on. Cached builds stay readable with the switch off.
//! - **One tag and GitHub release per build**, tagged with `Data.Version`. Only releases
//!   whose title or body says LIVE are offered.
//! - **Integrity.** The tag is resolved to a commit once; every dataset is fetched at that
//!   commit and checked against the git blob SHA-1 the commit's tree lists for it (and its
//!   size), then against its own `_build` stamp. The cache stores SHA-256 sums that are
//!   re-checked whenever a file is read ([`DumpFiles::read`]). Content is parsed as data
//!   only, never executed.
//! - **Cached per build** under `<cache>/dumps/<tag>/<commit>/`, written atomically.

use super::SourceError;
use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DUMPS_REPO: &str = "voidput/sc-gamedata-dumps";

/// Largest dataset accepted (the biggest one is about 5 MB on 4.10).
pub const MAX_DATASET_BYTES: u64 = 64 * 1024 * 1024;

/// Most datasets taken from one build (4.10 has 27).
pub const MAX_DATASETS: usize = 200;

/// A GitHub release of the dumps repo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReleaseInfo {
    pub tag: String,
    pub title: String,
    pub body: String,
    /// `published_at` as GitHub gives it (RFC 3339), when known.
    pub published: Option<String>,
}

/// One file of a commit's tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    /// Git blob SHA-1, lower-case hex.
    pub sha: String,
    pub size: u64,
}

/// How the dumps repo is reached. Implemented over HTTPS in `starorg-net`; tests use an
/// in-memory one.
pub trait DumpTransport {
    /// The newest releases, GitHub's order (newest first).
    fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError>;
    /// The release for `tag`, or `None` when there is none.
    fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError>;
    /// Commit SHA the tag points at.
    fn tag_commit(&self, tag: &str) -> Result<String, SourceError>;
    /// Files of the commit's tree (recursive).
    fn tree(&self, commit: &str) -> Result<Vec<TreeEntry>, SourceError>;
    /// Bytes of `path` at `commit`, at most `max` bytes.
    fn file(&self, commit: &str, path: &str, max: u64) -> Result<Vec<u8>, SourceError>;
}

/// Build identity a release states.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseBuild {
    pub branch: Option<String>,
    pub change: Option<u64>,
    pub channel: Option<String>,
}

/// Read `- Branch: \`sc-alpha-4.10.0\``, `- Channel: LIVE` and an optional
/// `- Change: \`13165946\`` (or `RequestedP4ChangeNum`) from a release body.
pub fn parse_release_body(body: &str) -> ReleaseBuild {
    let mut out = ReleaseBuild::default();
    for line in body.lines() {
        let line = line.trim().trim_start_matches(['-', '*']).trim();
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim().trim_matches('`').trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "branch" if !v.is_empty() => out.branch = Some(v.to_string()),
            "channel" if !v.is_empty() => out.channel = Some(v.to_string()),
            "change" | "change number" | "requestedp4changenum" => {
                out.change = v.parse().ok();
            }
            _ => {}
        }
    }
    out
}

/// The channel a release is for: the body's `Channel:` line, else a channel word in the
/// title (`4.10.0 LIVE (4.10.200.58746)`).
pub fn release_channel(r: &ReleaseInfo) -> Option<String> {
    if let Some(c) = parse_release_body(&r.body).channel {
        return Some(c.to_ascii_uppercase());
    }
    r.title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::to_ascii_uppercase)
        .find(|w| {
            matches!(
                w.as_str(),
                "LIVE" | "PTU" | "EPTU" | "HOTFIX" | "TECH-PREVIEW"
            )
        })
}

/// A release is a LIVE build.
pub fn is_live(r: &ReleaseInfo) -> bool {
    release_channel(r).as_deref() == Some("LIVE")
}

/// The LIVE releases with a usable tag, newest first (by `published`, then GitHub's order).
pub fn live_releases(transport: &dyn DumpTransport) -> Result<Vec<ReleaseInfo>, SourceError> {
    let mut v: Vec<(usize, ReleaseInfo)> = transport
        .releases()?
        .into_iter()
        .filter(|r| valid_component(&r.tag) && is_live(r))
        .enumerate()
        .collect();
    // RFC 3339 in UTC sorts as text; a release without a date keeps its place after dated ones.
    v.sort_by(|(ia, a), (ib, b)| match (&a.published, &b.published) {
        (Some(x), Some(y)) => y.cmp(x).then(ia.cmp(ib)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => ia.cmp(ib),
    });
    Ok(v.into_iter().map(|x| x.1).collect())
}

/// Git's blob id: SHA-1 of `blob <len>\0<bytes>`.
pub fn git_blob_sha1(bytes: &[u8]) -> String {
    let mut h = sha1::Sha1::new();
    h.update(format!("blob {}\0", bytes.len()).as_bytes());
    h.update(bytes);
    hex(&h.finalize())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&sha2::Sha256::digest(bytes))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// One verified dataset in the cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedFile {
    pub git_sha1: String,
    pub sha256: String,
    pub size: u64,
}

/// `meta.json` of a cache entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheMeta {
    pub repo: String,
    pub tag: String,
    pub commit: String,
    pub release: ReleaseBuild,
    /// The release title (`4.10.0 LIVE (4.10.200.58746)`); empty in older entries.
    #[serde(default)]
    pub title: String,
    /// The release date (RFC 3339), when GitHub gave one.
    #[serde(default)]
    pub published: Option<String>,
    pub files: BTreeMap<String, CachedFile>,
    /// Seconds since the Unix epoch.
    pub fetched_at: u64,
}

const META_JSON: &str = "meta.json";

/// Leftover temp dirs older than this are removed when the cache is listed (a crashed or
/// killed download); younger ones may belong to a download still running.
const STALE_TMP: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// A tag, commit or file name that is safe as one path component.
pub fn valid_component(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && s != "."
        && s != ".."
}

/// A dataset file name a cache entry may hold: one safe component, never `meta.json` in
/// any case.
fn valid_file_name(s: &str) -> bool {
    valid_component(s) && !s.eq_ignore_ascii_case(META_JSON)
}

/// Names that differ only in case (they collide on Windows and macOS file systems).
fn case_collision<'a>(names: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut seen = std::collections::BTreeSet::new();
    names
        .map(str::to_ascii_lowercase)
        .find(|n| !seen.insert(n.clone()))
}

/// The verified dataset files of one build, on disk.
#[derive(Debug, Clone)]
pub struct DumpFiles {
    pub dir: PathBuf,
    pub meta: CacheMeta,
}

impl DumpFiles {
    /// Read a dataset and check it against the SHA-256 in `meta.json`.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, SourceError> {
        let want = self
            .meta
            .files
            .get(name)
            .ok_or_else(|| SourceError::Check(format!("{name} not in cache entry")))?;
        let bytes = std::fs::read(self.dir.join(name))?;
        if bytes.len() as u64 != want.size || sha256_hex(&bytes) != want.sha256 {
            return Err(SourceError::Check(format!(
                "cached {name} does not match its checksum"
            )));
        }
        Ok(bytes)
    }

    /// The path of a dataset of this entry (`None` for a name not in `meta.json`).
    pub fn path(&self, name: &str) -> Option<PathBuf> {
        self.meta
            .files
            .contains_key(name)
            .then(|| self.dir.join(name))
    }

    /// The `*.json` datasets (not the `_build.json` stamp), by name.
    pub fn datasets(&self) -> impl Iterator<Item = (&String, &CachedFile)> {
        self.meta
            .files
            .iter()
            .filter(|(n, _)| !n.starts_with('_') && n.to_ascii_lowercase().ends_with(".json"))
    }
}

/// A `meta.json` that belongs in `<root>/<tag>/<dir name>/`.
fn meta_ok(m: &CacheMeta, tag: &str, dir_name: &str) -> bool {
    m.tag == tag
        && m.commit == dir_name
        && m.files.keys().all(|k| valid_file_name(k))
        && case_collision(m.files.keys().map(String::as_str)).is_none()
}

fn is_tmp(name: &str) -> bool {
    name.contains(".tmp-")
}

/// Remove `dir` when it is a temp dir older than [`STALE_TMP`].
fn prune_tmp(dir: &Path) {
    let old = std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age > STALE_TMP);
    if old {
        let _ = std::fs::remove_dir_all(dir);
    }
}

static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A build being written: files go to a temp dir as they arrive; [`Staging::finish`] writes
/// `meta.json` and renames it into place. Dropped unfinished, the temp dir is removed.
pub struct Staging {
    tmp: PathBuf,
    dir: PathBuf,
    files: BTreeMap<String, CachedFile>,
    done: bool,
}

impl Staging {
    /// Write one verified file (fsynced) and record its sums.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> Result<(), SourceError> {
        if !valid_file_name(name) || self.files.keys().any(|k| k.eq_ignore_ascii_case(name)) {
            return Err(SourceError::Check(format!("bad file name {name}")));
        }
        let mut f = std::fs::File::create(self.tmp.join(name))?;
        f.write_all(bytes)?;
        f.sync_all()?;
        self.files.insert(
            name.to_string(),
            CachedFile {
                git_sha1: git_blob_sha1(bytes),
                sha256: sha256_hex(bytes),
                size: bytes.len() as u64,
            },
        );
        Ok(())
    }

    /// The sums of the files written so far.
    pub fn files(&self) -> &BTreeMap<String, CachedFile> {
        &self.files
    }

    /// Write `meta.json` (its `files` are the ones added) and move the entry into place.
    pub fn finish(mut self, mut meta: CacheMeta) -> Result<DumpFiles, SourceError> {
        meta.files = std::mem::take(&mut self.files);
        let r = (|| -> std::io::Result<()> {
            let mut f = std::fs::File::create(self.tmp.join(META_JSON))?;
            f.write_all(&serde_json::to_vec_pretty(&meta)?)?;
            f.sync_all()?;
            if self.dir.exists() {
                std::fs::remove_dir_all(&self.dir)?;
            }
            crate::fsutil::rename_retry(&self.tmp, &self.dir)
        })();
        r?;
        self.done = true;
        Ok(DumpFiles {
            dir: self.dir.clone(),
            meta,
        })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.done {
            let _ = std::fs::remove_dir_all(&self.tmp);
        }
    }
}

/// Cache of downloaded builds: `<root>/<tag>/<commit>/`.
#[derive(Debug, Clone)]
pub struct DumpCache {
    pub root: PathBuf,
}

impl DumpCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The entries of `tag` whose `meta.json` reads (files not re-hashed), newest first.
    /// Stale temp dirs of crashed downloads are removed on the way.
    fn entries_of(&self, tag: &str) -> Vec<DumpFiles> {
        if !valid_component(tag) {
            return vec![];
        }
        let mut out: Vec<DumpFiles> = std::fs::read_dir(self.root.join(tag))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if !path.is_dir() {
                    return None;
                }
                if is_tmp(&name) {
                    prune_tmp(&path);
                    return None;
                }
                std::fs::read(path.join(META_JSON))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<CacheMeta>(&b).ok())
                    .filter(|m| meta_ok(m, tag, &name))
                    .map(|meta| DumpFiles { dir: path, meta })
            })
            .collect();
        out.sort_by_key(|f| std::cmp::Reverse(f.meta.fetched_at));
        out
    }

    /// The newest entry of `tag` by its `meta.json` alone (quick: files are checked when
    /// read, [`DumpFiles::read`]).
    pub fn entry(&self, tag: &str) -> Option<DumpFiles> {
        self.entries_of(tag).into_iter().next()
    }

    /// Every cached build (newest entry per tag), newest release first.
    pub fn list(&self) -> Vec<DumpFiles> {
        let mut tags: Vec<String> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|t| valid_component(t))
            .collect();
        tags.sort();
        let mut out: Vec<DumpFiles> = tags.iter().filter_map(|t| self.entry(t)).collect();
        out.sort_by(|a, b| {
            b.meta
                .published
                .cmp(&a.meta.published)
                .then(b.meta.fetched_at.cmp(&a.meta.fetched_at))
        });
        out
    }

    /// The newest valid entry for `tag` (every file re-hashed); invalid entries are deleted.
    pub fn find(&self, tag: &str) -> Option<DumpFiles> {
        let mut best = None;
        for f in self.entries_of(tag) {
            if f.meta.files.keys().all(|n| f.read(n).is_ok()) {
                if best.is_none() {
                    best = Some(f);
                }
            } else {
                let _ = std::fs::remove_dir_all(&f.dir);
            }
        }
        best
    }

    /// Delete every entry of `tag`. `Ok(false)` when there was none.
    pub fn remove(&self, tag: &str) -> Result<bool, SourceError> {
        if !valid_component(tag) {
            return Ok(false);
        }
        let dir = self.root.join(tag);
        if !dir.is_dir() {
            return Ok(false);
        }
        std::fs::remove_dir_all(&dir)?;
        Ok(true)
    }

    /// Start writing the entry `<tag>/<commit>` in a temp dir unique to this download.
    pub fn begin(&self, tag: &str, commit: &str) -> Result<Staging, SourceError> {
        if !valid_component(tag) || !valid_component(commit) {
            return Err(SourceError::Check("bad tag or commit name".into()));
        }
        let parent = self.root.join(tag);
        std::fs::create_dir_all(&parent)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = parent.join(format!("{commit}.tmp-{}-{nanos}-{seq}", std::process::id()));
        std::fs::create_dir_all(&tmp)?;
        Ok(Staging {
            tmp,
            dir: parent.join(commit),
            files: BTreeMap::new(),
            done: false,
        })
    }

    /// Write a verified set atomically (temp dir, fsync, rename).
    pub fn store(
        &self,
        meta: &CacheMeta,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<DumpFiles, SourceError> {
        let mut st = self.begin(&meta.tag, &meta.commit)?;
        for (name, bytes) in files {
            st.add(name, bytes)?;
        }
        st.finish(meta.clone())
    }
}

/// Why a download failed, so the app can tell "no such build" from a refused file and from
/// a network problem.
#[derive(Debug, thiserror::Error)]
pub enum DumpError {
    /// No published LIVE build with that tag (or it is not LIVE).
    #[error("{0}")]
    NotPublished(String),
    /// Refused: a bad tag, too large, a file that does not match its blob id or stamp.
    #[error("{0}")]
    Refused(String),
    /// GitHub could not be reached or answered with an error (offline, timeout, 403/429).
    #[error("{0}")]
    Network(String),
    /// The caller stopped the download (its switch went off).
    #[error("{0}")]
    Stopped(String),
    #[error("cache: {0}")]
    Io(#[from] std::io::Error),
}

impl From<SourceError> for DumpError {
    fn from(e: SourceError) -> Self {
        match e {
            SourceError::Check(m) => DumpError::Refused(m),
            SourceError::Io(e) => DumpError::Io(e),
            SourceError::Unavailable(m) | SourceError::Worker(m) => DumpError::Network(m),
            SourceError::Timeout(d) => DumpError::Network(format!("timed out after {d:?}")),
        }
    }
}

/// The text of a source error without its category prefix (for the UI).
pub fn error_text(e: &SourceError) -> String {
    match e {
        SourceError::Check(m) | SourceError::Unavailable(m) | SourceError::Worker(m) => m.clone(),
        other => other.to_string(),
    }
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "_build")]
    build: Option<serde_json::Value>,
}

/// Largest build accepted, all files together (4.10 is about 25 MB).
pub const MAX_BUILD_BYTES: u64 = 512 * 1024 * 1024;

/// Download, verify and cache every `data/*.json` of the LIVE release `tag`. `progress` gets
/// (files done, files in all, file name) before each file and once more with an empty name
/// before the entry is written; an `Err` from it stops the download (nothing is cached).
pub fn fetch_verified(
    transport: &dyn DumpTransport,
    cache: &DumpCache,
    tag: &str,
    progress: &dyn Fn(usize, usize, &str) -> Result<(), String>,
) -> Result<DumpFiles, DumpError> {
    if !valid_component(tag) {
        return Err(DumpError::Refused(format!("bad build tag {tag:?}")));
    }
    let stop = |r: Result<(), String>| r.map_err(DumpError::Stopped);
    let release = transport
        .release(tag)?
        .ok_or_else(|| DumpError::NotPublished(format!("no published build {tag}")))?;
    if release.tag != tag {
        return Err(DumpError::Refused(format!(
            "asked for {tag}, the release is {}",
            release.tag
        )));
    }
    if !is_live(&release) {
        return Err(DumpError::NotPublished(format!(
            "{tag} is not a LIVE build"
        )));
    }
    let commit = transport.tag_commit(tag)?;
    if !valid_component(&commit)
        || commit.len() != 40
        || !commit.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(DumpError::Refused(format!("bad commit id {commit:?}")));
    }
    let tree = transport.tree(&commit)?;
    let wanted: Vec<&TreeEntry> = tree
        .iter()
        .filter(|e| {
            e.path
                .strip_prefix("data/")
                .is_some_and(|n| valid_file_name(n) && n.to_ascii_lowercase().ends_with(".json"))
        })
        .collect();
    if wanted.is_empty() {
        return Err(DumpError::NotPublished(format!("{tag} has no datasets")));
    }
    if wanted.len() > MAX_DATASETS {
        return Err(DumpError::Refused(format!(
            "{tag} lists {} datasets, more than allowed",
            wanted.len()
        )));
    }
    if let Some(n) = case_collision(wanted.iter().map(|e| e.path.as_str())) {
        return Err(DumpError::Refused(format!(
            "{tag} lists {n} twice (names that differ only in case)"
        )));
    }
    let total: u64 = wanted.iter().map(|e| e.size).sum();
    if total > MAX_BUILD_BYTES {
        return Err(DumpError::Refused(format!(
            "{tag} is {total} bytes, more than the {MAX_BUILD_BYTES} allowed"
        )));
    }
    let mut staging = cache.begin(tag, &commit)?;
    let mut stated = parse_release_body(&release.body);
    for (i, entry) in wanted.iter().enumerate() {
        let name = &entry.path["data/".len()..];
        stop(progress(i, wanted.len(), name))?;
        if entry.size > MAX_DATASET_BYTES {
            return Err(DumpError::Refused(format!(
                "{} is {} bytes, more than allowed",
                entry.path, entry.size
            )));
        }
        let bytes = transport.file(&commit, &entry.path, MAX_DATASET_BYTES)?;
        if bytes.len() as u64 != entry.size || git_blob_sha1(&bytes) != entry.sha {
            return Err(DumpError::Refused(format!(
                "{} does not match the blob id of {tag} ({commit})",
                entry.path
            )));
        }
        if name.starts_with('_') {
            // `_build.json` is the stamp; it may carry the change number and branch.
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let ch = v
                    .get("change")
                    .or_else(|| v.get("changeNum"))
                    .or_else(|| v.get("RequestedP4ChangeNum"));
                if let Some(c) = ch.and_then(|c| c.as_u64().or_else(|| c.as_str()?.parse().ok())) {
                    stated.change = Some(c);
                }
                if stated.branch.is_none() {
                    stated.branch = v.get("branch").and_then(|b| b.as_str()).map(str::to_string);
                }
            }
        } else {
            // A dataset stamped with another build is refused.
            let env: Envelope = serde_json::from_slice(&bytes)
                .map_err(|e| DumpError::Refused(format!("{name}: not a dataset: {e}")))?;
            if let Some(b) = env.build
                && b.as_str() != Some(tag)
            {
                return Err(DumpError::Refused(format!(
                    "{name} is stamped {b}, expected {tag}"
                )));
            }
        }
        staging.add(name, &bytes)?;
    }
    stop(progress(wanted.len(), wanted.len(), ""))?;
    let meta = CacheMeta {
        repo: DUMPS_REPO.to_string(),
        tag: tag.to_string(),
        commit,
        release: stated,
        title: release.title.clone(),
        published: release.published.clone(),
        files: BTreeMap::new(),
        fetched_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    Ok(staging.finish(meta)?)
}

/// Default cache root below the app cache directory.
pub fn cache_root(app_cache: &Path) -> PathBuf {
    app_cache.join("dumps")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn ok(_: usize, _: usize, _: &str) -> Result<(), String> {
        Ok(())
    }

    const BODY: &str = "Parsed game data for Star Citizen build `4.10.200.58746`.\n\n- Channel: LIVE\n- Branch: `sc-alpha-4.10.0`\n\n| Dataset | Size |";
    const TAG: &str = "4.10.200.58746";

    #[test]
    fn release_body() {
        let b = parse_release_body(BODY);
        assert_eq!(b.branch.as_deref(), Some("sc-alpha-4.10.0"));
        assert_eq!(b.channel.as_deref(), Some("LIVE"));
        assert_eq!(b.change, None);
        let b = parse_release_body("- Branch: `x`\n- Change: `13165946`");
        assert_eq!(b.change, Some(13165946));
    }

    fn rel(tag: &str, title: &str, body: &str, published: Option<&str>) -> ReleaseInfo {
        ReleaseInfo {
            tag: tag.into(),
            title: title.into(),
            body: body.into(),
            published: published.map(str::to_string),
        }
    }

    #[test]
    fn live_from_body_or_title() {
        assert!(is_live(&rel(TAG, "x", BODY, None)));
        assert!(is_live(&rel(TAG, "4.10.0 LIVE (4.10.200.58746)", "", None)));
        assert!(!is_live(&rel(TAG, "4.10.0 LIVE", "- Channel: PTU", None)));
        assert!(!is_live(&rel(TAG, "5.0 PTU (5.0.1.2)", "", None)));
        assert!(!is_live(&rel(TAG, "something", "", None)));
    }

    #[test]
    fn blob_sha1_matches_git() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(
            git_blob_sha1(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    fn dataset(name: &str, tag: &str) -> Vec<u8> {
        format!(r#"{{"_source":"t","_build":"{tag}","items":[{{"name":"{name}"}}]}}"#).into_bytes()
    }

    struct Fake {
        releases: Vec<ReleaseInfo>,
        files: BTreeMap<String, Vec<u8>>,
        tamper: Option<String>,
        calls: RefCell<usize>,
        /// Tree sizes as listed (to test the build cap without big files).
        claimed: Option<u64>,
        downloads: RefCell<usize>,
    }

    impl Fake {
        fn new(tag: &str) -> Self {
            let mut files: BTreeMap<String, Vec<u8>> = ["game-ships.json", "game-weapons.json"]
                .iter()
                .map(|n| (format!("data/{n}"), dataset(n, tag)))
                .collect();
            files.insert("README.md".into(), b"readme".to_vec());
            files.insert("data/notes.txt".into(), b"not json".to_vec());
            Self {
                releases: vec![
                    rel(
                        "4.10.197.2878",
                        "4.10.2 LIVE (4.10.197.2878)",
                        "- Channel: LIVE\n- Branch: `sc-alpha-4.10.2`",
                        Some("2026-11-01T14:36:39Z"),
                    ),
                    rel(
                        "5.0.1.2",
                        "5.0 PTU (5.0.1.2)",
                        "- Channel: PTU",
                        Some("2026-11-15T00:00:00Z"),
                    ),
                    rel(
                        TAG,
                        "4.10.0 LIVE (4.10.200.58746)",
                        BODY,
                        Some("2026-11-12T18:20:43Z"),
                    ),
                    rel("../x", "LIVE", "", None),
                ],
                files,
                tamper: None,
                calls: RefCell::new(0),
                claimed: None,
                downloads: RefCell::new(0),
            }
        }
    }

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    impl DumpTransport for Fake {
        fn releases(&self) -> Result<Vec<ReleaseInfo>, SourceError> {
            *self.calls.borrow_mut() += 1;
            Ok(self.releases.clone())
        }
        fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
            *self.calls.borrow_mut() += 1;
            Ok(self.releases.iter().find(|r| r.tag == tag).cloned())
        }
        fn tag_commit(&self, _: &str) -> Result<String, SourceError> {
            Ok(COMMIT.into())
        }
        fn tree(&self, _: &str) -> Result<Vec<TreeEntry>, SourceError> {
            Ok(self
                .files
                .iter()
                .map(|(p, b)| TreeEntry {
                    path: p.clone(),
                    sha: git_blob_sha1(b),
                    size: self.claimed.unwrap_or(b.len() as u64),
                })
                .collect())
        }
        fn file(&self, _: &str, path: &str, _: u64) -> Result<Vec<u8>, SourceError> {
            *self.downloads.borrow_mut() += 1;
            let mut b = self.files[path].clone();
            if self.tamper.as_deref() == Some(path) {
                b[0] = b' ';
            }
            Ok(b)
        }
    }

    #[test]
    fn only_live_releases_newest_first() {
        let fake = Fake::new(TAG);
        let tags: Vec<String> = live_releases(&fake)
            .unwrap()
            .into_iter()
            .map(|r| r.tag)
            .collect();
        assert_eq!(tags, [TAG, "4.10.197.2878"]);
    }

    #[test]
    fn fetch_verify_cache_list_and_remove() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        let fake = Fake::new(TAG);
        let seen = RefCell::new(Vec::new());
        let files = fetch_verified(&fake, &cache, TAG, &|i, n, f| {
            seen.borrow_mut().push((i, n, f.to_string()));
            Ok(())
        })
        .unwrap();
        assert_eq!(files.dir, t.path().join(TAG).join(COMMIT));
        assert_eq!(seen.borrow().len(), 3);
        assert_eq!(seen.borrow()[2], (2, 2, String::new()));
        // Only data/*.json, flat.
        let names: Vec<&String> = files.datasets().map(|x| x.0).collect();
        assert_eq!(names, ["game-ships.json", "game-weapons.json"]);
        assert_eq!(files.meta.title, "4.10.0 LIVE (4.10.200.58746)");
        assert_eq!(
            files.meta.release.branch.as_deref(),
            Some("sc-alpha-4.10.0")
        );
        assert!(files.read("game-ships.json").unwrap().starts_with(b"{"));
        // Listed and found again (every file re-hashed).
        assert_eq!(cache.list().len(), 1);
        assert_eq!(cache.entry(TAG).unwrap().meta.commit, COMMIT);
        let again = cache.find(TAG).unwrap();
        // A corrupted file fails its read, and `find` deletes the entry.
        std::fs::write(again.dir.join("game-weapons.json"), b"{}").unwrap();
        assert!(matches!(
            cache.entry(TAG).unwrap().read("game-weapons.json"),
            Err(SourceError::Check(_))
        ));
        assert!(cache.find(TAG).is_none());
        assert!(!again.dir.exists());
        // Remove.
        fetch_verified(&fake, &cache, TAG, &ok).unwrap();
        assert!(cache.remove(TAG).unwrap());
        assert!(cache.list().is_empty());
        assert!(!cache.remove(TAG).unwrap());
        assert!(!cache.remove("../etc").unwrap());
    }

    #[test]
    fn tampered_download_is_rejected_and_not_cached() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        let mut fake = Fake::new(TAG);
        fake.tamper = Some("data/game-weapons.json".into());
        let err = fetch_verified(&fake, &cache, TAG, &ok).unwrap_err();
        assert!(matches!(err, DumpError::Refused(_)), "{err}");
        assert!(cache.find(TAG).is_none());
        assert!(cache.list().is_empty());
    }

    #[test]
    fn wrong_stamp_not_live_no_release() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        // Files stamped with another build.
        let fake = Fake::new("4.10.197.2878");
        assert!(matches!(
            fetch_verified(&fake, &cache, TAG, &ok),
            Err(DumpError::Refused(_))
        ));
        // A PTU release is not offered.
        assert!(matches!(
            fetch_verified(&Fake::new("5.0.1.2"), &cache, "5.0.1.2", &ok),
            Err(DumpError::NotPublished(_))
        ));
        // No release, or a tag that is not a path component.
        assert!(matches!(
            fetch_verified(&fake, &cache, "9.9.9.9", &ok),
            Err(DumpError::NotPublished(_))
        ));
        assert!(matches!(
            fetch_verified(&fake, &cache, "../x", &ok),
            Err(DumpError::Refused(_))
        ));
        assert!(cache.list().is_empty());
    }

    #[test]
    fn change_number_from_build_stamp() {
        let t = tempfile::tempdir().unwrap();
        let mut fake = Fake::new(TAG);
        fake.files.insert(
            "data/_build.json".into(),
            br#"{"build":"4.10.200.58746","branch":"sc-alpha-4.10.0","change":13165946}"#.to_vec(),
        );
        let files = fetch_verified(&fake, &DumpCache::new(t.path()), TAG, &ok).unwrap();
        assert_eq!(files.meta.release.change, Some(13165946));
        assert!(files.meta.files.contains_key("_build.json"));
        assert_eq!(files.datasets().count(), 2, "the stamp is not a dataset");
    }

    #[test]
    fn build_cap_stop_and_case_collisions() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        // Over the total cap: refused before any file is downloaded.
        let mut big = Fake::new(TAG);
        big.claimed = Some(MAX_BUILD_BYTES / 2 + 1);
        assert!(matches!(
            fetch_verified(&big, &cache, TAG, &ok),
            Err(DumpError::Refused(_))
        ));
        assert_eq!(*big.downloads.borrow(), 0);
        // The caller stops it between files: nothing cached, no temp dir left.
        let fake = Fake::new(TAG);
        let r = fetch_verified(&fake, &cache, TAG, &|i, _, _| {
            if i == 1 { Err("off".into()) } else { Ok(()) }
        });
        assert!(matches!(r, Err(DumpError::Stopped(_))));
        assert_eq!(*fake.downloads.borrow(), 1);
        assert!(cache.list().is_empty());
        assert_eq!(std::fs::read_dir(t.path().join(TAG)).unwrap().count(), 0);
        // Two names that differ only in case.
        let mut twice = Fake::new(TAG);
        twice
            .files
            .insert("data/Game-Ships.json".into(), dataset("x", TAG));
        assert!(matches!(
            fetch_verified(&twice, &cache, TAG, &ok),
            Err(DumpError::Refused(_))
        ));
    }

    #[test]
    fn bad_meta_entries_and_stale_temp_dirs() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        let files = fetch_verified(&Fake::new(TAG), &cache, TAG, &ok).unwrap();
        // A meta.json naming a file outside its dir is ignored.
        let mut bad = files.meta.clone();
        bad.files.insert(
            "../x".into(),
            CachedFile {
                git_sha1: String::new(),
                sha256: String::new(),
                size: 0,
            },
        );
        std::fs::write(files.dir.join(META_JSON), serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(cache.entry(TAG).is_none());
        // So is one whose names differ only in case, or that lists meta.json itself.
        for extra in ["GAME-SHIPS.json", "META.JSON"] {
            let mut m = files.meta.clone();
            m.files
                .insert(extra.into(), m.files["game-ships.json"].clone());
            std::fs::write(files.dir.join(META_JSON), serde_json::to_vec(&m).unwrap()).unwrap();
            assert!(cache.entry(TAG).is_none(), "{extra}");
        }
        // Temp dirs: a fresh one (a running download) stays, an old one is pruned.
        let fresh = t.path().join(TAG).join(format!("{COMMIT}.tmp-1-2-3"));
        let stale = t.path().join(TAG).join(format!("{COMMIT}.tmp-4-5-6"));
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::create_dir_all(&stale).unwrap();
        // (Setting a directory's mtime through File::open works on Unix only.)
        #[cfg(unix)]
        {
            let old = std::time::SystemTime::now() - 2 * STALE_TMP;
            std::fs::File::open(&stale)
                .unwrap()
                .set_modified(old)
                .unwrap();
            cache.list();
            assert!(!stale.exists());
        }
        cache.list();
        assert!(fresh.exists());
        // Two downloads never share a temp dir.
        let a = cache.begin(TAG, COMMIT).unwrap();
        let b = cache.begin(TAG, COMMIT).unwrap();
        assert_ne!(a.tmp, b.tmp);
        drop(a);
        drop(b);
        assert!(fresh.exists());
    }
}
