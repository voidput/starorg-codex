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
    /// Every entry name in the archive, in index order (backslash separated).
    pub fn entry_names(&self) -> impl Iterator<Item = &str> {
        self.inner.entries().iter().map(|e| e.name.as_str())
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

/// One main DataCore record as `sc-gamedata` sees it after `starbreaker dcb extract`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRecordMeta {
    /// `_RecordName_`, `SCItemManufacturer.AEGS`.
    pub name: String,
    /// The DataCore file name as stored, `libs/foundry/records/scitemmanufacturer/aegs.xml`.
    pub file_name: String,
    /// DataCore struct type (equals `_RecordValue_._Type_`).
    pub struct_type: String,
    /// `_RecordId_`, lower-case hyphenated GUID.
    pub id: String,
}

/// Every main record of a DataCore, exported on demand with the same JSON exporter the
/// StarBreaker CLI's `dcb extract --format json` uses (pretty and compact differ only in
/// whitespace). Used by `starorg-gamedata`; [`DataCoreStore`] is the narrower view for
/// `GameData`.
pub struct FullDataCore<'a> {
    db: starbreaker_datacore::Database<'a>,
    metas: Vec<RawRecordMeta>,
    positions: Vec<usize>,
}

impl<'a> FullDataCore<'a> {
    /// Parse DataCore bytes. May panic on malformed data; run under `catch_unwind`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let db = starbreaker_datacore::Database::from_bytes(bytes).map_err(|e| e.to_string())?;
        let mut metas = Vec::new();
        let mut positions = Vec::new();
        for (i, r) in db.records().iter().enumerate() {
            if !db.is_main_record(r) {
                continue;
            }
            metas.push(RawRecordMeta {
                name: db.resolve_string2(r.name_offset).to_string(),
                file_name: db.resolve_string(r.file_name_offset).to_string(),
                struct_type: db.struct_name(r.struct_id()).to_string(),
                id: r.id.to_string(),
            });
            positions.push(i);
        }
        Ok(Self {
            db,
            metas,
            positions,
        })
    }

    /// Main records, in DataCore order.
    pub fn records(&self) -> &[RawRecordMeta] {
        &self.metas
    }

    /// The export envelope (`_RecordName_`, `_RecordId_`, `_RecordValue_`, ...) of
    /// `records()[index]`, as compact JSON bytes.
    pub fn export_json(&self, index: usize) -> Result<Vec<u8>, String> {
        let pos = *self
            .positions
            .get(index)
            .ok_or_else(|| format!("record {index}: out of range"))?;
        starbreaker_datacore::export::to_json_compact(&self.db, &self.db.records()[pos])
            .map_err(|e| e.to_string())
    }

    pub fn summary(&self) -> DataCoreSummary {
        DataCoreSummary {
            records: self.db.records().len(),
            struct_defs: self.db.struct_defs().len(),
        }
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

// ---------------------------------------------------------------------------------------
// Developer mode (addendum 7): the explorer's view of the whole DataCore
// ---------------------------------------------------------------------------------------

use starorg_core::dev::explorer::{Entry, ExplorerIndex};

/// A parsed DataCore with an [`ExplorerIndex`] over every record. Entry `i` of the index is
/// record `i` of the database.
pub struct DevDatabase<'a> {
    db: starbreaker_datacore::Database<'a>,
    index: ExplorerIndex,
}

