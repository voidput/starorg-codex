//! Thin adapter over the StarBreaker crates.
//!
//! Everything StarBreaker-specific goes through this module, so a rev bump touches this file
//! (and the pin in the workspace `Cargo.toml`) only. Nothing outside `starorg-data` sees a
//! StarBreaker type.

use crate::gamedata::json::record_path;
use crate::gamedata::store::{Loaded, RecordMeta, RecordStore};
use std::path::Path;

/// The StarBreaker commit we build against. Part of the cache key (spec 4.2). Must equal the
/// `rev` in the workspace `Cargo.toml`; a test checks this.
pub const STARBREAKER_COMMIT: &str = "041e559215e18e1b13b124cab0e3ecb0f3aa50d4";

/// Error from the StarBreaker layer, as text plus a kind we can act on.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("cannot open archive: {0}")]
    Open(String),
    #[error("cannot read entry {name}: {detail}")]
    Read { name: String, detail: String },
}

/// Metadata of one archive entry that we use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub name: String,
    pub uncompressed_size: u64,
    pub crc32: u32,
}

/// An opened `Data.p4k`. Holds StarBreaker's in-memory index (about 400 MB on LIVE); drop it
/// as soon as the needed entries are read (spec 3.3 step 4).
pub struct Archive {
    inner: starbreaker_p4k::MappedP4k,
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        starbreaker_p4k::MappedP4k::open(path)
            .map(|inner| Self { inner })
            .map_err(|e| ArchiveError::Open(e.to_string()))
    }

    /// Number of entries StarBreaker parsed from the central directory.
    pub fn parsed_entries(&self) -> usize {
        self.inner.len()
    }

    /// Look up an entry by its exact name (backslash separated), then case-insensitively.
    pub fn entry(&self, name: &str) -> Option<EntryInfo> {
        let e = self
            .inner
            .entry(name)
            .or_else(|| self.inner.entry_case_insensitive(name))?;
        Some(EntryInfo {
            name: e.name.clone(),
            uncompressed_size: e.uncompressed_size,
            crc32: e.crc32,
        })
    }

    /// Read and decompress an entry found with [`Archive::entry`].
    pub fn read(&self, info: &EntryInfo) -> Result<Vec<u8>, ArchiveError> {
        let entry = self
            .inner
            .entry(&info.name)
            .ok_or_else(|| ArchiveError::Read {
                name: info.name.clone(),
                detail: "entry vanished from index".into(),
            })?;
        self.inner.read(entry).map_err(|e| ArchiveError::Read {
            name: info.name.clone(),
            detail: e.to_string(),
        })
    }
}

/// Summary of a parsed DataCore database. Record and struct counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataCoreSummary {
    pub records: usize,
    pub struct_defs: usize,
}

/// Record paths the `GameData` passes read (substring match on the normalised path).
const SELECTED_PATHS: [&str; 9] = [
    "entities/scitem/ships/",
    "entities/scitem/weapons/",
    "entities/spaceships/",
    "entities/groundvehicles/",
    "inventorycontainers/",
    "ammoparams/",
    "crafting/",
    "contracts/",
    "reputation/",
];

/// A parsed DataCore exposed as a [`RecordStore`]: main records only, each exported on
/// demand with StarBreaker's compact JSON exporter.
pub struct DataCoreStore<'a> {
    db: starbreaker_datacore::Database<'a>,
    metas: Vec<RecordMeta>,
    /// `metas[i].slot` indexes this: position in `db.records()`.
    positions: Vec<usize>,
}

impl<'a> DataCoreStore<'a> {
    /// Parse DataCore bytes. May panic on malformed data; run under `catch_unwind`.
    ///
    /// Every main `EntityClassDefinition` is listed (loadouts and blueprints reference
    /// entities anywhere by name or path); other records only below [`SELECTED_PATHS`].
    pub fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let db = starbreaker_datacore::Database::from_bytes(bytes).map_err(|e| e.to_string())?;
        let mut metas = Vec::new();
        let mut positions = Vec::new();
        for (i, r) in db.records().iter().enumerate() {
            if !db.is_main_record(r) {
                continue;
            }
            let struct_type = db.struct_name(r.struct_id());
            let Some(path) = record_path(db.resolve_string(r.file_name_offset)) else {
                continue;
            };
            if struct_type != "EntityClassDefinition"
                && !SELECTED_PATHS.iter().any(|p| path.contains(p))
            {
                continue;
            }
            metas.push(RecordMeta {
                name: db.resolve_string2(r.name_offset).to_string(),
                struct_type: struct_type.to_string(),
                path,
                slot: positions.len(),
            });
            positions.push(i);
        }
        Ok(Self {
            db,
            metas,
            positions,
        })
    }

    pub fn summary(&self) -> DataCoreSummary {
        DataCoreSummary {
            records: self.db.records().len(),
            struct_defs: self.db.struct_defs().len(),
        }
    }

    /// Whether the DataCore defines a struct type of this name.
    pub fn has_struct(&self, name: &str) -> bool {
        self.db.struct_index_by_name(name).is_some()
    }
}

