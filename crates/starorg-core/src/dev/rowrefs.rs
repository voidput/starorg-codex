//! Rows that name other rows of the same dataset: a ship's loadout lists its weapons by
//! record name (`"item": "GATS_BallisticGatling_S3"`), and the weapon's own row carries
//! `"recordName": "EntityClassDefinition.GATS_BallisticGatling_S3"`. [`RowIndex`] maps every
//! row's `id`, `recordName` (full and short) and `className` to the row; [`refs_in`] finds the
//! string values of a row that hit it, so the Datasets and Diff screens can jump there.

use super::dataset::Dataset;
use crate::model::short_name;
use serde_json::Value;
use std::collections::HashMap;

/// A row of a dataset, by table and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowAt<'a> {
    pub table: &'a str,
    pub row: u32,
}

/// One referenced row: where the first reference sits, the value, the row, and how many
/// values of the row point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRef<'a> {
    pub pointer: String,
    pub value: String,
    pub at: RowAt<'a>,
    pub count: u32,
}

const KEYS: [&str; 3] = ["id", "recordName", "className"];

/// Identity values of every row of a dataset, case-insensitive. A value two rows share
/// (an AI copy and its ship with the same `className`) maps to the first.
pub struct RowIndex<'a> {
    map: HashMap<String, RowAt<'a>>,
}

impl<'a> RowIndex<'a> {
    pub fn new(d: &'a Dataset) -> Self {
        let mut map = HashMap::new();
        for t in &d.tables {
            for (i, row) in t.rows.iter().enumerate() {
                let at = RowAt {
                    table: &t.name,
                    row: i as u32,
                };
                for k in KEYS {
                    if let Some(s) = row.get(k).and_then(Value::as_str).filter(|s| s.len() > 2) {
                        map.entry(s.to_lowercase()).or_insert(at);
                        let short = short_name(s);
                        if short != s && short.len() > 2 {
                            map.entry(short.to_lowercase()).or_insert(at);
                        }
                    }
                }
            }
        }
        Self { map }
    }

    pub fn get(&self, value: &str) -> Option<RowAt<'a>> {
        self.map.get(&value.to_lowercase()).copied()
    }
}

/// The rows `row` names through its string values (not through its own identity keys),
/// itself (`own`) excluded, in order of first reference.
pub fn refs_in<'a>(row: &Value, index: &RowIndex<'a>, own: RowAt<'_>) -> Vec<RowRef<'a>> {
    fn go<'a>(
        v: &Value,
        path: &mut String,
        top: bool,
        index: &RowIndex<'a>,
        own: (&str, u32),
        out: &mut Vec<RowRef<'a>>,
    ) {
        match v {
            Value::String(s) => {
                if s.len() < 3 || s.contains(char::is_whitespace) {
                    return;
                }
                let Some(at) = index.get(s) else { return };
                if (at.table, at.row) == own {
                    return;
                }
                match out.iter_mut().find(|r| r.at == at) {
                    Some(r) => r.count += 1,
                    None => out.push(RowRef {
                        pointer: path.clone(),
                        value: s.clone(),
                        at,
                        count: 1,
                    }),
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    let len = path.len();
                    path.push('/');
                    path.push_str(&i.to_string());
                    go(x, path, false, index, own, out);
                    path.truncate(len);
                }
            }
            Value::Object(m) => {
                for (k, x) in m {
                    if top && KEYS.contains(&k.as_str()) {
                        continue;
                    }
                    let len = path.len();
                    path.push('/');
                    path.push_str(&k.replace('~', "~0").replace('/', "~1"));
                    go(x, path, false, index, own, out);
                    path.truncate(len);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    go(
        row,
        &mut String::new(),
        true,
        index,
        (own.table, own.row),
        &mut out,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::dataset::infer;
    use serde_json::json;

    #[test]
    fn a_ship_names_its_weapons_by_short_record_name() {
        let root = json!({
            "ships": [
                { "recordName": "EntityClassDefinition.RSI_Perseus", "id": "p-1",
                  "loadout": { "weapons": [
                      { "item": "GATS_BallisticGatling_S3", "count": 4 },
                      { "item": "gats_ballisticgatling_s3" },
                      { "item": "RSI_Perseus_BallisticCannon_B_S8" },
                      { "item": "Not_A_Row" } ] },
                  "self": "RSI_Perseus" }
            ],
            "weapons": [
                { "recordName": "EntityClassDefinition.GATS_BallisticGatling_S3", "id": "w-1" },
                { "recordName": "EntityClassDefinition.RSI_Perseus_BallisticCannon_B_S8", "id": "w-2" }
            ]
        });
        let d = infer("gamedata.json", 0, root);
        let idx = RowIndex::new(&d);
        let ship = &d.table("ships").unwrap().rows[0];
        let refs = refs_in(
            ship,
            &idx,
            RowAt {
                table: "ships",
                row: 0,
            },
        );
        let got: Vec<_> = refs
            .iter()
            .map(|r| (r.at.table, r.at.row, r.count, r.pointer.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("weapons", 0, 2, "/loadout/weapons/0/item"),
                ("weapons", 1, 1, "/loadout/weapons/2/item")
            ],
            "the ship's own name and its identity keys do not link"
        );
    }
}
