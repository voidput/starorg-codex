//! Extraction cache (spec 4.2): `cache/<install-id>/<CHANNEL>/<cache-key>/`.
//!
//! - `<cache-key>` hashes the fingerprint (change number, `Data.p4k` size and mtime), the
//!   `GameData` schema number, the StarBreaker commit and the source, so an app update that
//!   changes the model layout never reads an old cache.
//! - Written into `<cache-key>.tmp-<pid>/`, files fsynced, then the directory is renamed.
//! - On every load `meta.json` is read and both files are hashed and compared. A mismatch
//!   deletes the entry.
//! - `base.ini` must start with the BOM and hold at least [`MIN_STOCK_KEYS`] keys, because
//!   validation compares our output against it.
//!
//! `GameData` is stored as plain `gamedata.json`: in M1 it holds only the build and DataCore
//! counts (a few hundred bytes). Compression (`.json.zst`) comes with the M2 model.

use crate::discovery::Fingerprint;
use crate::fsutil;
use crate::groups::{KEYREFS_SCHEMA, KeyRefs};
use crate::ini::{BOM, StockIni};
use crate::model::{GAMEDATA_SCHEMA, GameData};
use crate::source::{Loaded, Origin, Warning};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MIN_STOCK_KEYS: usize = 50_000;
const META: &str = "meta.json";
const BASE: &str = "base.ini";
const GAMEDATA: &str = "gamedata.json";
/// Optional (addendum 8.2): entries written by a launch hook have none.
const KEYREFS: &str = "keyrefs.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyInputs {
    pub change: Option<u64>,
    pub p4k_size: u64,
    pub p4k_mtime_ns: u128,
    pub schema: u32,
    /// StarBreaker commit for local extraction.
    pub source_id: String,
    /// `local` or `dump`.
    pub source: String,
}

impl KeyInputs {
    pub fn local(fp: &Fingerprint, starbreaker_commit: &str) -> Self {
        Self {
            change: fp.change,
            p4k_size: fp.p4k_size,
            p4k_mtime_ns: fp.p4k_mtime_ns,
            schema: GAMEDATA_SCHEMA,
            source_id: starbreaker_commit.to_string(),
            source: "local".into(),
        }
    }

