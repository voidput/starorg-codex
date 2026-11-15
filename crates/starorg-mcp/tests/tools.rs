//! Every tool on synthetic datasets, the stdio round trip and the HTTP transport.
//!
//! The datasets are small hand-written ones in the shapes the extraction writes; no game
//! data. A fake extractor serves them per channel, so the lazy extract-then-cache path runs
//! too (and counts how often it ran).

use serde_json::{Value, json};
use starorg_core::blueprints::{self, Characters, OwnedSet, Source};
use starorg_mcp::protocol::Server;
use starorg_mcp::store::{ChannelSource, StoreConfig};
use starorg_mcp::worker::{Extractor, Measurement};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const CAN: &str = "LootGeneration/CanGenerateAsLoot";

fn datasets(ptu: bool) -> BTreeMap<&'static str, Value> {
    let fs9_name = if ptu { "FS-9 LMG Mk II" } else { "FS-9 LMG" };
    let mut d = BTreeMap::new();
    d.insert(
        "game-item-tags",
        json!({"_build": "4.10", "items": [
            {"name": fs9_name, "className": "behr_lmg_ballistic_01", "id": "id-fs9", "type": "WeaponPersonal", "subType": "Medium", "manufacturer": "BEHR",
             "tags": [CAN, "LootGeneration/LootRarity/Common", "Manufacturer/Behring", "Weapon/FPS/Stocked/LMG/FS-9"], "ammoItems": ["behr_lmg_ballistic_01_mag"]},
            {"name": "Verdict LMG", "className": "ksar_lmg_energy_01", "id": "id-verdict", "type": "WeaponPersonal",
             "tags": [CAN, "LootGeneration/LootRarity/Common", "Manufacturer/KastakArms", "Weapon/FPS/Stocked/LMG/Verdict"]},
            {"name": "FS-9 Magazine", "className": "behr_lmg_ballistic_01_mag", "id": "id-mag", "type": "WeaponAttachment", "subType": "Magazine", "tags": ["Manufacturer/Behring"]},
            {"name": "Door", "className": "door_01", "id": "id-door", "type": "Door", "tags": []}
        ]}),
    );
    d.insert(
        "game-ships",
        json!({"_build": "4.10", "vehicles": [
            {"name": "Aegis Gladius", "displayName": "Aegis Gladius", "recordName": "EntityClassDefinition.AEGS_Gladius", "id": "id-gladius", "manufacturer": "aegs", "role": "Light Fighter", "size": 2, "crewSize": 1, "flight": {"scmSpeed": 220, "boostSpeed": 1200}, "loadout": [1, 2, 3]},
            {"name": "Anvil Arrow", "displayName": "Anvil Arrow", "recordName": "EntityClassDefinition.ANVL_Arrow", "id": "id-arrow", "manufacturer": "anvl", "role": "Light Fighter", "size": 2, "crewSize": 1, "flight": {"scmSpeed": if ptu { 240 } else { 230 }, "boostSpeed": 1250}, "loadout": []}
        ]}),
    );
    d.insert(
        "game-fps-weapons",
        json!({"weapons": [
            {"name": fs9_name, "recordName": "EntityClassDefinition.behr_lmg_ballistic_01", "id": "id-fs9", "kind": "ranged", "damagePerShot": 18, "fireRate": 750},
            {"name": "Verdict LMG", "recordName": "EntityClassDefinition.ksar_lmg_energy_01", "id": "id-verdict", "kind": "ranged", "damagePerShot": 12, "fireRate": 900}
        ]}),
    );
    d.insert(
        "game-weapons",
        json!({"shipWeapons": [
            {"name": "M5A Cannon", "recordName": "EntityClassDefinition.BEHR_LaserCannon_S3", "id": "id-m5a", "class": "ship", "type": "WeaponGun", "subType": "Gun", "size": 3, "manufacturer": "BEHR", "dps": 683.63, "range": 2404},
            {"name": "Deadbolt III Cannon", "recordName": "EntityClassDefinition.KRIG_BallisticCannon_S3", "id": "id-deadbolt", "class": "ship", "type": "WeaponGun", "subType": "Gun", "size": 3, "manufacturer": "KRIG", "dps": 512.5, "range": 2200}
        ], "fpsWeapons": [], "missiles": []}),
    );
    // Ship guns and components; item-tags above holds none of them (as on LIVE, where it
    // lists only the few with loot tags).
    d.insert(
        "game-ship-components",
        json!({"_build": "4.10", "weapons": [
            {"name": "M5A Cannon", "recordName": "EntityClassDefinition.BEHR_LaserCannon_S3", "id": "id-m5a", "category": "weapons", "type": "WeaponGun", "subType": "Gun", "size": 3, "manufacturer": "BEHR", "power": 1.3},
            {"name": "Deadbolt III Cannon", "recordName": "EntityClassDefinition.KRIG_BallisticCannon_S3", "id": "id-deadbolt", "category": "weapons", "type": "WeaponGun", "subType": "Gun", "size": 3, "manufacturer": "KRIG", "power": 1.1},
            {"name": "WEAP_Template", "isTemplate": true, "recordName": "EntityClassDefinition.WEAP_Template", "id": "id-tpl", "type": "WeaponGun"}
        ], "shields": [
            {"name": "5SA 'Rhada'", "recordName": "EntityClassDefinition.SHLD_BEHR_S01_5SA_SCItem", "id": "id-rhada", "type": "Shield", "size": 1, "manufacturer": "BEHR"}
        ], "summary": {"weapons": 3, "shields": 1}}),
    );
    d.insert(
        "game-loot",
        json!({"tables": [{"name": "Crate_Weapons", "version": "v1", "lootArchetypes": [{"archetype": "Arch_Weapons", "weight": 1}]}],
               "archetypes": [{"name": "Arch_Weapons", "primaryOrGroup": {"entries": [{"name": "LMG", "tag": "Weapon/FPS/Stocked/LMG", "weight": 1}]}}],
               "filters": [], "secondaryChoices": []}),
    );
    d.insert(
        "game-item-sources",
        json!({"areas": [
            {"name": "caves/loot_caves", "configs": [{"name": "Weapons Small", "spawnTags": [], "slots": [
                {"entity": "crate_small", "entityName": "Weapons Crate", "weight": 3, "loot": {"table": "Crate_Weapons", "version": "v1", "maxResults": 2}},
                {"entity": "rack_fs9", "weight": 1, "contains": ["behr_lmg_ballistic_01"]}]}]}
        ], "contractAwards": [
            {"generator": "TheCollector", "contract": "TheCollector_FS9", "item": "behr_lmg_ballistic_01", "amount": 1}
        ]}),
    );
    d.insert(
        "game-blueprints",
        json!({"blueprints": [
            {"name": "BP_FS9", "recordName": "CraftingBlueprintRecord.BP_FS9", "id": "bp-fs9", "displayName": "FS-9 LMG", "produces": "behr_lmg_ballistic_01",
             "rewardedBy": [{"mission": "Eliminate the Boss", "contract": "Bounty_Elite_Stanton", "pool": "BP_REWARDS_A", "chance": 1}]},
            {"name": "BP_Verdict", "recordName": "CraftingBlueprintRecord.BP_Verdict", "id": "bp-verdict", "displayName": "Verdict LMG", "produces": "ksar_lmg_energy_01"}
        ]}),
    );
    d.insert("game-wikelo-trades", json!({"trades": []}));
    d.insert(
        "game-missions",
        json!({"missions": [
            {"name": "Eliminate the Boss", "recordName": "MissionBrokerEntry.PU_EliminateBoss", "id": "m1", "title": "Eliminate the Boss", "giver": "Bounty Hunters Guild", "type": "mercenary", "reward": 14000, "currency": "UEC", "lawful": true},
            {"name": "Deliver Cargo", "recordName": "MissionBrokerEntry.PU_Deliver", "id": "m2", "title": "Deliver Cargo", "giver": "Covalex", "type": "delivery", "reward": 5000, "currency": "UEC", "lawful": true}
        ]}),
    );
    d.insert(
        "game-strings-english",
        json!({"language": "english", "strings": {
            "item_NameBEHR_LMG_FS9": fs9_name,
            "item_DescBEHR_LMG_FS9": "A light machine gun.",
            "vehicle_NameAEGS_Gladius": "Aegis Gladius"
        }}),
    );
    d
}

