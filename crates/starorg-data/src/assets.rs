//! Developer mode asset export (addendum 7.16): the game's files, sounds and models, read
//! through StarBreaker's crates (`starbreaker-p4k`, `-dds`, `-cryxml`, `-wem`, `-wwise`, `-3d`).
//!
//! Used by the `asset-worker` process ([`crate::assetworker`]) and by the CLI's `dev extract`,
//! `dev export-model` and `dev export-sounds`, which run these functions in their own process.
//! Everything here is read only on `Data.p4k`; every output path goes through
//! [`crate::devworker::check_out`] (never inside the game folder or its Wine prefix).
//!
//! - [`files`]: archive listing, search, previews (DDS -> PNG, CryXML -> XML text) and
//!   extraction of files and folders.
//! - [`audio`]: a record's audio triggers (DataCore) -> ATL triggers -> Wwise banks -> media;
//!   WEM -> Ogg Vorbis. Ported from StarBreaker's app (`audio_commands.rs`).
//! - [`model`]: entity -> GLB with the "Full" or "Print shell" preset, optional STL.
//!
//! Lazy: [`AssetArchive::open`] reads only the archive index. The DataCore is read on the first
//! request that needs a record (sounds, models); the Wwise/ATL index on the first sounds
//! request. Both live as long as the process.

pub mod audio;
pub mod files;
pub mod model;
pub mod stl;
#[cfg(any(test, feature = "synthetic-dcb"))]
pub mod synthetic;

use crate::extract::{DCB_ENTRY, DCB_FALLBACK_ENTRY, MAX_DCB_BYTES};
use serde::{Deserialize, Serialize};
use starbreaker_datacore::database::Database;
use starbreaker_datacore::types::Record;
use starbreaker_p4k::{MappedP4k, P4kEntry};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// An error with a reply code (`not-found`, `invalid`, `busy`, `cancelled`, `internal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct AssetError {
    pub code: String,
    pub message: String,
}

impl AssetError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new("not-found", m)
    }
    pub fn invalid(m: impl Into<String>) -> Self {
        Self::new("invalid", m)
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new("internal", m)
    }
    pub fn cancelled() -> Self {
        Self::new("cancelled", "cancelled")
    }
}

impl From<crate::devworker::ReplyError> for AssetError {
    fn from(e: crate::devworker::ReplyError) -> Self {
        Self {
            code: e.code,
            message: e.message,
        }
    }
}

impl From<AssetError> for crate::devworker::ReplyError {
    fn from(e: AssetError) -> Self {
        Self {
            code: e.code,
            message: e.message,
        }
    }
}

pub type AssetResult<T> = Result<T, AssetError>;

/// Peak resident memory of this process (Linux `VmHWM`), bytes.
pub fn peak_rss_bytes() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// `Data/Objects/x.dds` or `Data\Objects\x.dds\` -> `Data\Objects\x.dds`.
pub fn norm_path(p: &str) -> String {
    p.trim().replace('/', "\\").trim_matches('\\').to_string()
}

/// The open archive and what was read from it lazily.
pub struct AssetArchive {
    p4k: MappedP4k,
    /// The game folder (the p4k's): outputs never go there.
    game_dir: Option<PathBuf>,
    db: OnceLock<Result<Database<'static>, String>>,
    audio: OnceLock<Result<audio::AudioIndex, String>>,
}

impl AssetArchive {
    /// Open the archive index with the count check extraction uses (read only).
    pub fn open(p4k: &Path) -> AssetResult<Self> {
        let (archive, _) =
            crate::extract::guarded("archive open", || crate::extract::open_checked(p4k))
                .map_err(|e| AssetError::internal(e.to_string()))?;
        Ok(Self {
            p4k: archive.into_mapped(),
            game_dir: p4k.parent().map(Path::to_path_buf),
            db: OnceLock::new(),
            audio: OnceLock::new(),
        })
    }

    /// An archive opened without the count check (synthetic test archives).
    pub fn from_mapped(p4k: MappedP4k, game_dir: Option<PathBuf>) -> Self {
        Self {
            p4k,
            game_dir,
            db: OnceLock::new(),
            audio: OnceLock::new(),
        }
    }

    pub fn p4k(&self) -> &MappedP4k {
        &self.p4k
    }

    pub fn game_dir(&self) -> Option<&Path> {
        self.game_dir.as_deref()
    }

    pub fn entries(&self) -> usize {
        self.p4k.len()
    }

    /// Exports never go inside the game folder or its Wine prefix.
    pub fn check_out(&self, out: &Path) -> AssetResult<()> {
        crate::devworker::check_out(out, self.game_dir()).map_err(Into::into)
    }

