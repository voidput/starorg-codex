//! M1 store meets M2 data: `GameData` blueprint names feed the Smart Citizen import matcher,
//! and the per-character owned set feeds the mission generator's `[Owned]` markers.
//!
//! Synthetic fixtures only (`tests/fixtures/enhance/`); the character is made up.

use starorg_core::blueprints::catalog::{GameDataCatalog, index};
use starorg_core::blueprints::{self, Characters, OwnedSet, import};
use starorg_core::enhance::owned::{BlueprintRef, OwnedBlueprints};
use starorg_core::enhance::profile::Profile;
use starorg_core::enhance::{Generated, generate};
use starorg_core::ini::StockIni;
use starorg_core::model::GameData;
use std::path::Path;

const GEID: &str = "1000001";

fn fixture() -> (GameData, StockIni) {
    let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/enhance");
    let gd: GameData =
        serde_json::from_str(&std::fs::read_to_string(d.join("gamedata.json")).unwrap()).unwrap();
    let stock = StockIni::parse(std::fs::read(d.join("strings.ini")).unwrap());
    (gd, stock)
}

fn added(g: &Generated, stock: &StockIni, key: &str) -> String {
    let line = stock.find(key).unwrap();
    g.edits
        .get(&line)
        .map(|e| e.generated_part())
        .unwrap_or_default()
}

/// Write a channel data folder with one synthetic character whose set is `set`.
fn channel_with(set: &OwnedSet) -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(blueprints::dir(t.path())).unwrap();
    let mut chars = Characters::default();
    chars.seen(
        GEID,
        None,
        "Synthetic_Pilot_One",
        Some("2026-01-01T00:00:00Z"),
    );
    blueprints::save_characters(t.path(), &chars).unwrap();
    blueprints::save_set(t.path(), set).unwrap();
    t
}

#[test]
fn gamedata_catalog_names_blueprints_through_stock_text() {
    let (gd, stock) = fixture();
    let idx = index(&GameDataCatalog::new(&gd, &stock));
    assert_eq!(idx["filler rifle"], vec!["bp-bp_craft_zeta_rifle_01"]);
    assert_eq!(idx["filler wall"], vec!["bp-bp_craft_shld_zeta_s01_wall"]);
    assert_eq!(idx.len(), gd.blueprints.len());
}

#[test]
fn import_with_gamedata_then_owned_markers_from_the_store() {
    let (gd, stock) = fixture();
    let export = import::parse_export(
        br#"{"version":1,"exportedAt":"2026-01-01T00:00:00Z","blueprints":[
            {"name":"[P-S3] Filler rifle","completed":true},
            {"name":"[MIL-S1-B] Filler  Wall","completed":true,"favorite":true},
            {"name":"Filler pistol","completed":false},
            {"name":"Not A Real Thing","completed":true}]}"#,
    )
    .unwrap();
    let mut set = OwnedSet::new(Some(GEID));
    let rep = import::merge(&export, &GameDataCatalog::new(&gd, &stock), &mut set);
    assert!(!rep.empty_catalog);
    assert_eq!(rep.imported.len(), 2, "{rep:?}");
    assert_eq!(rep.unmatched, vec!["Not A Real Thing".to_string()]);
    assert_eq!(rep.not_completed, 1);
    assert_eq!(
        set.entries["filler rifle"].ids,
        vec!["bp-bp_craft_zeta_rifle_01"]
    );

    let dir = channel_with(&set);
    let sel = blueprints::select(dir.path(), None).unwrap();
    assert_eq!(sel.count, 2);
    assert!(sel.ids.contains("bp-bp_craft_zeta_rifle_01"));

    // By id even when the display name differs (a renamed item keeps its record).
    assert!(sel.owns(&BlueprintRef {
        id: "BP-BP_CRAFT_ZETA_RIFLE_01",
        record: "",
        name: "Renamed rifle",
    }));
    assert!(!sel.owns(&BlueprintRef {
        id: "bp-other",
        record: "BP_CRAFT_other",
        name: "Filler pistol",
    }));

    let g = generate(&gd, &stock, &Profile::default(), &sel);
    let desc = added(&g, &stock, "zeta_haul_desc");
    assert!(desc.contains("- Filler rifle [Owned]"), "{desc}");
    assert!(desc.contains("- Filler pistol\\n"), "{desc}");
    assert!(desc.contains("Filler Wall [Owned]"), "{desc}");
}

#[test]
fn log_only_entries_mark_by_name() {
    let (gd, stock) = fixture();
    // What the log scan stores: a tag-stripped name, no ids.
    let raw = serde_json::json!({
        "schema": 1, "geid": GEID,
        "entries": { "filler pistol": { "name": "Filler pistol", "sources": [{"kind": "log"}] } }
    });
    let set: OwnedSet = serde_json::from_value(raw).unwrap();
    let dir = channel_with(&set);
    let sel = blueprints::select(dir.path(), None).unwrap();
    assert!(sel.ids.is_empty());
    let g = generate(&gd, &stock, &Profile::default(), &sel);
    let desc = added(&g, &stock, "zeta_haul_desc");
    assert!(desc.contains("- Filler pistol [Owned]"), "{desc}");
    assert!(!desc.contains("Filler rifle [Owned]"), "{desc}");
}
