//! [`PublishedDump`]: `GameData` from `voidput/sc-gamedata-dumps` (addendum §1, spec 3.4).
//!
//! - **Opt-in.** Nothing here touches the network unless the caller built a
//!   [`DumpFirst`] with consent on (default off) and a transport. With consent off it
//!   always extracts locally. `starorg-core` itself links no HTTP client: the transport is
//!   a trait ([`DumpTransport`]) implemented elsewhere (`starorg-net`).
//! - **Same build.** One tag and GitHub release per build, tagged with `Data.Version`.
//!   `Version` is numbered per branch, so releases are never ordered: a dump is used only
//!   for the exact tag of the local `Version`, and only when the release names the same
//!   `Branch`. When the release (or a `data/_build.json`) carries a change number it must
//!   equal the local `RequestedP4ChangeNum`; when it does not, the match is weaker and a
//!   warning says so.
//! - **Integrity.** The tag is resolved to a commit once; every dataset is fetched at that
//!   commit and checked against the git blob SHA-1 the commit's tree lists for it (and its
//!   size), then against its own `_build` stamp. The cache stores SHA-256 sums that are
//!   re-checked on every load. Content is parsed as data only, never executed.
//! - **Cached per build** under `<cache>/dumps/<tag>/<commit>/`, written atomically.
//! - **What dumps lack.** No loc keys (names are resolved text), no contract-generator
//!   contracts, and only redacted strings. `base_ini` therefore always comes from the local
//!   `Data.p4k`; loc keys are recovered by the namesake convention against that file
//!   ([`recover_loc_keys`]); contracts stay empty and are reported, so [`DumpFirst`]
//!   falls back to local extraction when they are required.

use super::{Budget, GameSource, Loaded, Origin, SourceError, Warning};
use crate::build::BuildInfo;
use crate::discovery::Channel;
use crate::ini::StockIni;
use crate::model::{
    Ammo, Blueprint, BlueprintPool, Component, GameData, Missile, ReputationStanding, Ship, Weapon,
    short_name,
};
use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DUMPS_REPO: &str = "voidput/sc-gamedata-dumps";

/// Datasets read into `GameData`, below `data/` in the dumps repo.
pub const DATASETS: [&str; 6] = [
    "game-ships.json",
    "game-ship-components.json",
    "game-weapons.json",
    "game-ammo.json",
    "game-blueprints.json",
    "game-reputation.json",
];

/// Largest dataset accepted (the biggest one is about 5 MB on 4.10).
pub const MAX_DATASET_BYTES: u64 = 64 * 1024 * 1024;

/// A GitHub release of the dumps repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub title: String,
    pub body: String,
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

/// How strongly a release was matched to the local build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildMatch {
    /// Tag = `Version`, same `Branch`, same change number.
    Exact,
    /// Tag = `Version` and same `Branch`; the release states no change number.
    BranchAndVersion,
}

/// Decide whether `release` (with `stated` identity) is the local build (addendum §1).
pub fn match_build(
    local: &BuildInfo,
    release_tag: &str,
    stated: &ReleaseBuild,
) -> Result<BuildMatch, String> {
    if release_tag != local.version {
        return Err(format!(
            "release {release_tag} is not the local build {}",
            local.version
        ));
    }
    match &stated.branch {
        Some(b) if *b == local.branch => {}
        Some(b) => {
            return Err(format!(
                "release {release_tag} is branch {b}, local is {}",
                local.branch
            ));
        }
        None => return Err(format!("release {release_tag} does not name its branch")),
    }
    match (stated.change, local.change) {
        (Some(r), Some(l)) if r == l => Ok(BuildMatch::Exact),
        (Some(r), Some(l)) => Err(format!(
            "release {release_tag} is change {r}, local is change {l}"
        )),
        (Some(_), None) => Err("local build has no change number to compare".into()),
        (None, _) => Ok(BuildMatch::BranchAndVersion),
    }
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
    pub files: BTreeMap<String, CachedFile>,
    /// Seconds since the Unix epoch.
    pub fetched_at: u64,
}

const META_JSON: &str = "meta.json";

