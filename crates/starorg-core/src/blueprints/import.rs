//! Smart Citizen blueprint tracker export (addendum §4).
//!
//! `{"version": 1, "exportedAt": "<ISO>", "missions": [], "blueprints": [{"name", "completed",
//! "favorite"}]}`. Only `version` 1 is accepted. `completed: true` means owned; `false` was
//! only tracked and is not imported. Names carry Tag Builder prefixes (`[CIV-S3-A] ...`),
//! stripped before matching against the catalog.

use super::catalog::{BlueprintCatalog, index};
use super::{OwnedSet, SOURCE_SMART_CITIZEN, Source, normalize, strip_tags};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Exports larger than this are refused (the owner's 269 entries are 20 KB).
pub const EXPORT_LIMIT: u64 = 16 << 20;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportEntry {
    pub name: String,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub favorite: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub version: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub blueprints: Vec<ExportEntry>,
}

/// Parse an export. Refuses anything but version 1 with a clear message.
pub fn parse_export(bytes: &[u8]) -> Result<Export, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("not a Smart Citizen export (invalid JSON: {e})"))?;
    match v.get("version").and_then(serde_json::Value::as_u64) {
        Some(1) => {}
        Some(n) => {
            return Err(format!(
                "Smart Citizen export version {n} is not supported (only version 1)"
            ));
        }
        None => return Err("not a Smart Citizen export (no \"version\" field)".into()),
    }
    if !v.get("blueprints").is_some_and(serde_json::Value::is_array) {
        return Err("not a Smart Citizen export (no \"blueprints\" list)".into());
    }
    serde_json::from_value(v).map_err(|e| format!("Smart Citizen export: {e}"))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Ambiguous {
    pub name: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    /// Entries in the export.
    pub total: usize,
    /// `completed: false`: tracked only, not imported.
    pub not_completed: usize,
    /// Same name twice in the export (after stripping tags).
    pub duplicates: usize,
    /// New for this character, one catalog match. Original names.
    pub imported: Vec<String>,
    /// Already in this character's set.
    pub known: Vec<String>,
    /// No catalog match. Original names.
    pub unmatched: Vec<String>,
    /// Several catalog matches: stored with every candidate and flagged, never guessed.
    pub ambiguous: Vec<Ambiguous>,
    pub exported_at: Option<String>,
    /// The catalog was empty: nothing can match.
    pub empty_catalog: bool,
}

impl ImportReport {
    /// Anything to write.
    pub fn changes(&self) -> bool {
        !self.imported.is_empty() || !self.ambiguous.is_empty() || !self.known.is_empty()
    }
}

/// Merge `export` into `set` (union, never deletes). Returns the report; `set` is changed in
/// place, so pass a clone for a dry run.
pub fn merge(export: &Export, catalog: &dyn BlueprintCatalog, set: &mut OwnedSet) -> ImportReport {
    let idx = index(catalog);
    let mut rep = ImportReport {
        total: export.blueprints.len(),
        exported_at: export.exported_at.clone(),
        empty_catalog: idx.is_empty(),
        ..ImportReport::default()
    };
    let mut seen = BTreeSet::new();
    for e in &export.blueprints {
        if !e.completed {
            rep.not_completed += 1;
            continue;
        }
        let key = normalize(&e.name);
        if key.is_empty() {
            rep.unmatched.push(e.name.clone());
            continue;
        }
        if !seen.insert(key.clone()) {
            rep.duplicates += 1;
            continue;
        }
        let Some(ids) = idx.get(&key) else {
            rep.unmatched.push(e.name.clone());
            continue;
        };
        let known = set.entries.contains_key(&key);
        set.add(
            &e.name,
            Source {
                kind: SOURCE_SMART_CITIZEN.into(),
                time: export.exported_at.clone(),
                file: None,
            },
        );
        let entry = set.entries.get_mut(&key).expect("added above");
        for id in ids {
            if !entry.ids.contains(id) {
                entry.ids.push(id.clone());
            }
        }
        entry.ids.sort();
        entry.ambiguous = entry.ids.len() > 1;
        entry.favorite |= e.favorite;
        if known {
            rep.known.push(e.name.clone());
        } else if ids.len() > 1 {
            rep.ambiguous.push(Ambiguous {
                name: e.name.clone(),
                candidates: ids.clone(),
            });
        } else {
            rep.imported.push(e.name.clone());
        }
    }
    rep
}