impl DataCoreStore<'_> {
    /// The loc-reference pass (addendum 8.2) over every main record, in parallel. Records the
    /// walker cannot read are skipped (the pass only feeds grouping).
    pub fn scan_key_refs(&self) -> Vec<crate::keyrefs::Scanned> {
        use rayon::prelude::*;
        let db = &self.db;
        db.records()
            .par_iter()
            .filter(|r| db.is_main_record(r))
            .filter_map(|r| {
                let path = record_path(db.resolve_string(r.file_name_offset))?;
                let mut sink = ScanSink::default();
                starbreaker_datacore::walker::walk_record(db, r, &mut sink).ok()?;
                Some(crate::keyrefs::Scanned {
                    struct_type: db.struct_name(r.struct_id()).to_string(),
                    path,
                    scan: sink.0.into(),
                })
            })
            .collect()
    }
}

/// An export sink that builds no output: it forwards strings, sizes and record references to
/// [`crate::keyrefs::RecordScan`].
#[derive(Default)]
struct ScanSink(crate::keyrefs::RecordScan);

impl starbreaker_datacore::sink::ExportSink for ScanSink {
    type Error = std::io::Error;

    fn extension(&self) -> &str {
        "json"
    }
    fn begin_object(&mut self, name: Option<&str>) -> Result<(), Self::Error> {
        self.0.begin(name);
        Ok(())
    }
    fn end_object(&mut self) -> Result<(), Self::Error> {
        self.0.end();
        Ok(())
    }
    fn begin_array(&mut self, name: &str) -> Result<(), Self::Error> {
        self.0.begin(Some(name));
        Ok(())
    }
    fn end_array(&mut self) -> Result<(), Self::Error> {
        self.0.end();
        Ok(())
    }
    fn write_null(&mut self, _: Option<&str>) -> Result<(), Self::Error> {
        Ok(())
    }
    fn write_bool(&mut self, _: Option<&str>, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn write_i8(&mut self, n: Option<&str>, v: i8) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_i16(&mut self, n: Option<&str>, v: i16) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_i32(&mut self, n: Option<&str>, v: i32) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_i64(&mut self, n: Option<&str>, v: i64) -> Result<(), Self::Error> {
        self.0.int(n, v);
        Ok(())
    }
    fn write_u8(&mut self, n: Option<&str>, v: u8) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_u16(&mut self, n: Option<&str>, v: u16) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_u32(&mut self, n: Option<&str>, v: u32) -> Result<(), Self::Error> {
        self.0.int(n, v.into());
        Ok(())
    }
    fn write_u64(&mut self, n: Option<&str>, v: u64) -> Result<(), Self::Error> {
        self.0.int(n, i64::try_from(v).unwrap_or(i64::MAX));
        Ok(())
    }
    fn write_f32(&mut self, _: Option<&str>, _: f32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn write_f64(&mut self, _: Option<&str>, _: f64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn write_str(&mut self, n: Option<&str>, v: &str) -> Result<(), Self::Error> {
        self.0.string(n, v);
        Ok(())
    }
    fn write_guid(
        &mut self,
        _: Option<&str>,
        _: &starbreaker_datacore::types::CigGuid,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn write_record_ref(
        &mut self,
        name: Option<&str>,
        _: &starbreaker_datacore::types::CigGuid,
        _: &str,
        path: &str,
    ) -> Result<(), Self::Error> {
        self.0.record_ref(name, path);
        Ok(())
    }
}

impl RecordStore for DataCoreStore<'_> {
    fn records(&self) -> &[RecordMeta] {
        &self.metas
    }

    fn load(&self, meta: &RecordMeta) -> Result<Loaded, String> {
        let pos = *self
            .positions
            .get(meta.slot)
            .ok_or_else(|| format!("{}: bad slot", meta.name))?;
        let record = &self.db.records()[pos];
        let bytes = starbreaker_datacore::export::to_json_compact(&self.db, record)
            .map_err(|e| e.to_string())?;
        let mut env: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("export is not JSON: {e}"))?;
        let id = env["_RecordId_"].as_str().unwrap_or_default().to_string();
        let value = env
            .get_mut("_RecordValue_")
            .map(serde_json::Value::take)
            .ok_or("export has no _RecordValue_")?;
        Ok(Loaded { id, value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_constant_matches_workspace_pin() {
        let manifest = include_str!("../../../Cargo.toml");
        let pins: Vec<&str> = manifest
            .lines()
            .filter(|l| l.starts_with("starbreaker-"))
            .collect();
        assert_eq!(pins.len(), 3, "three StarBreaker crates are pinned");
        for line in pins {
            assert!(
                line.contains("https://github.com/voidput/StarBreaker"),
                "StarBreaker must come from our mirror: {line}"
            );
            assert!(
                line.contains(&format!("rev = \"{STARBREAKER_COMMIT}\"")),
                "pin and STARBREAKER_COMMIT differ: {line}"
            );
        }
    }

    #[test]
    fn open_missing_file_is_an_error() {
        let err = Archive::open(Path::new("/nonexistent/Data.p4k")).err();
        assert!(matches!(err, Some(ArchiveError::Open(_))));
    }
}