    pub fn key(&self) -> String {
        let text = serde_json::to_string(self).unwrap_or_default();
        fsutil::sha256_hex(text.as_bytes())[..24].to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Meta {
    inputs: KeyInputs,
    base_ini_sha256: String,
    gamedata_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keyrefs_sha256: Option<String>,
    origin: Origin,
    #[serde(default)]
    warnings: Vec<Warning>,
}

/// Why a cache entry was not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Miss {
    Absent,
    /// Present but failed a check; it was deleted.
    Rejected(String),
}

fn check_stock(base: &[u8], min_keys: usize) -> Result<(), String> {
    if !base.starts_with(BOM) {
        return Err("base.ini has no BOM".into());
    }
    let keys = StockIni::parse(base.to_vec()).key_count();
    if keys < min_keys {
        return Err(format!("base.ini has {keys} keys, fewer than {min_keys}"));
    }
    Ok(())
}

pub fn entry_dir(channel_cache: &Path, inputs: &KeyInputs) -> PathBuf {
    channel_cache.join(inputs.key())
}

/// Load and verify an entry.
pub fn load(channel_cache: &Path, inputs: &KeyInputs, min_keys: usize) -> Result<Loaded, Miss> {
    let dir = entry_dir(channel_cache, inputs);
    if !dir.is_dir() {
        return Err(Miss::Absent);
    }
    let reject = |why: String| {
        let _ = std::fs::remove_dir_all(&dir);
        Err(Miss::Rejected(why))
    };
    let meta: Meta = match fsutil::read_json(&dir.join(META)) {
        Ok(Some(m)) => m,
        Ok(None) => return reject("no meta.json".into()),
        Err(e) => return reject(e.to_string()),
    };
    if &meta.inputs != inputs {
        return reject("meta.json inputs differ from the key".into());
    }
    let (Ok(base), Ok(gd)) = (
        std::fs::read(dir.join(BASE)),
        std::fs::read(dir.join(GAMEDATA)),
    ) else {
        return reject("missing files".into());
    };
    if fsutil::sha256_hex(&base) != meta.base_ini_sha256 {
        return reject("base.ini hash mismatch".into());
    }
    if fsutil::sha256_hex(&gd) != meta.gamedata_sha256 {
        return reject("gamedata.json hash mismatch".into());
    }
    if let Err(e) = check_stock(&base, min_keys) {
        return reject(e);
    }
    let game_data: GameData = match serde_json::from_slice(&gd) {
        Ok(g) => g,
        Err(e) => return reject(format!("gamedata.json: {e}")),
    };
    Ok(Loaded {
        base_ini: base,
        game_data,
        keyrefs: load_keyrefs(&dir, meta.keyrefs_sha256.as_deref()),
        origin: meta.origin,
        warnings: meta.warnings,
    })
}

/// Store an entry atomically and remove older entries of the channel.
pub fn store(
    channel_cache: &Path,
    inputs: &KeyInputs,
    loaded: &Loaded,
    min_keys: usize,
) -> std::io::Result<()> {
    check_stock(&loaded.base_ini, min_keys).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(channel_cache)?;
    let key = inputs.key();
    let dir = channel_cache.join(&key);
    let tmp = channel_cache.join(format!("{key}.tmp-{}", std::process::id()));
    let gd = serde_json::to_vec(&loaded.game_data).map_err(std::io::Error::other)?;
    let kr = match &loaded.keyrefs {
        Some(k) => Some(serde_json::to_vec(k).map_err(std::io::Error::other)?),
        None => None,
    };
    let meta = Meta {
        inputs: inputs.clone(),
        base_ini_sha256: fsutil::sha256_hex(&loaded.base_ini),
        gamedata_sha256: fsutil::sha256_hex(&gd),
        keyrefs_sha256: kr.as_deref().map(fsutil::sha256_hex),
        origin: loaded.origin.clone(),
        warnings: loaded.warnings.clone(),
    };
    let result = (|| {
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp)?;
        }
        std::fs::create_dir_all(&tmp)?;
        if let Some(kr) = &kr {
            write_synced(&tmp.join(KEYREFS), kr)?;
        }
        for (name, bytes) in [
            (BASE, loaded.base_ini.as_slice()),
            (GAMEDATA, gd.as_slice()),
            (
                META,
                &serde_json::to_vec_pretty(&meta).map_err(std::io::Error::other)?,
            ),
        ] {
            let mut f = std::fs::File::create(tmp.join(name))?;
            std::io::Write::write_all(&mut f, bytes)?;
            f.sync_all()?;
        }
        fsutil::fsync_dir(&tmp)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        crate::fsutil::rename_retry(&tmp, &dir)?;
        fsutil::fsync_dir(channel_cache)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
        return result;
    }
    // Old entries of this channel are of no further use.
    if let Ok(rd) = std::fs::read_dir(channel_cache) {
        for e in rd.filter_map(Result::ok) {
            if e.file_name().to_string_lossy() != key {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    std::io::Write::write_all(&mut f, bytes)?;
    f.sync_all()
}

/// `keyrefs.json` of an entry when present, hashed as `meta.json` says and of the current
/// schema; anything else counts as absent (grouping is then approximate, nothing else).
fn load_keyrefs(dir: &Path, sha: Option<&str>) -> Option<KeyRefs> {
    let sha = sha?;
    let bytes = std::fs::read(dir.join(KEYREFS)).ok()?;
    if fsutil::sha256_hex(&bytes) != sha {
        return None;
    }
    serde_json::from_slice::<KeyRefs>(&bytes)
        .ok()
        .filter(|k| k.schema == KEYREFS_SCHEMA)
}

/// Add (or replace) `keyrefs.json` of an existing, verified entry: the file is written first,
/// then `meta.json` is replaced through a temp file and a rename. An entry that no longer
/// exists is not recreated.
pub fn add_keyrefs(
    channel_cache: &Path,
    inputs: &KeyInputs,
    keyrefs: &KeyRefs,
) -> std::io::Result<()> {
    let dir = entry_dir(channel_cache, inputs);
    let mut meta: Meta = fsutil::read_json(&dir.join(META))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no cache entry"))?;
    if &meta.inputs != inputs {
        return Err(std::io::Error::other("cache entry is for other inputs"));
    }
    let bytes = serde_json::to_vec(keyrefs).map_err(std::io::Error::other)?;
    write_synced(&dir.join(KEYREFS), &bytes)?;
    meta.keyrefs_sha256 = Some(fsutil::sha256_hex(&bytes));
    let tmp = dir.join(format!("{META}.tmp-{}", std::process::id()));
    write_synced(
        &tmp,
        &serde_json::to_vec_pretty(&meta).map_err(std::io::Error::other)?,
    )?;
    fsutil::rename_retry(&tmp, &dir.join(META))?;
    fsutil::fsync_dir(&dir)
}

/// The entry's `base.ini` path when a verified entry exists (used by `patches` to evaluate
/// without extracting).
pub fn has_entry(channel_cache: &Path, inputs: &KeyInputs) -> bool {
    entry_dir(channel_cache, inputs).join(META).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> Loaded {
        let mut base = BOM.to_vec();
        for i in 0..10 {
            base.extend_from_slice(format!("k{i}=v\r\n").as_bytes());
        }
        Loaded {
            base_ini: base,
            game_data: GameData::default(),
            keyrefs: None,
            origin: Origin::LocalExtraction {
                starbreaker_commit: "c".into(),
            },
            warnings: vec![],
        }
    }

    fn inputs() -> KeyInputs {
        KeyInputs::local(
            &Fingerprint {
                change: Some(1),
                p4k_size: 2,
                p4k_mtime_ns: 3,
            },
            "c",
        )
    }

    #[test]
    fn store_load_and_tamper() {
        let t = tempfile::tempdir().unwrap();
        let i = inputs();
        assert_eq!(load(t.path(), &i, 5).unwrap_err(), Miss::Absent);
        store(t.path(), &i, &loaded(), 5).unwrap();
        assert_eq!(load(t.path(), &i, 5).unwrap().base_ini, loaded().base_ini);
        // Too few keys for the real threshold: rejected and deleted.
        assert!(matches!(load(t.path(), &i, 50_000), Err(Miss::Rejected(_))));
        assert!(!entry_dir(t.path(), &i).exists());
        store(t.path(), &i, &loaded(), 5).unwrap();
        std::fs::write(entry_dir(t.path(), &i).join(BASE), b"\xEF\xBB\xBFtampered").unwrap();
        assert!(matches!(load(t.path(), &i, 5), Err(Miss::Rejected(_))));
        // Keyrefs: absent, then added to the entry, then tampered (dropped, entry kept).
        store(t.path(), &i, &loaded(), 5).unwrap();
        assert_eq!(load(t.path(), &i, 5).unwrap().keyrefs, None);
        let mut b = crate::groups::KeyRefsBuilder::new();
        b.add(crate::groups::RefSource::default(), ["@k1"]);
        let kr = b.finish();
        add_keyrefs(t.path(), &i, &kr).unwrap();
        assert_eq!(load(t.path(), &i, 5).unwrap().keyrefs, Some(kr.clone()));
        let mut with = loaded();
        with.keyrefs = Some(kr.clone());
        store(t.path(), &i, &with, 5).unwrap();
        assert_eq!(load(t.path(), &i, 5).unwrap().keyrefs, Some(kr));
        std::fs::write(entry_dir(t.path(), &i).join(KEYREFS), b"{}").unwrap();
        let l = load(t.path(), &i, 5).unwrap();
        assert_eq!(l.keyrefs, None);
        // A different schema or source is a different key.
        let mut other = i.clone();
        other.schema += 1;
        assert_ne!(other.key(), i.key());
    }
}