    /// The entry at `path` (either slash), case-insensitively.
    pub fn entry(&self, path: &str) -> Option<&P4kEntry> {
        let p = norm_path(path);
        self.p4k
            .entry(&p)
            .or_else(|| self.p4k.entry_case_insensitive(&p))
    }

    /// Read and decompress an entry, checking its size limit, size and CRC-32C as extraction
    /// does.
    pub fn read(&self, e: &P4kEntry, max: u64) -> AssetResult<Vec<u8>> {
        if e.uncompressed_size > max {
            return Err(AssetError::invalid(format!(
                "{} is {} bytes (limit {max})",
                e.name, e.uncompressed_size
            )));
        }
        let bytes = self
            .p4k
            .read(e)
            .map_err(|x| AssetError::internal(format!("{}: {x}", e.name)))?;
        if bytes.len() as u64 != e.uncompressed_size {
            return Err(AssetError::internal(format!(
                "{}: read {} bytes, expected {}",
                e.name,
                bytes.len(),
                e.uncompressed_size
            )));
        }
        if e.crc32 != 0 && crc32c::crc32c(&bytes) != e.crc32 {
            return Err(AssetError::internal(format!(
                "{}: checksum mismatch",
                e.name
            )));
        }
        Ok(bytes)
    }

    /// The DataCore, read and parsed once. The bytes stay for the life of the process (the
    /// worker exits when idle; the CLI when done).
    pub fn db(&self) -> AssetResult<&Database<'static>> {
        let r = self.db.get_or_init(|| {
            let e = self
                .p4k
                .entry(DCB_ENTRY)
                .or_else(|| self.p4k.entry(DCB_FALLBACK_ENTRY))
                .ok_or_else(|| format!("{DCB_ENTRY} is missing"))?;
            let bytes = self.read(e, MAX_DCB_BYTES).map_err(|e| e.message)?;
            let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            crate::extract::guarded("DataCore parse", || {
                Database::from_bytes(bytes)
                    .map_err(|e| crate::extract::ExtractError::DataCore(e.to_string()))
            })
            .map_err(|e| e.to_string())
        });
        r.as_ref().map_err(|e| AssetError::internal(e.clone()))
    }

    /// The ATL trigger, external source and Wwise file indexes, built once on the first
    /// sounds request.
    pub fn audio(&self) -> AssetResult<&audio::AudioIndex> {
        let r = self
            .audio
            .get_or_init(|| audio::AudioIndex::build(&self.p4k).map_err(|e| e.message));
        r.as_ref().map_err(|e| AssetError::internal(e.clone()))
    }

    /// Whether the audio index is built (status only).
    pub fn audio_ready(&self) -> bool {
        self.audio.get().is_some_and(|r| r.is_ok())
    }

    /// Whether the DataCore is loaded (status only).
    pub fn db_ready(&self) -> bool {
        self.db.get().is_some_and(|r| r.is_ok())
    }
}

/// A record by GUID, full name (`EntityClassDefinition.AEGS_Javelin`) or short name
/// (`AEGS_Javelin`, preferring `EntityClassDefinition`), case-insensitively.
pub fn find_record<'d>(db: &'d Database<'static>, what: &str) -> AssetResult<&'d Record> {
    let w = what.trim();
    if let Ok(g) = w.parse::<starbreaker_common::CigGuid>()
        && let Some(r) = db.record_by_id(&g)
    {
        return Ok(r);
    }
    let lower = w.to_ascii_lowercase();
    let records = db.records();
    let name = |r: &Record| db.resolve_string2(r.name_offset);
    if let Some(r) = records.iter().find(|r| name(r).eq_ignore_ascii_case(w)) {
        return Ok(r);
    }
    let short = |r: &Record| {
        let n = name(r);
        n.rsplit('.').next().unwrap_or(n).to_ascii_lowercase()
    };
    let entity = db.struct_id("EntityClassDefinition");
    let mut hits: Vec<&Record> = records.iter().filter(|r| short(r) == lower).collect();
    hits.sort_by_key(|r| Some(r.struct_id()) != entity);
    hits.into_iter()
        .next()
        .ok_or_else(|| AssetError::not_found(format!("no record {what:?}")))
}

/// A record's display name: its name without the type prefix.
pub fn record_short_name(db: &Database<'static>, r: &Record) -> String {
    let n = db.resolve_string2(r.name_offset);
    n.rsplit('.').next().unwrap_or(n).to_string()
}

#[cfg(test)]
mod tests;