fn valid_component(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && s != "."
        && s != ".."
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

    /// The newest valid entry for `tag` (every file re-hashed); invalid entries are deleted.
    pub fn find(&self, tag: &str) -> Option<DumpFiles> {
        if !valid_component(tag) {
            return None;
        }
        let dir = self.root.join(tag);
        let mut best: Option<DumpFiles> = None;
        for e in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.contains(".tmp-") || !path.is_dir() {
                continue;
            }
            let ok = std::fs::read(path.join(META_JSON))
                .ok()
                .and_then(|b| serde_json::from_slice::<CacheMeta>(&b).ok())
                .filter(|m| m.tag == tag && m.commit == name)
                .map(|meta| DumpFiles {
                    dir: path.clone(),
                    meta,
                })
                .filter(|f| f.meta.files.keys().all(|n| f.read(n).is_ok()));
            match ok {
                Some(f) => {
                    if best
                        .as_ref()
                        .is_none_or(|b| b.meta.fetched_at < f.meta.fetched_at)
                    {
                        best = Some(f);
                    }
                }
                None => {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }
        best
    }

    /// Write a verified set atomically (temp dir, fsync, rename).
    pub fn store(
        &self,
        meta: &CacheMeta,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<DumpFiles, SourceError> {
        if !valid_component(&meta.tag) || !valid_component(&meta.commit) {
            return Err(SourceError::Check("bad tag or commit name".into()));
        }
        let parent = self.root.join(&meta.tag);
        std::fs::create_dir_all(&parent)?;
        let dir = parent.join(&meta.commit);
        let tmp = parent.join(format!("{}.tmp-{}", meta.commit, std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp)?;
        let write = |name: &str, bytes: &[u8]| -> std::io::Result<()> {
            let mut f = std::fs::File::create(tmp.join(name))?;
            f.write_all(bytes)?;
            f.sync_all()
        };
        let result = (|| -> std::io::Result<()> {
            for (name, bytes) in files {
                if !valid_component(name) {
                    return Err(std::io::Error::other(format!("bad file name {name}")));
                }
                write(name, bytes)?;
            }
            write(META_JSON, &serde_json::to_vec_pretty(meta)?)?;
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            crate::fsutil::rename_retry(&tmp, &dir)
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e.into());
        }
        Ok(DumpFiles {
            dir,
            meta: meta.clone(),
        })
    }
}

/// Download, verify and cache the datasets of the release tagged `local.version`.
pub fn fetch_verified(
    transport: &dyn DumpTransport,
    cache: &DumpCache,
    local: &BuildInfo,
) -> Result<(DumpFiles, BuildMatch), SourceError> {
    let tag = local.version.as_str();
    if !valid_component(tag) {
        return Err(SourceError::Unavailable(format!(
            "unusable version {tag:?}"
        )));
    }
    let release = transport
        .release(tag)?
        .ok_or_else(|| SourceError::Unavailable(format!("no published dump for {tag}")))?;
    let mut stated = parse_release_body(&release.body);
    let commit = transport.tag_commit(tag)?;
    if !valid_component(&commit) || commit.len() != 40 {
        return Err(SourceError::Check(format!("bad commit id {commit:?}")));
    }
    let tree = transport.tree(&commit)?;
    let mut files = BTreeMap::new();
    let fetch = |name: &str| -> Result<Vec<u8>, SourceError> {
        let path = format!("data/{name}");
        let entry = tree
            .iter()
            .find(|e| e.path == path)
            .ok_or_else(|| SourceError::Unavailable(format!("{path} not in {tag}")))?;
        if entry.size > MAX_DATASET_BYTES {
            return Err(SourceError::Check(format!(
                "{path} is {} bytes, more than allowed",
                entry.size
            )));
        }
        let bytes = transport.file(&commit, &path, MAX_DATASET_BYTES)?;
        if bytes.len() as u64 != entry.size || git_blob_sha1(&bytes) != entry.sha {
            return Err(SourceError::Check(format!(
                "{path} does not match the blob id of {tag} ({commit})"
            )));
        }
        Ok(bytes)
    };
    // An optional build stamp may carry the change number (sc-gamedata extract metadata).
    if tree.iter().any(|e| e.path == "data/_build.json") {
        let b = fetch("_build.json")?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) {
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
        files.insert("_build.json".to_string(), b);
    }
    let how = match_build(local, &release.tag, &stated).map_err(SourceError::Unavailable)?;
    for name in DATASETS {
        let bytes = fetch(name)?;
        files.insert(name.to_string(), bytes);
    }
    let meta = CacheMeta {
        repo: DUMPS_REPO.to_string(),
        tag: tag.to_string(),
        commit,
        release: stated,
        files: files
            .iter()
            .map(|(n, b)| {
                (
                    n.clone(),
                    CachedFile {
                        git_sha1: git_blob_sha1(b),
                        sha256: sha256_hex(b),
                        size: b.len() as u64,
                    },
                )
            })
            .collect(),
        fetched_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    Ok((cache.store(&meta, &files)?, how))
}

// ----- dataset envelopes -------------------------------------------------------------

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "_build")]
    build: Option<String>,
}

#[derive(Deserialize)]
struct ShipsFile {
    #[serde(default)]
    vehicles: Vec<Ship>,
}

#[derive(Deserialize)]
struct WeaponsFile {
    #[serde(default, rename = "shipWeapons")]
    ship: Vec<Weapon>,
    #[serde(default, rename = "fpsWeapons")]
    fps: Vec<Weapon>,
    #[serde(default)]
    missiles: Vec<Missile>,
}

#[derive(Deserialize)]
struct AmmoFile {
    #[serde(default)]
    ammo: Vec<Ammo>,
}

#[derive(Deserialize)]
struct BlueprintsFile {
    #[serde(default)]
    blueprints: Vec<Blueprint>,
    #[serde(default, rename = "rewardPools")]
    pools: Vec<BlueprintPool>,
}

#[derive(Deserialize)]
struct ReputationFile {
    #[serde(default)]
    standings: Vec<ReputationStanding>,
}

fn parse<T: serde::de::DeserializeOwned>(
    name: &str,
    bytes: &[u8],
    tag: &str,
) -> Result<T, SourceError> {
    let env: Envelope = serde_json::from_slice(bytes)
        .map_err(|e| SourceError::Check(format!("{name}: not a dataset: {e}")))?;
    if env.build.as_deref() != Some(tag) {
        return Err(SourceError::Check(format!(
            "{name} is stamped {:?}, expected {tag}",
            env.build
        )));
    }
    serde_json::from_slice(bytes).map_err(|e| SourceError::Check(format!("{name}: {e}")))
}

/// Turn the verified datasets into `GameData` (no loc keys, no contracts).
pub fn to_game_data(files: &DumpFiles) -> Result<GameData, SourceError> {
    let tag = files.meta.tag.as_str();
    let mut gd = GameData {
        ships: parse::<ShipsFile>(DATASETS[0], &files.read(DATASETS[0])?, tag)?.vehicles,
        ..GameData::default()
    };
    let comps: serde_json::Map<String, serde_json::Value> =
        parse(DATASETS[1], &files.read(DATASETS[1])?, tag)?;
    for (cat, v) in comps {
        if cat.starts_with('_') || cat == "summary" || !v.is_array() {
            continue;
        }
        let mut list: Vec<Component> = serde_json::from_value(v)
            .map_err(|e| SourceError::Check(format!("{}: {cat}: {e}", DATASETS[1])))?;
        for c in &mut list {
            if c.category.is_empty() {
                c.category = cat.clone();
            }
        }
        gd.components.extend(list);
    }
    let w: WeaponsFile = parse(DATASETS[2], &files.read(DATASETS[2])?, tag)?;
    gd.weapons = w.ship.into_iter().chain(w.fps).collect();
    gd.missiles = w.missiles;
    gd.ammo = parse::<AmmoFile>(DATASETS[3], &files.read(DATASETS[3])?, tag)?.ammo;
    let b: BlueprintsFile = parse(DATASETS[4], &files.read(DATASETS[4])?, tag)?;
    gd.blueprints = b.blueprints;
    gd.blueprint_pools = b.pools;
    gd.reputation_standings =
        parse::<ReputationFile>(DATASETS[5], &files.read(DATASETS[5])?, tag)?.standings;
    gd.sort();
    Ok(gd)
}

/// Fill loc keys a dump cannot carry by the namesake convention: a record `X` owns
/// `item_NameX` / `item_NameX_short` / `item_DescX` (ships: `vehicle_NameX` /
/// `vehicle_DescX`) when that key is in the local stock file. Returns (filled, records).
pub fn recover_loc_keys(gd: &mut GameData, stock: &StockIni) -> (usize, usize) {
    let found = |key: String| stock.find(&key).ok().map(|_| format!("@{key}"));
    let mut filled = 0;
    let mut total = 0;
    let items = gd
        .components
        .iter_mut()
        .map(|c| &mut c.item)
        .chain(gd.weapons.iter_mut().map(|w| &mut w.item))
        .chain(gd.missiles.iter_mut().map(|m| &mut m.item));
    for item in items {
        total += 1;
        let s = short_name(&item.record_name).to_string();
        item.loc.name = item
            .loc
            .name
            .take()
            .or_else(|| found(format!("item_Name{s}")));
        item.loc.short_name = item
            .loc
            .short_name
            .take()
            .or_else(|| found(format!("item_Name{s}_short")));
        item.loc.desc = item
            .loc
            .desc
            .take()
            .or_else(|| found(format!("item_Desc{s}")));
        if item.loc.name.is_some() || item.loc.desc.is_some() {
            filled += 1;
        }
    }
    for ship in &mut gd.ships {
        total += 1;
        let s = short_name(&ship.record_name).to_string();
        ship.loc.name = ship
            .loc
            .name
            .take()
            .or_else(|| found(format!("vehicle_Name{s}")));
        ship.loc.desc = ship
            .loc
            .desc
            .take()
            .or_else(|| found(format!("vehicle_Desc{s}")));
        if ship.loc.desc.is_some() {
            filled += 1;
        }
    }
    for bp in &mut gd.blueprints {
        if bp.produces_name_key.is_none()
            && let Some(p) = &bp.produces
        {
            bp.produces_name_key = found(format!("item_Name{p}"));
        }
    }
    (filled, total)
}

/// Loads `GameData` from a published dump; the stock text from `text` (a local source,
/// typically a `LocalExtraction` in ini-only mode).
pub struct PublishedDump {
    pub transport: Box<dyn DumpTransport>,
    pub cache: DumpCache,
    pub text: Box<dyn GameSource>,
    /// Use a cached entry without asking the network (default true).
    pub prefer_cache: bool,
}

impl GameSource for PublishedDump {
    fn load(&self, channel: &Channel, budget: &Budget) -> Result<Loaded, SourceError> {
        let build = channel
            .build
            .as_ref()
            .ok_or_else(|| SourceError::Unavailable("channel has no build identity".into()))?;
        let cached = if self.prefer_cache {
            self.cache.find(&build.version)
        } else {
            None
        };
        let (files, how) = match cached {
            Some(f) => {
                let how = match_build(build, &f.meta.tag, &f.meta.release)
                    .map_err(SourceError::Unavailable)?;
                (f, how)
            }
            None => fetch_verified(self.transport.as_ref(), &self.cache, build)?,
        };
        if budget.expired() {
            return Err(SourceError::Timeout(std::time::Duration::ZERO));
        }
        let mut game_data = to_game_data(&files)?;
        let text = self.text.load(channel, budget)?;
        let stock = StockIni::parse(text.base_ini.clone());
        let (filled, total) = recover_loc_keys(&mut game_data, &stock);
        game_data.build = Some(build.clone());
        let mut warnings = text.warnings;
        if how == BuildMatch::BranchAndVersion {
            warnings.push(Warning::new(
                "dump-weak-match",
                format!(
                    "dump {} matched by version and branch only; it states no change number",
                    files.meta.tag
                ),
            ));
        }
        warnings.push(Warning::new(
            "dump-no-contracts",
            "published dumps carry no contract-generator contracts; mission tags need local extraction",
        ));
        warnings.push(Warning::new(
            "dump-loc-keys",
            format!(
                "loc keys recovered by the namesake convention for {filled} of {total} records"
            ),
        ));
        Ok(Loaded {
            base_ini: text.base_ini,
            game_data,
            keyrefs: None,
            origin: Origin::PublishedDump {
                dataset: format!("{}@{} ({})", DUMPS_REPO, files.meta.tag, files.meta.commit),
            },
            warnings,
        })
    }
}

/// Use a published dump when the user turned that on and it is usable, else extract
/// locally (addendum §1). With `consent` off (the default) the dump is never asked.
pub struct DumpFirst {
    pub consent: bool,
    pub dump: Option<PublishedDump>,
    pub local: Box<dyn GameSource>,
    /// Fall back when the dump lacks contracts (true while the dumps carry none).
    pub require_contracts: bool,
}

impl GameSource for DumpFirst {
    fn load(&self, channel: &Channel, budget: &Budget) -> Result<Loaded, SourceError> {
        let mut note = None;
        if self.consent
            && let Some(dump) = &self.dump
        {
            match dump.load(channel, budget) {
                Ok(l) if !(self.require_contracts && l.game_data.contracts.is_empty()) => {
                    return Ok(l);
                }
                Ok(_) => note = Some("the dump has no contracts".to_string()),
                Err(e) => note = Some(e.to_string()),
            }
        }
        let mut l = self.local.load(channel, budget)?;
        if let Some(n) = note {
            l.warnings.push(Warning::new(
                "dump-unused",
                format!("published game data not used ({n}); extracted locally"),
            ));
        }
        Ok(l)
    }
}

/// Default cache root below the app cache directory.
pub fn cache_root(app_cache: &Path) -> PathBuf {
    app_cache.join("dumps")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn live() -> BuildInfo {
        BuildInfo::from_json(
            br#"{"Data":{"Branch":"sc-alpha-4.10.0","RequestedP4ChangeNum":"13165946","Version":"4.10.200.58746"}}"#,
        )
        .unwrap()
    }

    const BODY: &str = "Parsed game data for Star Citizen build `4.10.200.58746`.\n\n- Channel: LIVE\n- Branch: `sc-alpha-4.10.0`\n\n| Dataset | Size |";

    #[test]
    fn release_body() {
        let b = parse_release_body(BODY);
        assert_eq!(b.branch.as_deref(), Some("sc-alpha-4.10.0"));
        assert_eq!(b.channel.as_deref(), Some("LIVE"));
        assert_eq!(b.change, None);
        let b = parse_release_body("- Branch: `x`\n- Change: `13165946`");
        assert_eq!(b.change, Some(13165946));
    }

    #[test]
    fn same_build_rule() {
        let l = live();
        let stated = parse_release_body(BODY);
        assert_eq!(
            match_build(&l, "4.10.200.58746", &stated),
            Ok(BuildMatch::BranchAndVersion)
        );
        let mut exact = stated.clone();
        exact.change = Some(13165946);
        assert_eq!(
            match_build(&l, "4.10.200.58746", &exact),
            Ok(BuildMatch::Exact)
        );
        exact.change = Some(1);
        assert!(match_build(&l, "4.10.200.58746", &exact).is_err());
        // Never by version order: a "newer" tag is not this build.
        assert!(match_build(&l, "4.10.201.0", &stated).is_err());
        // HOTFIX numbers its versions on its own branch.
        let mut hotfix = stated.clone();
        hotfix.branch = Some("sc-alpha-4.10.0-hotfix".into());
        assert!(match_build(&l, "4.10.200.58746", &hotfix).is_err());
        assert!(match_build(&l, "4.10.200.58746", &ReleaseBuild::default()).is_err());
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
        let body = match name {
            "game-ships.json" => {
                r#""vehicles":[{"recordName":"EntityClassDefinition.TEST_Ship","id":"s","flight":{"controller":"c","pitch":38}}]"#
            }
            "game-ship-components.json" => {
                r#""shields":[{"recordName":"EntityClassDefinition.TEST_Shield","category":"shields","stats":{"MaxShieldHealth":10}}],"summary":{"shields":1}"#
            }
            "game-weapons.json" => {
                r#""shipWeapons":[{"recordName":"EntityClassDefinition.TEST_Gun","class":"ship","dps":5,"modes":[]}],"fpsWeapons":[],"missiles":[]"#
            }
            "game-ammo.json" => r#""ammo":[{"name":"a","recordName":"AmmoParams.a","speed":1}]"#,
            "game-blueprints.json" => {
                r#""blueprints":[{"recordName":"CraftingBlueprintRecord.BP_X","produces":"TEST_Gun","tiers":[{}]}],"rewardPools":[{"name":"P","blueprints":[]}]"#
            }
            "game-reputation.json" => {
                r#""standings":[{"name":"R","recordName":"SReputationStandingParams.R","minReputation":1}]"#
            }
            _ => "",
        };
        format!(r#"{{"_source":"t","_build":"{tag}",{body}}}"#).into_bytes()
    }

    struct Fake {
        body: String,
        files: BTreeMap<String, Vec<u8>>,
        tamper: Option<String>,
        calls: RefCell<usize>,
    }

    impl Fake {
        fn new(tag: &str) -> Self {
            Self {
                body: BODY.into(),
                files: DATASETS
                    .iter()
                    .map(|n| (format!("data/{n}"), dataset(n, tag)))
                    .collect(),
                tamper: None,
                calls: RefCell::new(0),
            }
        }
    }

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    impl DumpTransport for Fake {
        fn release(&self, tag: &str) -> Result<Option<ReleaseInfo>, SourceError> {
            *self.calls.borrow_mut() += 1;
            Ok((tag == "4.10.200.58746").then(|| ReleaseInfo {
                tag: tag.into(),
                title: "t".into(),
                body: self.body.clone(),
            }))
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
                    size: b.len() as u64,
                })
                .collect())
        }
        fn file(&self, _: &str, path: &str, _: u64) -> Result<Vec<u8>, SourceError> {
            let mut b = self.files[path].clone();
            if self.tamper.as_deref() == Some(path) {
                b[0] = b' ';
            }
            Ok(b)
        }
    }

    #[test]
    fn fetch_verify_cache_and_convert() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        let fake = Fake::new("4.10.200.58746");
        let (files, how) = fetch_verified(&fake, &cache, &live()).unwrap();
        assert_eq!(how, BuildMatch::BranchAndVersion);
        assert_eq!(files.dir, t.path().join("4.10.200.58746").join(COMMIT));
        let gd = to_game_data(&files).unwrap();
        assert_eq!(gd.ships[0].flight.as_ref().unwrap().pitch, Some(38.0));
        assert_eq!(gd.components[0].category, "shields");
        assert_eq!(gd.weapons[0].dps, Some(5.0));
        assert_eq!(gd.blueprint_pools[0].name, "P");
        assert_eq!(gd.reputation_standings[0].min_reputation, Some(1.0));
        assert!(gd.contracts.is_empty());
        // Cached: found again, every file re-hashed.
        let again = cache.find("4.10.200.58746").unwrap();
        assert_eq!(again.meta.commit, COMMIT);
        // A corrupted cache file invalidates (and deletes) the entry.
        std::fs::write(again.dir.join("game-ammo.json"), b"{}").unwrap();
        assert!(cache.find("4.10.200.58746").is_none());
        assert!(!again.dir.exists());
    }

    #[test]
    fn tampered_download_is_rejected_and_not_cached() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        let mut fake = Fake::new("4.10.200.58746");
        fake.tamper = Some("data/game-weapons.json".into());
        let err = fetch_verified(&fake, &cache, &live()).unwrap_err();
        assert!(matches!(err, SourceError::Check(_)), "{err}");
        assert!(cache.find("4.10.200.58746").is_none());
    }

    #[test]
    fn wrong_stamp_wrong_branch_no_release() {
        let t = tempfile::tempdir().unwrap();
        let cache = DumpCache::new(t.path());
        // Files stamped with another build.
        let fake = Fake::new("4.10.197.2878");
        let (files, _) = fetch_verified(&fake, &cache, &live()).unwrap();
        assert!(matches!(to_game_data(&files), Err(SourceError::Check(_))));
        // Release names another branch.
        let mut fake = Fake::new("4.10.200.58746");
        fake.body = "- Branch: `sc-alpha-4.10.0-hotfix`".into();
        let t2 = tempfile::tempdir().unwrap();
        assert!(matches!(
            fetch_verified(&fake, &DumpCache::new(t2.path()), &live()),
            Err(SourceError::Unavailable(_))
        ));
        // No release for this build (PTU, HOTFIX not published).
        let mut ptu = live();
        ptu.version = "5.0.201.16954".into();
        assert!(matches!(
            fetch_verified(&Fake::new("x"), &DumpCache::new(t2.path()), &ptu),
            Err(SourceError::Unavailable(_))
        ));
    }

    #[test]
    fn change_number_from_build_stamp() {
        let t = tempfile::tempdir().unwrap();
        let mut fake = Fake::new("4.10.200.58746");
        fake.files.insert(
            "data/_build.json".into(),
            br#"{"build":"4.10.200.58746","branch":"sc-alpha-4.10.0","change":13165946}"#.to_vec(),
        );
        let (_, how) = fetch_verified(&fake, &DumpCache::new(t.path()), &live()).unwrap();
        assert_eq!(how, BuildMatch::Exact);
        fake.files
            .insert("data/_build.json".into(), br#"{"change":"1"}"#.to_vec());
        let t2 = tempfile::tempdir().unwrap();
        assert!(fetch_verified(&fake, &DumpCache::new(t2.path()), &live()).is_err());
    }

    #[test]
    fn recover_keys_by_namesake() {
        let t = tempfile::tempdir().unwrap();
        let fake = Fake::new("4.10.200.58746");
        let (files, _) = fetch_verified(&fake, &DumpCache::new(t.path()), &live()).unwrap();
        let mut gd = to_game_data(&files).unwrap();
        let stock = StockIni::parse(
            b"\xEF\xBB\xBFitem_NameTEST_Gun=G\r\nitem_DescTEST_Gun=D\r\nvehicle_DescTEST_Ship=S\r\n".to_vec(),
        );
        let (filled, total) = recover_loc_keys(&mut gd, &stock);
        assert_eq!((filled, total), (2, 3));
        assert_eq!(
            gd.weapons[0].item.loc.desc.as_deref(),
            Some("@item_DescTEST_Gun")
        );
        assert_eq!(
            gd.ships[0].loc.desc.as_deref(),
            Some("@vehicle_DescTEST_Ship")
        );
        assert_eq!(
            gd.blueprints[0].produces_name_key.as_deref(),
            Some("@item_NameTEST_Gun")
        );
        assert_eq!(gd.components[0].item.loc.name, None);
    }

    struct Text;
    impl GameSource for Text {
        fn load(&self, _: &Channel, _: &Budget) -> Result<Loaded, SourceError> {
            Ok(Loaded {
                base_ini: b"\xEF\xBB\xBFk=v\r\n".to_vec(),
                game_data: GameData::default(),
                keyrefs: None,
                origin: Origin::LocalExtraction {
                    starbreaker_commit: "x".into(),
                },
                warnings: vec![],
            })
        }
    }

    fn channel() -> Channel {
        Channel {
            name: "LIVE".into(),
            dir: PathBuf::from("/nonexistent/LIVE"),
            p4k: PathBuf::from("/nonexistent/LIVE/Data.p4k"),
            build: Some(live()),
            build_error: None,
        }
    }

    #[test]
    fn dump_first_respects_consent_and_falls_back() {
        let t = tempfile::tempdir().unwrap();
        let mk = || PublishedDump {
            transport: Box::new(Fake::new("4.10.200.58746")),
            cache: DumpCache::new(t.path()),
            text: Box::new(Text),
            prefer_cache: true,
        };
        // Consent off: the dump is never asked and nothing is cached.
        let off = DumpFirst {
            consent: false,
            dump: Some(mk()),
            local: Box::new(Text),
            require_contracts: false,
        };
        let l = off.load(&channel(), &Budget::unlimited()).unwrap();
        assert!(matches!(l.origin, Origin::LocalExtraction { .. }));
        assert!(l.warnings.is_empty());
        assert!(std::fs::read_dir(t.path()).unwrap().next().is_none());
        // Consent on, contracts not required: the dump is used.
        let on = DumpFirst {
            consent: true,
            dump: Some(mk()),
            local: Box::new(Text),
            require_contracts: false,
        };
        let l = on.load(&channel(), &Budget::unlimited()).unwrap();
        assert!(matches!(l.origin, Origin::PublishedDump { .. }));
        assert!(l.warnings.iter().any(|w| w.kind == "dump-weak-match"));
        assert_eq!(l.game_data.build, Some(live()));
        // Contracts required: falls back with a note.
        let strict = DumpFirst {
            consent: true,
            dump: Some(mk()),
            local: Box::new(Text),
            require_contracts: true,
        };
        let l = strict.load(&channel(), &Budget::unlimited()).unwrap();
        assert!(matches!(l.origin, Origin::LocalExtraction { .. }));
        assert!(l.warnings.iter().any(|w| w.kind == "dump-unused"));
    }
}