impl<'a> DevDatabase<'a> {
    /// Parse and index. May panic on malformed data; run under `catch_unwind`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let db = starbreaker_datacore::Database::from_bytes(bytes).map_err(|e| e.to_string())?;
        let entries = db
            .records()
            .iter()
            .map(|r| {
                let tag = (r.tag_offset.0 != -1)
                    .then(|| db.resolve_string2(r.tag_offset).to_string())
                    .filter(|t| !t.is_empty());
                Entry {
                    guid: r.id.to_string(),
                    name: db.resolve_string2(r.name_offset).to_string(),
                    struct_type: db.struct_name(r.struct_id()).to_string(),
                    path: db.resolve_string(r.file_name_offset).replace('\\', "/"),
                    main: db.is_main_record(r),
                    tag,
                }
            })
            .collect();
        Ok(Self {
            index: ExplorerIndex::new(entries),
            db,
        })
    }

    pub fn index(&self) -> &ExplorerIndex {
        &self.index
    }

    pub fn summary(&self) -> DataCoreSummary {
        DataCoreSummary {
            records: self.db.records().len(),
            struct_defs: self.db.struct_defs().len(),
        }
    }

    fn record(&self, i: usize) -> Result<&starbreaker_datacore::types::Record, String> {
        self.db
            .records()
            .get(i)
            .ok_or_else(|| format!("no record {i}"))
    }

    /// The record as a JSON value with references written as
    /// `{"_ref": guid, "name", "type", "path"}` (addendum 7.4).
    pub fn record_value(&self, i: usize) -> Result<serde_json::Value, String> {
        let r = self.record(i)?;
        let mut sink = ValueSink {
            index: &self.index,
            stack: Vec::new(),
            root: None,
        };
        starbreaker_datacore::walker::walk_record(&self.db, r, &mut sink)
            .map_err(|e| e.to_string())?;
        sink.root.ok_or_else(|| "empty record export".to_string())
    }

    /// StarBreaker's own JSON form (references as relative paths), as `starbreaker` writes it.
    pub fn write_export(
        &self,
        i: usize,
        w: impl std::io::Write,
        pretty: bool,
    ) -> Result<(), String> {
        let r = self.record(i)?;
        if pretty {
            starbreaker_datacore::export::write_json(&self.db, r, w)
        } else {
            starbreaker_datacore::export::write_json_compact(&self.db, r, w)
        }
        .map_err(|e| e.to_string())
    }
}

enum Frame {
    Obj(Option<String>, serde_json::Map<String, serde_json::Value>),
    Arr(Option<String>, Vec<serde_json::Value>),
}

/// Builds a [`serde_json::Value`] from the walker; references become `_ref` objects.
struct ValueSink<'i> {
    index: &'i ExplorerIndex,
    stack: Vec<Frame>,
    root: Option<serde_json::Value>,
}

impl ValueSink<'_> {
    fn put(&mut self, name: Option<&str>, v: serde_json::Value) {
        match self.stack.last_mut() {
            Some(Frame::Obj(_, m)) => {
                m.insert(name.unwrap_or_default().to_string(), v);
            }
            Some(Frame::Arr(_, a)) => a.push(v),
            None => self.root = Some(v),
        }
    }

    fn close(&mut self) {
        let (name, v) = match self.stack.pop() {
            Some(Frame::Obj(n, m)) => (n, serde_json::Value::Object(m)),
            Some(Frame::Arr(n, a)) => (n, serde_json::Value::Array(a)),
            None => return,
        };
        self.put(name.as_deref(), v);
    }

    fn num(&mut self, name: Option<&str>, n: impl Into<serde_json::Number>) {
        self.put(name, serde_json::Value::Number(n.into()));
    }

    fn float(&mut self, name: Option<&str>, f: Option<f64>) {
        let v = f
            .and_then(serde_json::Number::from_f64)
            .map_or(serde_json::Value::Null, serde_json::Value::Number);
        self.put(name, v);
    }
}