struct FakeExtractor {
    runs: AtomicUsize,
    by_p4k: Mutex<BTreeMap<PathBuf, bool>>,
}

impl Extractor for FakeExtractor {
    fn extract(&self, p4k: &Path, dataset: &str, out: &Path) -> Result<Measurement, String> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let ptu = *self.by_p4k.lock().unwrap().get(p4k).unwrap_or(&false);
        let v = datasets(ptu)
            .remove(dataset)
            .ok_or_else(|| format!("no fixture for {dataset}"))?;
        let text = v.to_string();
        std::fs::write(out.join(format!("{dataset}.json")), &text).map_err(|e| e.to_string())?;
        Ok(Measurement {
            dataset: dataset.into(),
            seconds: 0.01,
            peak_rss_bytes: Some(1 << 20),
            bytes: text.len() as u64,
            warnings: 0,
        })
    }
}

struct World {
    _t: tempfile::TempDir,
    root: PathBuf,
    fake: Arc<FakeExtractor>,
}

impl World {
    fn new() -> Self {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().to_path_buf();
        let fake = Arc::new(FakeExtractor {
            runs: AtomicUsize::new(0),
            by_p4k: Mutex::new(BTreeMap::new()),
        });
        for ch in ["LIVE", "PTU"] {
            let d = root.join("game").join(ch);
            std::fs::create_dir_all(d.join("Data/Localization/english")).unwrap();
            std::fs::write(d.join("Data.p4k"), ch).unwrap();
            fake.by_p4k
                .lock()
                .unwrap()
                .insert(d.join("Data.p4k"), ch == "PTU");
        }
        // The text StarOrg Strings applied to LIVE (its sha is in our outputs list).
        let applied = "\u{feff}item_NameBEHR_LMG_FS9=FS-9 LMG [Owned]\r\nitem_DescBEHR_LMG_FS9=A light machine gun.\r\n";
        std::fs::write(
            root.join("game/LIVE/Data/Localization/english/global.ini"),
            applied,
        )
        .unwrap();
        // Owned blueprints of two characters.
        let data = root.join("data/LIVE");
        let mut chars = Characters::default();
        chars.seen("111", None, "Pilot_A", Some("2026-11-10T00:00:00Z"));
        chars.seen("222", None, "Pilot_B", Some("2026-11-01T00:00:00Z"));
        blueprints::save_characters(&data, &chars).unwrap();
        let mut set = OwnedSet::new(Some("111"));
        set.add(
            "FS-9 LMG",
            Source {
                kind: "log".into(),
                time: None,
                file: None,
            },
        );
        blueprints::save_set(&data, &set).unwrap();
        // UEX price files the user supplied.
        let uex = root.join("cache/uex");
        std::fs::create_dir_all(&uex).unwrap();
        std::fs::write(
            uex.join("uex-item-prices.json"),
            json!({"_fetched": "2026-11-14T00:00:00Z", "prices": [
                {"item_name": "FS-9 LMG", "item_uuid": "id-fs9", "id_terminal": 1, "terminal_name": "Guns Ruin", "price_buy": 7029, "date_modified": 1794268800},
                {"item_name": "FS-9 LMG", "item_uuid": "id-fs9", "id_terminal": 2, "terminal_name": "Dumper's GrimHEX", "price_buy": 6900, "date_modified": 1788220800},
                {"item_name": "M5A Cannon", "item_uuid": "id-m5a", "id_terminal": 2, "terminal_name": "Dumper's GrimHEX", "price_buy": 78262, "date_modified": 1794268800}
            ]})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            uex.join("uex-terminals.json"),
            json!({"terminals": [
                {"id": 1, "nickname": "Guns Ruin", "displayname": "Ruin Station", "orbit_name": "Terminus", "star_system_name": "Pyro"},
                {"id": 2, "nickname": "Dumper's GrimHEX", "displayname": "Grim HEX", "moon_name": "Yela", "orbit_name": "Crusader", "star_system_name": "Stanton"}
            ]})
            .to_string(),
        )
        .unwrap();
        Self { _t: t, root, fake }
    }