/// Display name of an export entry (tags stripped), for messages.
pub fn display(name: &str) -> &str {
    strip_tags(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprints::SOURCE_LOG;
    use crate::blueprints::catalog::ListCatalog;

    const EXPORT: &str = r#"{"version": 1, "exportedAt": "2026-11-01T10:00:00.000Z", "missions": [],
      "blueprints": [
        {"name": "[CIV-S3-A] Fake Rifle 'Kestrel'", "completed": true, "favorite": true},
        {"name": "[H] [E-S2] Test Helmet", "completed": true, "favorite": false},
        {"name": "Test Helmet", "completed": true, "favorite": false},
        {"name": "[L] Twin Name", "completed": true, "favorite": false},
        {"name": "[M] Nowhere Item", "completed": true, "favorite": false},
        {"name": "Tracked Only", "completed": false, "favorite": true},
        {"name": "Already Logged", "completed": true, "favorite": false}
      ]}"#;

    fn catalog() -> ListCatalog {
        ListCatalog::parse(
            "bp_rifle\tFake Rifle \u{2019}Kestrel\u{2019}\nbp_helmet\tTest Helmet\nbp_twin_a\tTwin Name\nbp_twin_b\tTwin  name\nbp_logged\tAlready Logged\nbp_tracked\tTracked Only\n",
        )
    }

    #[test]
    fn version_check() {
        assert!(parse_export(EXPORT.as_bytes()).is_ok());
        let e = parse_export(br#"{"version": 2, "blueprints": []}"#).unwrap_err();
        assert!(e.contains("version 2 is not supported"), "{e}");
        assert!(parse_export(br#"{"blueprints": []}"#).is_err());
        assert!(parse_export(br#"{"version": 1}"#).is_err());
        assert!(parse_export(b"nope").is_err());
    }

    #[test]
    fn merge_reports_every_category() {
        let ex = parse_export(EXPORT.as_bytes()).unwrap();
        let mut set = OwnedSet::new(Some("1001"));
        set.add(
            "Already Logged",
            Source {
                kind: SOURCE_LOG.into(),
                time: Some("2026-10-01T00:00:00Z".into()),
                file: Some("Game.log".into()),
            },
        );
        let r = merge(&ex, &catalog(), &mut set);
        assert_eq!(r.total, 7);
        assert_eq!(r.not_completed, 1);
        assert_eq!(r.duplicates, 1);
        assert_eq!(
            r.imported,
            vec!["[CIV-S3-A] Fake Rifle 'Kestrel'", "[H] [E-S2] Test Helmet"]
        );
        assert_eq!(r.known, vec!["Already Logged"]);
        assert_eq!(r.unmatched, vec!["[M] Nowhere Item"]);
        assert_eq!(r.ambiguous.len(), 1);
        assert_eq!(r.ambiguous[0].candidates, vec!["bp_twin_a", "bp_twin_b"]);
        assert!(!set.entries.contains_key("tracked only"));
        let rifle = &set.entries["fake rifle 'kestrel'"];
        assert!(rifle.favorite);
        assert_eq!(rifle.ids, vec!["bp_rifle"]);
        assert_eq!(rifle.name, "Fake Rifle 'Kestrel'");
        assert_eq!(rifle.sources[0].kind, SOURCE_SMART_CITIZEN);
        assert_eq!(
            rifle.sources[0].time.as_deref(),
            Some("2026-11-01T10:00:00.000Z")
        );
        let twin = &set.entries["twin name"];
        assert!(twin.ambiguous);
        let logged = &set.entries["already logged"];
        assert_eq!(logged.sources.len(), 2, "log source kept, import added");
        assert_eq!(set.entries.len(), 4);

        // Second import: everything known, nothing new.
        let r2 = merge(&ex, &catalog(), &mut set);
        assert!(r2.imported.is_empty() && r2.ambiguous.is_empty());
        assert_eq!(r2.known.len(), 4);
        assert_eq!(set.entries.len(), 4);
    }

    #[test]
    fn empty_catalog_matches_nothing() {
        let ex = parse_export(EXPORT.as_bytes()).unwrap();
        let mut set = OwnedSet::new(Some("1"));
        let r = merge(&ex, &ListCatalog::default(), &mut set);
        assert!(r.empty_catalog);
        assert_eq!(r.unmatched.len(), 5);
        assert!(set.entries.is_empty());
    }
}