impl starbreaker_datacore::sink::ExportSink for ValueSink<'_> {
    type Error = std::io::Error;

    fn extension(&self) -> &str {
        "json"
    }
    fn begin_object(&mut self, name: Option<&str>) -> Result<(), Self::Error> {
        self.stack
            .push(Frame::Obj(name.map(str::to_string), Default::default()));
        Ok(())
    }
    fn end_object(&mut self) -> Result<(), Self::Error> {
        self.close();
        Ok(())
    }
    fn begin_array(&mut self, name: &str) -> Result<(), Self::Error> {
        self.stack
            .push(Frame::Arr(Some(name.to_string()), Vec::new()));
        Ok(())
    }
    fn end_array(&mut self) -> Result<(), Self::Error> {
        self.close();
        Ok(())
    }
    fn write_null(&mut self, n: Option<&str>) -> Result<(), Self::Error> {
        self.put(n, serde_json::Value::Null);
        Ok(())
    }
    fn write_bool(&mut self, n: Option<&str>, v: bool) -> Result<(), Self::Error> {
        self.put(n, serde_json::Value::Bool(v));
        Ok(())
    }
    fn write_i8(&mut self, n: Option<&str>, v: i8) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_i16(&mut self, n: Option<&str>, v: i16) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_i32(&mut self, n: Option<&str>, v: i32) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_i64(&mut self, n: Option<&str>, v: i64) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_u8(&mut self, n: Option<&str>, v: u8) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_u16(&mut self, n: Option<&str>, v: u16) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_u32(&mut self, n: Option<&str>, v: u32) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_u64(&mut self, n: Option<&str>, v: u64) -> Result<(), Self::Error> {
        self.num(n, v);
        Ok(())
    }
    fn write_f32(&mut self, n: Option<&str>, v: f32) -> Result<(), Self::Error> {
        // The shortest text of the f32 (as the JSON export writes it), not its f64 widening.
        let f = v
            .is_finite()
            .then(|| v.to_string().parse::<f64>().ok())
            .flatten();
        self.float(n, f);
        Ok(())
    }
    fn write_f64(&mut self, n: Option<&str>, v: f64) -> Result<(), Self::Error> {
        self.float(n, v.is_finite().then_some(v));
        Ok(())
    }
    fn write_str(&mut self, n: Option<&str>, v: &str) -> Result<(), Self::Error> {
        self.put(n, serde_json::Value::String(v.to_string()));
        Ok(())
    }
    fn write_guid(
        &mut self,
        n: Option<&str>,
        v: &starbreaker_datacore::types::CigGuid,
    ) -> Result<(), Self::Error> {
        self.put(n, serde_json::Value::String(v.to_string()));
        Ok(())
    }
    fn write_record_ref(
        &mut self,
        name: Option<&str>,
        id: &starbreaker_datacore::types::CigGuid,
        record_name: &str,
        path: &str,
    ) -> Result<(), Self::Error> {
        let guid = id.to_string();
        let target = self.index.by_guid(&guid).and_then(|i| self.index.entry(i));
        let v = serde_json::json!({
            starorg_core::dev::record::REF_KEY: guid,
            "name": record_name,
            "type": target.map(|e| e.struct_type.as_str()),
            "path": target.map_or(path, |e| e.path.as_str()),
        });
        self.put(name, v);
        Ok(())
    }
}

/// A small valid DCB built with StarBreaker's `DcbBuilder`, for tests here and in the app
/// (feature `synthetic-dcb`): `makers` manufacturer records, `items` item records that
/// reference a maker and hold a `@item_Name...` loc key, and `tags` tag records sharing one
/// file (only the last is a main record). No CIG data.
#[cfg(any(test, feature = "synthetic-dcb"))]
pub mod synthetic {
    use starbreaker_datacore::dcb_builder::DcbBuilder;
    use starbreaker_datacore::types::{
        CigGuid, DataMapping, PropertyDefinition, Record, StringId2, StructDefinition,
    };

    pub const ITEM_TYPE: &str = "SynItem";
    pub const MAKER_TYPE: &str = "SynMaker";
    pub const TAG_TYPE: &str = "SynTag";

    /// GUID of record `n` of kind `k` (0 makers, 1 items, 2 tags).
    pub fn guid(k: u8, n: u32) -> CigGuid {
        let mut b = [0u8; 16];
        b[0] = k + 1;
        b[8..12].copy_from_slice(&n.to_le_bytes());
        b[15] = 0x5a;
        CigGuid::from_bytes(b)
    }

    pub fn guid_text(k: u8, n: u32) -> String {
        guid(k, n).to_string()
    }

    pub fn item_path(n: u32) -> String {
        format!("libs/foundry/records/entities/scitem/ships/syn/syn_item_{n:03}.xml")
    }

    pub fn maker_path(n: u32) -> String {
        format!("libs/foundry/records/scitemmanufacturer/syn_maker_{n}.xml")
    }

    pub const TAG_FILE: &str = "libs/foundry/records/tagdatabase/syn_tags.xml";