    fn config(&self) -> StoreConfig {
        let ch = |name: &str, build: &str, data: bool| ChannelSource {
            name: name.into(),
            p4k: self.root.join("game").join(name).join("Data.p4k"),
            build: Some(build.into()),
            dir: self.root.join("game").join(name),
            data_dir: data.then(|| self.root.join("data").join(name)),
        };
        let applied = std::fs::read(
            self.root
                .join("game/LIVE/Data/Localization/english/global.ini"),
        )
        .unwrap();
        use sha2::Digest;
        let sha: String = sha2::Sha256::digest(&applied)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        StoreConfig {
            channels: vec![
                ch("LIVE", "4.10.200.58746", true),
                ch("PTU", "5.0.0.1", false),
            ],
            default_channel: None,
            cache_dir: self.root.join("cache"),
            uex_dir: Some(self.root.join("cache/uex")),
            our_outputs: vec![sha],
            extractor: self.fake.clone(),
        }
    }

    fn server(&self) -> Server {
        starorg_mcp::server(self.config(), None)
    }
}

fn call(s: &Server, name: &str, args: Value) -> (bool, Value) {
    let mut log = Vec::new();
    let r = s
        .handle(
            &json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": name, "arguments": args}}),
            &mut log,
        )
        .unwrap();
    let res = &r["result"];
    let text = res["content"][0]["text"].as_str().unwrap_or_default();
    let err = res["isError"].as_bool().unwrap_or(true);
    let v = serde_json::from_str(text).unwrap_or(Value::String(text.into()));
    (err, v)
}

fn ok(s: &Server, name: &str, args: Value) -> Value {
    let (err, v) = call(s, name, args.clone());
    assert!(!err, "{name} {args}: {v}");
    v
}

