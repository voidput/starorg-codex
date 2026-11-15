//! Where record JSON comes from: the DataCore (via StarBreaker's JSON export) or, in tests,
//! a set of hand-written fixture records.
//!
//! Records are loaded one at a time and dropped after parsing: the export of everything
//! the extraction reads is about 600 MB of JSON on LIVE 4.10, too much to hold at once.

use super::json::record_path;
use serde_json::Value;
use std::collections::HashMap;

/// One main record, without its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMeta {
    /// Full record name, `EntityClassDefinition.AEGS_Gladius`.
    pub name: String,
    /// DataCore struct type, `EntityClassDefinition`.
    pub struct_type: String,
    /// Normalised record path (see [`record_path`]).
    pub path: String,
    /// Index into the store's own table.
    pub slot: usize,
}

impl RecordMeta {
    pub fn short_name(&self) -> &str {
        starorg_core::model::short_name(&self.name)
    }
}

/// A loaded record: GUID and the `_RecordValue_` object.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub id: String,
    pub value: Value,
}

pub trait RecordStore: Sync {
    /// All main records.
    fn records(&self) -> &[RecordMeta];
    /// Export one record. `Err` is a readable message (the caller warns and skips).
    fn load(&self, meta: &RecordMeta) -> Result<Loaded, String>;
}

/// Lookup tables over a store's records.
pub struct Index<'s> {
    pub records: &'s [RecordMeta],
    by_path: HashMap<&'s str, usize>,
    /// Lower-case short name -> entity record (`EntityClassDefinition` only).
    entities: HashMap<String, usize>,
}

impl<'s> Index<'s> {
    pub fn new(records: &'s [RecordMeta]) -> Self {
        let mut by_path = HashMap::with_capacity(records.len());
        let mut entities = HashMap::new();
        for (i, r) in records.iter().enumerate() {
            by_path.insert(r.path.as_str(), i);
            if r.struct_type == "EntityClassDefinition" {
                entities.insert(r.short_name().to_ascii_lowercase(), i);
            }
        }
        Self {
            records,
            by_path,
            entities,
        }
    }

    pub fn by_path(&self, path: &str) -> Option<&'s RecordMeta> {
        self.by_path.get(path).map(|&i| &self.records[i])
    }

    pub fn entity(&self, short: &str) -> Option<&'s RecordMeta> {
        self.entities
            .get(&short.to_ascii_lowercase())
            .map(|&i| &self.records[i])
    }

    pub fn of_type<'a>(&'a self, ty: &'a str) -> impl Iterator<Item = &'s RecordMeta> + 'a {
        self.records.iter().filter(move |r| r.struct_type == ty)
    }

    /// Entity records whose path contains `fragment`.
    pub fn entities_under<'a>(
        &'a self,
        fragment: &'a str,
    ) -> impl Iterator<Item = &'s RecordMeta> + 'a {
        self.records
            .iter()
            .filter(move |r| r.struct_type == "EntityClassDefinition" && r.path.contains(fragment))
    }
}

/// Records given as StarBreaker export envelopes plus a path, for tests and fixtures:
/// `{"_RecordName_": ..., "_RecordId_": ..., "_RecordPath_": "libs/foundry/records/....xml",
///   "_RecordValue_": {"_Type_": ..., ...}}`.
#[derive(Debug, Default)]
pub struct FixtureStore {
    metas: Vec<RecordMeta>,
    values: Vec<Loaded>,
}

impl FixtureStore {
    pub fn from_envelopes(envelopes: Vec<Value>) -> Result<Self, String> {
        let mut s = Self::default();
        for env in envelopes {
            let name = env["_RecordName_"]
                .as_str()
                .ok_or("fixture without _RecordName_")?
                .to_string();
            let path = env["_RecordPath_"]
                .as_str()
                .and_then(record_path)
                .ok_or_else(|| format!("{name}: no _RecordPath_ below libs/foundry/records/"))?;
            let value = env["_RecordValue_"].clone();
            let struct_type = value["_Type_"]
                .as_str()
                .ok_or_else(|| format!("{name}: no _Type_"))?
                .to_string();
            let id = env["_RecordId_"].as_str().unwrap_or_default().to_string();
            s.metas.push(RecordMeta {
                name,
                struct_type,
                path,
                slot: s.values.len(),
            });
            s.values.push(Loaded { id, value });
        }
        Ok(s)
    }
}

impl RecordStore for FixtureStore {
    fn records(&self) -> &[RecordMeta] {
        &self.metas
    }

    fn load(&self, meta: &RecordMeta) -> Result<Loaded, String> {
        self.values
            .get(meta.slot)
            .cloned()
            .ok_or_else(|| format!("{}: no such fixture", meta.name))
    }
}