    pub fn build(makers: u32, items: u32, tags: u32) -> Vec<u8> {
        let mut b = DcbBuilder::new();
        let prop = |b: &mut DcbBuilder, name: &str, s: u16, ty: u16| {
            let n = b.add_string2(name);
            b.property_defs.push(PropertyDefinition {
                name_offset: n,
                struct_index: s,
                data_type: ty,
                conversion_type: 0,
                _padding: 0,
            });
        };
        // 0 SynItem: Name (String), Size (Int32), Mass (Single), Maker (Reference),
        // Active (Boolean) = 4 + 4 + 4 + 20 + 1 bytes.
        let item_name = b.add_string2(ITEM_TYPE);
        b.struct_defs.push(StructDefinition {
            name_offset: item_name,
            parent_type_index: -1,
            attribute_count: 5,
            first_attribute_index: 0,
            struct_size: 33,
        });
        prop(&mut b, "Name", 0, 0x000A);
        prop(&mut b, "Size", 0, 0x0004);
        prop(&mut b, "Mass", 0, 0x000B);
        prop(&mut b, "Maker", 0, 0x0310);
        prop(&mut b, "Active", 0, 0x0001);
        // 1 SynMaker: Name, Code (String).
        let maker_name = b.add_string2(MAKER_TYPE);
        b.struct_defs.push(StructDefinition {
            name_offset: maker_name,
            parent_type_index: -1,
            attribute_count: 2,
            first_attribute_index: 5,
            struct_size: 8,
        });
        prop(&mut b, "Name", 1, 0x000A);
        prop(&mut b, "Code", 1, 0x000A);
        // 2 SynTag: Label (String).
        let tag_name = b.add_string2(TAG_TYPE);
        b.struct_defs.push(StructDefinition {
            name_offset: tag_name,
            parent_type_index: -1,
            attribute_count: 1,
            first_attribute_index: 7,
            struct_size: 4,
        });
        prop(&mut b, "Label", 2, 0x000A);
        // Instance data follows the data mappings: items, makers, tags.
        b.data_mappings.push(DataMapping {
            struct_count: items,
            struct_index: 0,
        });
        b.data_mappings.push(DataMapping {
            struct_count: makers,
            struct_index: 1,
        });
        b.data_mappings.push(DataMapping {
            struct_count: tags,
            struct_index: 2,
        });
        let tag_file = b.add_string1(TAG_FILE);
        for n in 0..items {
            let name = b.add_string2(&format!("SYN_Item_{n:03}"));
            let file = b.add_string1(&item_path(n));
            let loc = b.add_string1(&format!("@item_NameSYN_Item_{n:03}"));
            b.records.push(Record {
                name_offset: name,
                file_name_offset: file,
                tag_offset: StringId2(-1),
                struct_index: 0,
                id: guid(1, n),
                instance_index: n as u16,
                struct_size: 33,
            });
            b.instance_data.extend_from_slice(&loc.0.to_le_bytes());
            b.instance_data
                .extend_from_slice(&((n % 4 + 1) as i32).to_le_bytes());
            b.instance_data
                .extend_from_slice(&(n as f32 * 0.5 + 0.1).to_le_bytes());
            let maker = if makers > 0 {
                guid(0, n % makers)
            } else {
                CigGuid::EMPTY
            };
            b.instance_data.extend_from_slice(&0i32.to_le_bytes());
            b.instance_data.extend_from_slice(&maker.bytes);
            b.instance_data.push(u8::from(n % 2 == 0));
        }
        for n in 0..makers {
            let name = b.add_string2(&format!("SYN_Maker_{n}"));
            let file = b.add_string1(&maker_path(n));
            let display = b.add_string1(&format!("Synthetic Maker {n}"));
            let code = b.add_string1(&format!("SYN{n}"));
            b.records.push(Record {
                name_offset: name,
                file_name_offset: file,
                tag_offset: StringId2(-1),
                struct_index: 1,
                id: guid(0, n),
                instance_index: n as u16,
                struct_size: 8,
            });
            b.instance_data.extend_from_slice(&display.0.to_le_bytes());
            b.instance_data.extend_from_slice(&code.0.to_le_bytes());
        }
        for n in 0..tags {
            let name = b.add_string2(&format!("SynTag{n}"));
            let label = b.add_string1(&format!("tag {n}"));
            b.records.push(Record {
                name_offset: name,
                file_name_offset: tag_file,
                tag_offset: StringId2(-1),
                struct_index: 2,
                id: guid(2, n),
                instance_index: n as u16,
                struct_size: 4,
            });
            b.instance_data.extend_from_slice(&label.0.to_le_bytes());
        }
        b.build()
    }
}