#[test]
fn lifecycle_and_schemas() {
    let w = World::new();
    let s = w.server();
    let mut log = Vec::new();
    let init = s
        .handle(
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}),
            &mut log,
        )
        .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "starorg-strings");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // Unknown versions get the newest; notifications get no answer.
    let init2 = s
        .handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}}), &mut log)
        .unwrap();
    assert_eq!(init2["result"]["protocolVersion"], "2025-06-18");
    assert!(
        s.handle(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            &mut log
        )
        .is_none()
    );
    assert_eq!(
        s.handle(
            &json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}),
            &mut log
        )
        .unwrap()["result"],
        json!({})
    );
    assert_eq!(
        s.handle(
            &json!({"jsonrpc": "2.0", "id": 4, "method": "nope"}),
            &mut log
        )
        .unwrap()["error"]["code"],
        -32601
    );
    assert_eq!(
        s.handle_text("{not json", &mut log).unwrap()["error"]["code"],
        -32700
    );

    let list = s
        .handle(
            &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}),
            &mut log,
        )
        .unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "search",
            "item",
            "where_to_buy",
            "where_found",
            "how_to_obtain",
            "mission",
            "compare",
            "string",
            "diff",
            "status"
        ]
    );
    for t in tools {
        let schema = &t["inputSchema"];
        assert_eq!(schema["type"], "object", "{}", t["name"]);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["channel"]["enum"],
            json!(["LIVE", "PTU"])
        );
        for r in schema["required"].as_array().unwrap() {
            assert!(
                schema["properties"][r.as_str().unwrap()].is_object(),
                "{}: {r}",
                t["name"]
            );
        }
        assert_eq!(t["annotations"]["readOnlyHint"], true);
        assert_eq!(t["annotations"]["openWorldHint"], false);
        assert!(!t["description"].as_str().unwrap().is_empty());
    }
    // Nothing was extracted to answer the lifecycle.
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    let unknown = s
        .handle(&json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "write_file", "arguments": {}}}), &mut log)
        .unwrap();
    assert_eq!(unknown["error"]["code"], -32602);
}

#[test]
fn search_and_item() {
    let w = World::new();
    let s = w.server();
    let v = ok(&s, "search", json!({"query": "fs9"}));
    assert_eq!(v["channel"], "LIVE");
    assert_eq!(v["rows"][0]["key"], "behr_lmg_ballistic_01");
    let ships = ok(
        &s,
        "search",
        json!({"query": "light fighter", "kind": "ship"}),
    );
    assert_eq!(ships["total"], 2);
    assert!(
        ships["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["kind"] == "ship")
    );
    // Paging.
    let p1 = ok(&s, "search", json!({"query": "lmg", "limit": 1}));
    assert_eq!(p1["rows"].as_array().unwrap().len(), 1);
    let p2 = ok(
        &s,
        "search",
        json!({"query": "lmg", "limit": 1, "cursor": p1["nextCursor"]}),
    );
    assert_ne!(p1["rows"][0]["key"], p2["rows"][0]["key"]);
    // World furniture only with all.
    assert_eq!(ok(&s, "search", json!({"query": "door"}))["total"], 0);
    assert_eq!(
        ok(&s, "search", json!({"query": "door", "all": true}))["total"],
        1
    );

    let it = ok(&s, "item", json!({"key": "behr_lmg_ballistic_01"}));
    assert_eq!(it["item"]["name"], "FS-9 LMG");
    assert_eq!(it["item"]["loot"]["canLoot"], true);
    assert_eq!(it["stats"]["dataset"], "game-fps-weapons");
    assert_eq!(it["stats"]["record"]["fireRate"], 750);
    let ship = ok(&s, "item", json!({"key": "AEGS_Gladius"}));
    assert_eq!(ship["item"]["kind"], "ship");
    assert_eq!(ship["stats"]["loadoutPorts"], 3);
    assert!(ship["stats"].get("loadout").is_none());
    // A typo finds the closest name.
    assert_eq!(
        ok(&s, "item", json!({"key": "verdcit"}))["item"]["key"],
        "ksar_lmg_energy_01"
    );
    // Each dataset was extracted once, then served from the cache.
    let runs = w.fake.runs.load(Ordering::SeqCst);
    ok(&s, "search", json!({"query": "verdict"}));
    ok(&s, "item", json!({"key": "behr_lmg_ballistic_01"}));
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
}

#[test]
fn a_name_not_found_is_marked_inexact() {
    let w = World::new();
    let s = w.server();
    // Exact names and class names carry no marker.
    for (tool, args) in [
        ("item", json!({"key": "Verdict LMG"})),
        ("item", json!({"key": "behr_lmg_ballistic_01"})),
        // Every word asked is in the name: a match, not a guess.
        ("item", json!({"key": "gladius"})),
        ("where_to_buy", json!({"item": "FS-9 LMG"})),
        ("where_found", json!({"item": "FS-9 LMG"})),
        ("how_to_obtain", json!({"item": "FS-9 LMG"})),
    ] {
        let v = ok(&s, tool, args.clone());
        assert!(v.get("inexact").is_none(), "{tool} {args}: {v}");
    }
    // A typo: the closest item answers, and says it is not what was asked.
    let it = ok(&s, "item", json!({"key": "verdcit"}));
    assert_eq!(it["item"]["key"], "ksar_lmg_energy_01");
    assert_eq!(it["inexact"]["asked"], "verdcit");
    assert_eq!(it["inexact"]["used"]["key"], "ksar_lmg_energy_01");
    assert!(it["inexact"]["others"].is_array());
    assert!(
        it["inexact"]["note"]
            .as_str()
            .unwrap()
            .contains("closest match")
    );
    for tool in ["where_to_buy", "where_found", "how_to_obtain"] {
        let v = ok(&s, tool, json!({"item": "Verdcit LMG"}));
        assert_eq!(v["inexact"]["asked"], "Verdcit LMG", "{tool}: {v}");
        assert_eq!(v["inexact"]["used"]["name"], "Verdict LMG", "{tool}");
        if tool != "how_to_obtain" {
            assert_eq!(v["results"][0]["item"]["key"], "ksar_lmg_energy_01");
        }
    }
    let c = ok(
        &s,
        "compare",
        json!({"keys": ["behr_lmg_ballistic_01", "verdcit"]}),
    );
    assert_eq!(c["inexact"][0]["asked"], "verdcit");
    assert_eq!(c["inexact"].as_array().unwrap().len(), 1);
    // Nothing close at all is still an error that points at search.
    let (err, v) = call(&s, "where_to_buy", json!({"item": "zzzzqqqq"}));
    assert!(err);
    assert!(v.as_str().unwrap().contains("search"));
}

#[test]
fn ship_weapons_and_components_item_tags_lacks() {
    let w = World::new();
    let s = w.server();
    // search finds them (from game-ship-components), not templates.
    let v = ok(&s, "search", json!({"query": "m5a cannon"}));
    assert_eq!(v["rows"][0]["key"], "BEHR_LaserCannon_S3");
    assert_eq!(
        ok(&s, "search", json!({"query": "rhada"}))["rows"][0]["key"],
        "SHLD_BEHR_S01_5SA_SCItem"
    );
    assert_eq!(
        ok(&s, "search", json!({"query": "WEAP_Template", "all": true}))["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["key"] == "WEAP_Template")
            .count(),
        0
    );
    // item by name and by class name; stats from game-weapons.
    for key in ["M5A Cannon", "BEHR_LaserCannon_S3", "behr_lasercannon_s3"] {
        let it = ok(&s, "item", json!({"key": key}));
        assert_eq!(it["item"]["key"], "BEHR_LaserCannon_S3", "{key}");
        assert_eq!(it["item"]["type"], "WeaponGun");
        assert!(it.get("inexact").is_none(), "{key}: {it}");
        assert_eq!(it["stats"]["dataset"], "game-weapons");
        assert_eq!(it["stats"]["record"]["dps"], 683.63);
    }
    let sh = ok(&s, "item", json!({"key": "5SA 'Rhada'"}));
    assert_eq!(sh["stats"]["dataset"], "game-ship-components");
    // where_to_buy: the cannon's own UEX row (matched by uuid), not some other item.
    let buy = ok(&s, "where_to_buy", json!({"item": "M5A Cannon"}));
    assert_eq!(buy["matches"], 1);
    assert!(buy.get("inexact").is_none());
    assert_eq!(buy["results"][0]["item"]["key"], "BEHR_LaserCannon_S3");
    assert_eq!(buy["results"][0]["shops"]["rows"][0]["priceBuy"], 78262);
    assert_eq!(
        buy["results"][0]["shops"]["rows"][0]["terminal"],
        "Dumper's GrimHEX"
    );
    let how = ok(&s, "how_to_obtain", json!({"item": "M5A Cannon"}));
    assert_eq!(how["results"][0]["rows"][0]["method"], "buy");
    // Words match too, and compare works across the two cannons.
    assert_eq!(
        ok(&s, "where_to_buy", json!({"item": "deadbolt iii"}))["results"][0]["item"]["key"],
        "KRIG_BallisticCannon_S3"
    );
    let c = ok(
        &s,
        "compare",
        json!({"keys": ["M5A Cannon", "Deadbolt III Cannon"]}),
    );
    let rows = c["rows"]["rows"].as_array().unwrap();
    let dps = rows.iter().find(|r| r["field"] == "dps").unwrap();
    assert_eq!(dps["values"], json!([683.63, 512.5]));
    assert!(c.get("inexact").is_none());
}

#[test]
fn where_to_buy_found_and_how_to_obtain() {
    let w = World::new();
    let s = w.server();
    let buy = ok(&s, "where_to_buy", json!({"item": "FS-9 LMG"}));
    let shops = &buy["results"][0]["shops"]["rows"];
    assert_eq!(shops[0]["terminal"], "Dumper's GrimHEX");
    assert_eq!(shops[0]["priceBuy"], 6900);
    assert_eq!(shops[0]["location"], "Grim HEX, Yela, Crusader, Stanton");
    assert!(buy.get("notes").is_none());
    let at = ok(
        &s,
        "where_to_buy",
        json!({"item": "FS-9 LMG", "place": "ruin station"}),
    );
    assert_eq!(at["results"][0]["shops"]["total"], 1);
    assert_eq!(at["results"][0]["shopsElsewhere"], 1);
    let inv = ok(&s, "where_to_buy", json!({"place": "grim hex"}));
    assert_eq!(inv["terminals"]["rows"][0]["items"][0]["name"], "FS-9 LMG");
    assert!(call(&s, "where_to_buy", json!({})).0);

    let found = ok(&s, "where_found", json!({"item": "behr_lmg_ballistic_01"}));
    // One row per area, its best spawn: the crate (3 of 4 spawns, 2 rolls) beats the rack.
    let areas = &found["results"][0]["areas"]["rows"];
    assert_eq!(found["results"][0]["areas"]["total"], 1);
    assert_eq!(areas[0]["area"], "caves: loot caves");
    assert_eq!(areas[0]["kind"], "loot");
    assert_eq!(areas[0]["container"], "Weapons Crate");
    assert_eq!(areas[0]["oddsPerBox"], 0.5625);

    let how = ok(
        &s,
        "how_to_obtain",
        json!({"item": "FS-9 LMG", "owned_by": "Pilot_A"}),
    );
    let rows = how["results"][0]["rows"].as_array().unwrap();
    let methods: Vec<&str> = rows.iter().map(|r| r["method"].as_str().unwrap()).collect();
    assert_eq!(methods, ["buy", "buy", "contract", "craft", "loot",]);
    assert_eq!(how["results"][0]["blueprints"][0]["owned"], true);
    assert_eq!(how["ownedBy"]["characters"], json!(["Pilot_A"]));
    let other = ok(
        &s,
        "how_to_obtain",
        json!({"item": "FS-9 LMG", "owned_by": "Pilot_B"}),
    );
    assert_eq!(other["results"][0]["blueprints"][0]["owned"], false);
    // active = the newest session's character (Pilot_A).
    let active = ok(
        &s,
        "how_to_obtain",
        json!({"item": "FS-9 LMG", "owned_by": "active"}),
    );
    assert_eq!(active["ownedBy"]["characters"], json!(["Pilot_A"]));
    // PTU has no app data folder: owned_by is an error there, not a guess.
    assert!(
        call(
            &s,
            "how_to_obtain",
            json!({"item": "FS-9 LMG", "owned_by": "any", "channel": "PTU"})
        )
        .0
    );
}

#[test]
fn mission_compare_string_diff_status() {
    let w = World::new();
    let s = w.server();
    let m = ok(&s, "mission", json!({"query": "boss"}));
    assert_eq!(
        m["missions"]["rows"][0]["recordName"],
        "MissionBrokerEntry.PU_EliminateBoss"
    );
    assert_eq!(
        m["contracts"]["rows"][0]["contract"],
        "Bounty_Elite_Stanton"
    );
    assert_eq!(m["contracts"]["rows"][0]["blueprints"], json!(["FS-9 LMG"]));
    let collector = ok(&s, "mission", json!({"query": "collector"}));
    assert_eq!(
        collector["contracts"]["rows"][0]["items"][0]["item"],
        "behr_lmg_ballistic_01"
    );
    assert_eq!(
        ok(&s, "mission", json!({"query": "cargo", "type": "delivery"}))["missions"]["total"],
        1
    );

    let c = ok(
        &s,
        "compare",
        json!({"keys": ["behr_lmg_ballistic_01", "ksar_lmg_energy_01"]}),
    );
    let rows = c["rows"]["rows"].as_array().unwrap();
    let fire = rows.iter().find(|r| r["field"] == "fireRate").unwrap();
    assert_eq!(fire["values"], json!([750, 900]));
    let ships = ok(
        &s,
        "compare",
        json!({"keys": ["AEGS_Gladius", "ANVL_Arrow"]}),
    );
    assert!(
        ships["rows"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["field"] == "flight.scmSpeed")
    );
    assert!(call(&s, "compare", json!({"keys": ["one"]})).0);

    let st = ok(&s, "string", json!({"key": "item_NameBEHR_LMG_FS9"}));
    assert_eq!(st["string"]["stock"], "FS-9 LMG");
    assert_eq!(st["string"]["enhanced"], "FS-9 LMG [Owned]");
    let same = ok(&s, "string", json!({"key": "item_DescBEHR_LMG_FS9"}));
    assert!(same["string"].get("enhanced").is_none());
    let found = ok(&s, "string", json!({"search": "gladius"}));
    assert_eq!(found["strings"]["total"], 1);
    assert!(call(&s, "string", json!({"key": "no_such_key"})).0);
    // PTU has no applied file: stock only.
    let ptu = ok(
        &s,
        "string",
        json!({"key": "item_NameBEHR_LMG_FS9", "channel": "PTU"}),
    );
    assert_eq!(ptu["string"]["stock"], "FS-9 LMG Mk II");
    assert!(ptu["note"].as_str().unwrap().contains("nothing applied"));

    let d = ok(&s, "diff", json!({"dataset": "items", "to": "PTU"}));
    assert_eq!(d["summary"]["items"]["changed"], 1);
    assert_eq!(d["changes"]["rows"][0]["key"], "id-fs9");
    assert_eq!(d["changes"]["rows"][0]["fields"][0]["path"], "name");
    let ds = ok(&s, "diff", json!({"dataset": "strings", "to": "PTU"}));
    assert_eq!(ds["summary"]["strings"]["changed"], 1);
    let ships = ok(
        &s,
        "diff",
        json!({"dataset": "ships", "to": "PTU", "match": "arrow"}),
    );
    assert_eq!(ships["changes"]["total"], 1);
    assert!(call(&s, "diff", json!({"dataset": "items"})).0);
    assert!(call(&s, "diff", json!({"dataset": "nonsense", "to": "PTU"})).0);
    assert!(
        call(
            &s,
            "diff",
            json!({"dataset": "items", "from_build": "0.0.0"})
        )
        .0
    );

    let status = ok(&s, "status", json!({}));
    let live = &status["channels"][0];
    assert_eq!(live["default"], true);
    let cached = live["cached"][0]["datasets"].as_array().unwrap();
    assert!(cached.contains(&json!("game-item-tags")));
    assert!(live["cached"][0]["extraction"]["game-item-tags"]["peakRssBytes"].is_number());
    assert_eq!(status["uexPresent"], true);
}

#[test]
fn bad_arguments_are_tool_errors_not_crashes() {
    let w = World::new();
    let s = w.server();
    assert!(call(&s, "search", json!({})).0);
    assert!(
        call(
            &s,
            "search",
            json!({"query": "x", "channel": "TECH-PREVIEW"})
        )
        .0
    );
    assert!(call(&s, "search", json!({"query": "x", "cursor": "abc"})).0);
    assert!(call(&s, "search", json!({"query": "x", "kind": "boat"})).0);
    assert!(call(&s, "where_found", json!({"item": "zzzzqqqq"})).0);
    // A failed extraction is an error for that call; the server keeps answering.
    let mut cfg = w.config();
    struct Failing;
    impl Extractor for Failing {
        fn extract(&self, _: &Path, d: &str, _: &Path) -> Result<Measurement, String> {
            Err(format!("cannot read {d}"))
        }
    }
    cfg.extractor = Arc::new(Failing);
    cfg.cache_dir = w.root.join("other-cache");
    let s2 = starorg_mcp::server(cfg, None);
    let (err, v) = call(&s2, "search", json!({"query": "fs9"}));
    assert!(err);
    assert!(v.as_str().unwrap().contains("cannot read"));
    assert!(!call(&s2, "status", json!({})).0);
}

#[test]
fn stdio_round_trip() {
    let w = World::new();
    let s = w.server();
    let input = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "search", "arguments": {"query": "gladius"}}}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n")
        + "\n\n";
    let mut out = Vec::new();
    let mut seen = Vec::new();
    starorg_mcp::stdio::serve(&s, input.as_bytes(), &mut out, |h| {
        seen.push(h.method.clone())
    })
    .unwrap();
    let lines: Vec<Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        lines.len(),
        3,
        "one answer per request, none for the notification"
    );
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["id"], 2);
    let text = lines[2]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("AEGS_Gladius"));
    assert_eq!(seen, ["initialize", "tools/list", "tools/call"]);
}

mod http {
    use super::*;
    use starorg_mcp::http::{HttpServer, RequestLog, new_token};
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn send(addr: std::net::SocketAddr, raw: &str) -> (u16, String, String) {
        let mut s = TcpStream::connect(addr).unwrap();
        s.write_all(raw.as_bytes()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let (head, body) = out.split_once("\r\n\r\n").unwrap_or((&out, ""));
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, head.to_string(), body.to_string())
    }

    fn post(
        addr: std::net::SocketAddr,
        token: Option<&str>,
        extra: &str,
        body: &str,
    ) -> (u16, String, String) {
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        send(
            addr,
            &format!(
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}{extra}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    #[test]
    fn needs_the_token_binds_loopback_and_stops() {
        let w = World::new();
        let token = new_token().unwrap();
        let log: Arc<Mutex<Vec<RequestLog>>> = Arc::default();
        let l2 = log.clone();
        let mut srv = HttpServer::start(
            w.server(),
            0,
            token.clone(),
            Some(Arc::new(move |r| l2.lock().unwrap().push(r))),
        )
        .unwrap();
        let addr = srv.addr();
        assert!(addr.ip().is_loopback(), "bound to {addr}");
        assert!(srv.url().starts_with("http://127.0.0.1:"));
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;

        // No token, a wrong token: 401, and the body never reached the server.
        let (st, head, _) = post(addr, None, "", init);
        assert_eq!(st, 401);
        assert!(head.contains("WWW-Authenticate: Bearer"));
        assert_eq!(post(addr, Some("0000"), "", init).0, 401);
        assert_eq!(post(addr, Some(&token[..63]), "", init).0, 401);

        // With it: initialize, a session id, then a tool call in that session.
        let (st, head, body) = post(addr, Some(&token), "", init);
        assert_eq!(st, 200, "{body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["result"]["serverInfo"]["name"], "starorg-strings");
        let sid = head
            .lines()
            .find_map(|l| l.strip_prefix("Mcp-Session-Id: "))
            .unwrap()
            .to_string();
        let hdr = format!("Mcp-Session-Id: {sid}\r\nMCP-Protocol-Version: 2025-06-18\r\n");
        let (st, _, body) = post(
            addr,
            Some(&token),
            &hdr,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search","arguments":{"query":"fs9"}}}"#,
        );
        assert_eq!(st, 200);
        let v: Value = serde_json::from_str(&body).unwrap();
        assert!(
            v["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("behr_lmg_ballistic_01")
        );
        // A notification: 202, no body.
        assert_eq!(
            post(
                addr,
                Some(&token),
                &hdr,
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
            )
            .0,
            202
        );
        // Wrong path, GET, bad version, unknown session, browser origin, not JSON.
        assert_eq!(send(addr, &format!("POST /other HTTP/1.1\r\nAuthorization: Bearer {token}\r\nContent-Length: 0\r\n\r\n")).0, 404);
        assert_eq!(
            send(
                addr,
                &format!("GET /mcp HTTP/1.1\r\nAuthorization: Bearer {token}\r\n\r\n")
            )
            .0,
            405
        );
        assert_eq!(
            post(
                addr,
                Some(&token),
                "MCP-Protocol-Version: 1999-01-01\r\n",
                init
            )
            .0,
            400
        );
        assert_eq!(
            post(addr, Some(&token), "Mcp-Session-Id: nope\r\n", init).0,
            404
        );
        assert_eq!(
            post(addr, Some(&token), "Origin: https://evil.example\r\n", init).0,
            403
        );
        assert_eq!(
            post(
                addr,
                Some(&token),
                "Origin: http://localhost:1420\r\n",
                init
            )
            .0,
            200
        );
        assert_eq!(post(addr, Some(&token), "", "{nope").0, 400);
        // Ending the session.
        assert_eq!(send(addr, &format!("DELETE /mcp HTTP/1.1\r\nAuthorization: Bearer {token}\r\nMcp-Session-Id: {sid}\r\n\r\n")).0, 200);
        assert_eq!(post(addr, Some(&token), &hdr, init).0, 404);

        let entries = log.lock().unwrap().clone();
        assert!(entries.iter().any(|e| e.status == 401));
        assert!(
            entries
                .iter()
                .any(|e| e.tool.as_deref() == Some("search") && e.ok)
        );

        srv.stop();
        assert!(
            TcpStream::connect(addr).is_err(),
            "still listening after stop"
        );
    }

    #[test]
    fn a_panicking_connection_frees_its_slot() {
        // The first 12 requests panic inside the connection thread (more than the 8 slots);
        // the server must still answer afterwards instead of a permanent 503.
        let w = World::new();
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        let srv = HttpServer::start(
            w.server(),
            0,
            new_token().unwrap(),
            Some(Arc::new(move |_| {
                if n2.fetch_add(1, Ordering::SeqCst) < 12 {
                    panic!("test: request hook fails");
                }
            })),
        )
        .unwrap();
        let addr = srv.addr();
        let raw = "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\n\r\n{}";
        // A panicked request gets no answer (empty read); 503 = no free slot; 401 = answered.
        let status = || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(raw.as_bytes()).unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            out.split_whitespace()
                .nth(1)
                .and_then(|c| c.parse::<u16>().ok())
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match status() {
                Some(401) => break,
                None | Some(503) => {}
                Some(other) => panic!("unexpected {other}"),
            }
            assert!(std::time::Instant::now() < deadline, "slots never freed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(n.load(Ordering::SeqCst) > 12);
    }

    #[test]
    fn dropping_the_server_stops_it() {
        let w = World::new();
        let srv = HttpServer::start(w.server(), 0, new_token().unwrap(), None).unwrap();
        let addr = srv.addr();
        drop(srv);
        assert!(TcpStream::connect(addr).is_err());
    }
}
