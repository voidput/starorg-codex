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
            {"name": "Deadbolt III Cannon", "recordName": "EntityClassDefinition.KRIG_BallisticCannon_S3", "id": "id-deadbolt", "class": "ship", "type": "WeaponGun", "subType": "Gun", "size": 3, "manufacturer": "KRIG", "dps": 512.5, "range": 2200},
            {"name": "Acme Mining Laser S1", "recordName": "EntityClassDefinition.Mining_Laser_ACME_S1", "id": "id-ml1", "class": "ship", "type": "WeaponMining", "subType": "Gun", "size": 1,
             "modes": [{"name": "Beam", "kind": "beam", "dps": 2000}, {"name": "Beam", "kind": "beam", "dps": 1500}]}
        ], "fpsWeapons": [], "missiles": []}),
    );
    d.insert(
        "game-npc-loadouts",
        json!({"actors": [
            {"class": "PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_Grunt", "name": "Generic criminal grunt",
             "faction": "genericcriminal", "lootable": true, "wear": {"min": 0.0, "max": 0.5}, "fixed": [], "groups": [],
             "items": [{"item": "behr_lmg_ballistic_01", "slot": "Primary weapon", "chance": 0.5}]}
        ], "groups": [], "byItem": {"behr_lmg_ballistic_01": ["PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_Grunt"]}, "byLootTable": {}}),
    );
    d.insert(
        "game-mining",
        json!({"mineableElements": [
            {"name": "Raw_Quantainium", "recordName": "MineableElement.Quantainium_Raw", "id": "e1",
             "instability": 1000, "resistance": 0.95, "optimalWindowMidpoint": 0.5, "optimalWindowThinness": 2.3},
            {"name": "Sadaryx", "recordName": "MineableElement.MinableElement_FPS_Sadaryx", "id": "e2", "instability": 200, "resistance": 0}
        ], "miningModules": []}),
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
                {"entity": "rack_fs9", "weight": 1, "contains": ["behr_lmg_ballistic_01"]}]}]},
            {"name": "v3slotpreset_rockcracker", "configs": [{"name": "Sadaryx minable", "spawnTags": [], "slots": [
                {"entity": "mineablerock_fps_sadaryx", "weight": 1}]}]}
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
        "game-mining-spawns",
        json!({"rocks": [
            {"rock": "MineableRock_FPS_Sadaryx", "kind": "hand", "ores": [{"element": "minableelement_fps_sadaryx"}]},
            {"rock": "MineableRock_AsteroidRare_Quantainium", "kind": "ship", "ores": [{"element": "quantainium_raw"}]}
        ], "oreSpawns": [
            {"ore": "quantainium_raw", "location": "Stanton1", "system": "Stanton", "body": "Hurston",
             "rock": "MineableRock_AsteroidRare_Quantainium", "kind": "ship", "spawnShare": 0.12}
        ]}),
    );
    d.insert(
        "game-commodities",
        json!({"resources": [
            {"name": "Sadaryx", "recordName": "ResourceType.Sadaryx", "quality": {"tier": "FPSMineable",
             "distribution": {"min": 201, "max": 1000, "mean": 201, "stddev": 298},
             "bands": [{"start": 0, "end": 399, "value": 258}, {"start": 400, "end": 1000, "value": 700}]}},
            {"name": "Raw_Quantainium", "recordName": "ResourceType.Raw_Quantainium", "quality": {"tier": "Legendary",
             "distribution": {"min": 0, "max": 1000, "mean": 500, "stddev": 100},
             "locationOverrides": [{"location": "Pyro System", "min": 0, "max": 1000, "mean": 600, "stddev": 100}],
             "bands": [{"start": 0, "end": 499, "value": 250}, {"start": 500, "end": 1000, "value": 750}]}},
            {"name": "Laranite", "recordName": "ResourceType.Laranite", "displayName": "Laranite"}
        ], "items": [
            {"name": "Laranite_Cargo_1SCU", "resource": "Laranite", "displayName": "Laranite"}
        ]}),
    );
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
            sha256: None,
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
        // The text StarOrg Codex applied to LIVE (its sha is in our outputs list).
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
        // Commodity kiosk prices: three buy laranite, one pays nothing, one name is unknown.
        std::fs::write(
            uex.join("uex-commodity-prices.json"),
            json!({"_fetched": "2026-11-21T09:00:00.000Z", "prices": [
                {"commodity_name": "Laranite", "id_terminal": 1, "terminal_name": "Guns Ruin", "price_sell": 8700, "date_modified": 1794268800},
                {"commodity_name": "Laranite", "id_terminal": 2, "terminal_name": "Dumper's GrimHEX", "price_sell": 9612, "date_modified": 1794268800},
                {"commodity_name": "Laranite", "id_terminal": 3, "terminal_name": "TDD Area 18", "price_buy": 9900},
                {"commodity_name": "Sadaryx", "id_terminal": 1, "terminal_name": "Guns Ruin", "price_sell": 41},
                {"commodity_name": "Unobtainium", "id_terminal": 1, "terminal_name": "Guns Ruin", "price_sell": 5}
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
            cache_cap: None,
            prebuilt: None,
            serve_dumps: true,
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
    assert_eq!(init["result"]["serverInfo"]["name"], "starorg-codex");
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
            "shop_inventory",
            "where_found",
            "where_mined",
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
fn shop_inventory_lists_one_shop_by_category() {
    let w = World::new();
    let s = w.server();
    let v = ok(
        &s,
        "shop_inventory",
        json!({"terminal": "Dumper's GrimHEX"}),
    );
    assert_eq!(v["shop"]["id"], 2);
    assert_eq!(v["shop"]["location"], "Grim HEX, Yela, Crusader");
    assert_eq!(v["shop"]["system"], "Stanton");
    // The cannon is in ship-components only: linked there, a ship weapon.
    let rows = &v["items"]["rows"];
    assert_eq!(v["items"]["total"], 2);
    assert_eq!(rows[0]["name"], "FS-9 LMG");
    assert_eq!(rows[0]["category"], "fps-weapons");
    assert_eq!(rows[0]["key"], "behr_lmg_ballistic_01");
    assert_eq!(rows[0]["priceBuy"], 6900);
    assert_eq!(rows[1]["category"], "ship-weapons");
    assert_eq!(rows[1]["key"], "BEHR_LaserCannon_S3");
    assert_eq!(v["categories"][1]["label"], "Ship weapons");
    assert!(
        v["linkedWith"]
            .as_str()
            .unwrap()
            .starts_with("LIVE 4.10.200.58746")
    );
    assert!(v.get("notes").is_none());
    // A category, an id.
    let g = ok(
        &s,
        "shop_inventory",
        json!({"terminal": "2", "category": "Ship weapons"}),
    );
    assert_eq!(g["items"]["total"], 1);
    assert_eq!(g["category"], "ship-weapons");
    assert!(
        call(
            &s,
            "shop_inventory",
            json!({"terminal": "2", "category": "boats"})
        )
        .0
    );
    // A word of the place.
    let many = ok(&s, "shop_inventory", json!({"terminal": "ruin"}));
    assert_eq!(many["shop"]["name"], "Guns Ruin");
    assert!(call(&s, "shop_inventory", json!({"terminal": "nowhere at all"})).0);
    assert!(call(&s, "shop_inventory", json!({})).0);

    // Without the UEX files: a note, not an error.
    std::fs::remove_file(w.root.join("cache/uex/uex-terminals.json")).unwrap();
    let none = ok(&s, "shop_inventory", json!({"terminal": "Guns Ruin"}));
    assert!(none["notes"][0].as_str().unwrap().contains("no shop data"));
}

#[test]
fn shop_directory_for_the_app_reads_only_cached_item_data() {
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    // Nothing extracted yet: UEX names, no links, nothing extracted for this.
    let d = t.shop_directory(Some("LIVE"));
    assert_eq!(serde_json::to_value(d.state).unwrap(), json!("ready"));
    assert_eq!((d.shop_count, d.item_count), (2, 3));
    assert!(d.linked.is_none());
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    let names: Vec<&str> = d.systems.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Pyro", "Stanton"]);
    // The item search works on UEX names before any item data is read.
    let r = t.shop_item_search(Some("LIVE"), "fs-9", None);
    let best = r.item.as_ref().expect("a match");
    assert!(
        best.record.is_none() && best.key.starts_with("uex:"),
        "{best:?}"
    );
    assert!(!r.offers.is_empty());
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    // After a tool extracted item-tags, rows link.
    let s = starorg_mcp::server(w.config(), None);
    ok(&s, "item", json!({"key": "behr_lmg_ballistic_01"}));
    let i = t.shop_items(Some("LIVE"), 2);
    assert_eq!(i.shop.as_ref().unwrap().name, "Dumper's GrimHEX");
    assert_eq!(
        i.items[0].record.as_ref().unwrap().guid,
        "id-fs9",
        "{:?}",
        i.items
    );
    assert!(i.linked.is_some());
    // Linked now: the item keys on its class name, its shops cheapest first.
    let r = t.shop_item_search(Some("LIVE"), "FS-9 LMG", None);
    assert_eq!(r.item.as_ref().unwrap().key, "behr_lmg_ballistic_01");
    assert!(r.offers.windows(2).all(
        |w| w[0].price_buy.unwrap_or(f64::INFINITY) <= w[1].price_buy.unwrap_or(f64::INFINITY)
    ));
    // Ship-components not cached yet: the cannon keeps UEX's name, unlinked.
    let cannon = |i: &starorg_mcp::shops::ShopItems| {
        i.items
            .iter()
            .find(|r| r.name == "M5A Cannon")
            .unwrap()
            .record
            .clone()
    };
    assert!(cannon(&i).is_none());
    assert!(!t.shop_directory(Some("LIVE")).ship_items);
    // "Read game data" warms it; then the cannon links.
    t.warm_shop_links("LIVE").unwrap();
    assert!(t.shop_directory(Some("LIVE")).ship_items);
    assert_eq!(
        cannon(&t.shop_items(Some("LIVE"), 2)).unwrap().class_name,
        "BEHR_LaserCannon_S3"
    );
    assert!(t.warm_shop_links("NOPE").is_err());
    // No channel at all: still the shops.
    assert_eq!(t.shop_directory(Some("NOPE")).shop_count, 2);
    std::fs::remove_file(w.root.join("cache/uex/uex-item-prices.json")).unwrap();
    let gone = t.shop_directory(Some("LIVE"));
    assert_eq!(serde_json::to_value(gone.state).unwrap(), json!("missing"));
    assert!(gone.folder.unwrap().ends_with("uex"));
    assert!(t.shop_items(None, 2).shop.is_none());
    let gone = t.shop_item_search(Some("LIVE"), "fs-9", None);
    assert_eq!(serde_json::to_value(gone.state).unwrap(), json!("missing"));
    assert!(gone.items.is_empty() && gone.offers.is_empty());
}

#[test]
fn loot_screen_reads_only_cached_data() {
    use starorg_mcp::loot::{LootDataState, LootView};
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    // Nothing cached: missing, and nothing extracted for it.
    let l = t.loot_list(Some("LIVE"), LootView::Areas);
    assert_eq!(l.state, LootDataState::Missing);
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    // After a tool extracted the loot datasets (as "Read game data" does).
    let s = starorg_mcp::server(w.config(), None);
    ok(&s, "where_found", json!({"item": "FS-9 LMG"}));
    let runs = w.fake.runs.load(Ordering::SeqCst);
    let areas = t.loot_list(Some("LIVE"), LootView::Areas);
    assert_eq!(areas.state, LootDataState::Ready);
    assert_eq!(areas.build.as_deref(), Some("LIVE 4.10.200.58746"));
    assert_eq!(areas.rows.len(), 1, "{:?}", areas.rows);
    assert_eq!(areas.rows[0].id, "caves/loot_caves");
    assert_eq!(areas.rows[0].group, "Caves");
    let items = t.loot_list(Some("LIVE"), LootView::Items);
    let names: Vec<&str> = items.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["FS-9 LMG", "Verdict LMG"]);
    let q = starorg_mcp::loot::DetailQuery::default();
    let cave = t.loot_detail(Some("LIVE"), LootView::Areas, "caves/loot_caves", &q);
    assert_eq!(cave.total, 2);
    let rows: Vec<_> = cave.rows().collect();
    assert_eq!(rows.len(), 2, "{:?}", cave.groups);
    assert!(cave.groups.windows(2).all(|w| w[0].best >= w[1].best));
    let fs9 = t.loot_detail(Some("LIVE"), LootView::Items, "behr_lmg_ballistic_01", &q);
    assert_eq!(fs9.record.as_ref().unwrap().guid, "id-fs9");
    assert_eq!(fs9.rows().next().unwrap().name, "Caves");
    assert_eq!(fs9.groups[0].label, "Caves");
    // Reading the lists again extracts nothing more; the worn gear is not read yet.
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
    assert_eq!(areas.gear, LootDataState::Missing);
    assert_eq!(
        t.loot_list(Some("NOPE"), LootView::Items).state,
        LootDataState::Missing
    );
    // "Read game data" adds the NPC gear; the screen is built again with its rows.
    t.warm_npc_gear("LIVE").unwrap();
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs + 1);
    let areas = t.loot_list(Some("LIVE"), LootView::Areas);
    assert_eq!(areas.gear, LootDataState::Ready);
    let grunt = areas
        .rows
        .iter()
        .find(|r| starorg_mcp::loot::is_gear(&r.id))
        .unwrap();
    assert_eq!(grunt.name, "NPCs (worn gear): Grunt");
    let fs9 = t.loot_detail(Some("LIVE"), LootView::Items, "behr_lmg_ballistic_01", &q);
    assert!(
        fs9.group(starorg_mcp::loot::GEAR_KIND).is_some(),
        "{:?}",
        fs9.groups
    );
    // Cached: no second extraction.
    t.warm_npc_gear("LIVE").unwrap();
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs + 1);
    assert!(t.warm_npc_gear("NOPE").is_err());
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
    assert_eq!(areas[0]["spawnPoint"], "Weapons Small");
    assert_eq!(areas[0]["slotShare"], 0.75);
    assert!(areas[0].get("spawn_point").is_none(), "keys are camelCase");

    // Sadaryx has no spawn table: only the Rock Cracker spot places it.
    let mined = ok(&s, "where_mined", json!({"ore": "Sadaryx"}));
    let r = &mined["results"][0];
    assert_eq!(r["name"], "Sadaryx");
    assert_eq!(r["belts"]["total"], 0);
    assert_eq!(r["spots"]["rows"][0]["place"], "Rock Cracker, Nyx");
    assert_eq!(r["spots"]["rows"][0]["slotShare"], 1.0);
    let q = ok(&s, "where_mined", json!({"ore": "quantainium"}));
    assert_eq!(q["results"][0]["belts"]["rows"][0]["body"], "Hurston");
    let ql = &q["results"][0]["quality"];
    assert_eq!(ql["tier"], "Legendary");
    assert_eq!(ql["overrides"][0]["location"], "Pyro System");
    assert_eq!(ql["steps"][1]["shown"], 750.0);
    assert!((ql["steps"][0]["share"].as_f64().unwrap() - 0.5).abs() < 0.01);
    assert_eq!(
        ok(&s, "where_mined", json!({"ore": "FS-9 LMG"}))["matches"],
        0
    );
    assert!(call(&s, "where_mined", json!({})).0);

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
        assert_eq!(v["result"]["serverInfo"]["name"], "starorg-codex");
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

#[test]
fn commodities_for_the_app_read_only_cached_data_and_build_once() {
    use starorg_mcp::commodities::CommodityDataState as S;
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    // Nothing cached: not read, and nothing is extracted for it.
    let d = t.commodity_list(Some("LIVE"));
    assert_eq!(d.state, S::NotRead);
    assert!(d.commodities.is_empty() && d.market.loaded);
    assert_eq!(
        d.market.fetched.as_deref(),
        Some("2026-11-21T09:00:00.000Z")
    );
    assert_eq!(
        t.commodity_detail(Some("LIVE"), "Laranite").state,
        S::NotRead
    );
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    assert_eq!(t.commodity_list(None).state, S::NoChannel);
    assert_eq!(t.commodity_list(Some("NOPE")).state, S::NoChannel);
    // "Read game data" (here: the hints, which read the same datasets) fills the cache.
    t.obtain_hints("LIVE").unwrap();
    let runs = w.fake.runs.load(Ordering::SeqCst);
    let d = t.commodity_list(Some("LIVE"));
    assert_eq!(d.state, S::Ready);
    assert!(d.build.as_deref().unwrap().starts_with("LIVE "));
    let names: Vec<&str> = d.commodities.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Laranite", "Raw_Quantainium", "Sadaryx"]);
    assert!(d.commodities.iter().all(|c| c.group == "Other"));
    // Laranite: cargo and prices on both sides; not mined anywhere the data knows.
    let lar = t
        .commodity_detail(Some("LIVE"), "ResourceType.Laranite")
        .commodity
        .unwrap();
    assert_eq!(lar.cargo[0].class_name, "Laranite_Cargo_1SCU");
    let sells: Vec<(&str, f64)> = lar
        .market
        .sell
        .iter()
        .map(|k| (k.terminal.as_str(), k.price))
        .collect();
    assert_eq!(sells, [("Dumper's GrimHEX", 9612.0), ("Guns Ruin", 8700.0)]);
    assert_eq!(lar.market.buy[0].price, 9900.0);
    assert_eq!(lar.mined, Some(Vec::new()));
    // Sadaryx: quality and loot-area spots.
    let sad = t
        .commodity_detail(Some("LIVE"), "Sadaryx")
        .commodity
        .unwrap();
    assert_eq!(sad.quality.unwrap().steps[0].shown, 258.0);
    assert!(!sad.mined.unwrap()[0].spots.is_empty());
    let q = t
        .commodity_detail(Some("LIVE"), "ResourceType.Raw_Quantainium")
        .commodity
        .unwrap();
    assert_eq!(
        q.mined.unwrap()[0].belts[0].body.as_deref(),
        Some("Hurston")
    );
    // Unknown id: ready, but no commodity.
    let none = t.commodity_detail(Some("LIVE"), "Unobtainium");
    assert!(none.state == S::Ready && none.commodity.is_none());
    // Nothing was extracted for the screen.
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
    // Without the commodity prices: the market says so, the rest stays.
    std::fs::remove_file(w.root.join("cache/uex/uex-commodity-prices.json")).unwrap();
    let gone = t
        .commodity_detail(Some("LIVE"), "Laranite")
        .commodity
        .unwrap();
    assert!(!gone.market.loaded && gone.market.sell.is_empty());
    assert!(!gone.cargo.is_empty());
    assert!(!t.commodity_list(Some("LIVE")).market.loaded);
}

#[test]
fn mining_tab_reads_cached_data_and_adds_physics_once_read() {
    use starorg_mcp::mining::{CommodityDataState as S, Grade, Tool};
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    // Nothing cached: not read, and nothing is extracted for it.
    assert_eq!(t.mining_list(Some("LIVE")).state, S::NotRead);
    assert_eq!(t.mining_detail(Some("LIVE"), "Sadaryx").state, S::NotRead);
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), 0);
    assert_eq!(t.mining_list(None).state, S::NoChannel);
    assert_eq!(t.mining_detail(Some("NOPE"), "x").state, S::NoChannel);
    // The Where to get datasets: places and quality, no physics or lasers yet.
    t.obtain_hints("LIVE").unwrap();
    let runs = w.fake.runs.load(Ordering::SeqCst);
    let d = t.mining_list(Some("LIVE"));
    assert_eq!(d.state, S::Ready);
    assert!(d.build.as_deref().unwrap().starts_with("LIVE "));
    let names: Vec<&str> = d.minerals.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["Raw_Quantainium", "Sadaryx"]);
    assert!(!d.physics && !d.lasers_read);
    assert!(d.spawns_read && d.spots_read);
    let q = t
        .mining_detail(Some("LIVE"), "ResourceType.Raw_Quantainium")
        .mineral
        .unwrap();
    let p = &q.places[0];
    assert_eq!(p.body.as_deref(), Some("Hurston"));
    assert_eq!(p.quality.override_location, None);
    assert!(p.difficulty.is_none());
    assert_eq!(p.recommendation.tool, Tool::Ship);
    assert!(
        p.recommendation
            .reason
            .contains("resistance and laser power not known")
    );
    assert_eq!(q.overrides[0].places, 0, "no Pyro place in the data");
    let sad = t.mining_detail(Some("LIVE"), "Sadaryx").mineral.unwrap();
    assert!(sad.places.is_empty() && !sad.spots.is_empty());
    assert_eq!(
        w.fake.runs.load(Ordering::SeqCst),
        runs,
        "nothing extracted"
    );
    // "Read game data" adds the physics and the lasers; the index is built again.
    t.warm_mining("LIVE").unwrap();
    let d = t.mining_list(Some("LIVE"));
    assert!(d.physics && d.lasers_read);
    assert_eq!(d.lasers[0].max, 2000.0);
    let q = t
        .mining_detail(Some("LIVE"), "quantainium_raw")
        .mineral
        .unwrap();
    let p = &q.places[0];
    assert_eq!(p.difficulty.as_ref().unwrap().grade, Grade::Hard);
    assert_eq!(p.recommendation.options[0].power, Some(2000.0));
    assert!((p.recommendation.options[0].effective.unwrap() - 100.0).abs() < 1e-6);
    assert!(t.warm_mining("NOPE").is_err());
    let none = t.mining_detail(Some("LIVE"), "Unobtainium");
    assert!(none.state == S::Ready && none.mineral.is_none());
}

#[test]
fn sources_card_is_typed_and_exact() {
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    let gun = t
        .sources("LIVE", "EntityClassDefinition.behr_lmg_ballistic_01")
        .unwrap();
    let loot = gun.loot.unwrap();
    assert_eq!(loot.name, "FS-9 LMG");
    assert_eq!(loot.container_total, 1);
    assert_eq!(loot.containers[0].container, "Weapons Crate");
    assert_eq!(loot.containers[0].kind, "loot");
    assert_eq!(loot.containers[0].odds_per_box, 0.5625);
    assert!(gun.mined.is_empty());
    let buy = gun.buy.unwrap();
    assert!(buy.loaded && buy.folder.is_some());
    assert_eq!(buy.total, 2);
    assert_eq!(buy.offers[0].terminal, "Dumper's GrimHEX");
    assert_eq!(buy.offers[0].price_buy, 6900.0);
    assert_eq!(buy.offers[1].location, "Ruin Station, Terminus, Pyro");
    let ore = t.sources("LIVE", "ResourceType.Sadaryx").unwrap();
    assert!(ore.loot.is_none() && ore.buy.is_none());
    assert_eq!(ore.quality.len(), 1);
    assert_eq!(ore.quality[0].steps[0].shown, 258.0);
    assert!(gun.quality.is_empty());
    assert_eq!(
        ore.mined[0].spots[0].place.as_deref(),
        Some("Rock Cracker, Nyx")
    );
    // No fuzzy match: a near name is no item.
    let none = t.sources("LIVE", "behr_lmg_ballistic").unwrap();
    assert!(none.loot.is_none() && none.mined.is_empty());
    // Sell prices: only for a resource (or cargo holding one), best first.
    assert!(gun.sell.is_none() && none.sell.is_none());
    let lar = t
        .sources("LIVE", "ResourceType.Laranite")
        .unwrap()
        .sell
        .unwrap();
    assert!(lar.loaded);
    assert_eq!(
        (lar.resource.as_str(), lar.name.as_str()),
        ("ResourceType.Laranite", "Laranite")
    );
    let got: Vec<(&str, f64)> = lar
        .offers
        .iter()
        .map(|o| (o.terminal.as_str(), o.price_sell))
        .collect();
    assert_eq!(got, [("Dumper's GrimHEX", 9612.0), ("Guns Ruin", 8700.0)]);
    assert_eq!(lar.total, 2);
    assert_eq!(lar.offers[1].location, "Ruin Station, Terminus, Pyro");
    assert_eq!(lar.fetched.as_deref(), Some("2026-11-21T09:00:00.000Z"));
    let cargo = t
        .sources("LIVE", "EntityClassDefinition.Laranite_Cargo_1SCU")
        .unwrap();
    assert_eq!(cargo.sell.unwrap().offers.len(), 2);
    let sad = ore.sell.unwrap();
    assert_eq!(sad.offers[0].price_sell, 41.0);
    // Without the file: the section says so.
    std::fs::remove_file(w.root.join("cache/uex/uex-commodity-prices.json")).unwrap();
    let gone = t
        .sources("LIVE", "ResourceType.Laranite")
        .unwrap()
        .sell
        .unwrap();
    assert!(!gone.loaded && gone.offers.is_empty());
}

#[test]
fn obtain_hints_cover_items_and_mined_resources() {
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    let b = t.obtain_hints("LIVE").unwrap();
    assert!(b.warnings.is_empty(), "{:?}", b.warnings);
    let h = &b.hints;
    assert!(h.desc.is_empty(), "apply fills desc from its own GameData");
    // The gun: one container kind in the caves (loot and the fixed rack merge into one
    // place row each), shops from the UEX files, cheapest first, names only.
    let fs9 = &h.items["behr_lmg_ballistic_01"];
    assert_eq!(fs9.loot_count(), fs9.loot.len());
    assert!(
        fs9.loot.iter().all(|l| l.place.as_deref() == Some("Caves")),
        "{:?}",
        fs9.loot
    );
    assert!(fs9.loot.iter().any(|l| l.contents == "Weapons Crate"));
    assert!(
        fs9.loot
            .iter()
            .all(|l| l.odds.is_some_and(|o| o > 0.0 && o <= 1.0))
    );
    let shops: Vec<&str> = fs9.shops.iter().map(|s| s.terminal.as_str()).collect();
    assert_eq!(shops, ["Dumper's GrimHEX", "Guns Ruin"]);
    assert_eq!(fs9.shops[0].system.as_deref(), Some("Stanton"));
    assert!(fs9.mined.is_empty() && fs9.quality.is_none());
    // An item with no source has no entry; a non-lootable one neither.
    assert!(!h.items.contains_key("door_01"));
    assert!(!h.items.contains_key("behr_lmg_ballistic_01_mag"));
    // Resources by lower-case name: belts (body, system, kind) and spots, quality.
    let q = &h.items["raw_quantainium"];
    assert_eq!(q.mined[0].place, "Hurston");
    assert_eq!(q.mined[0].system.as_deref(), Some("Stanton"));
    assert_eq!(q.mined[0].kind.as_deref(), Some("ship"));
    let qh = q.quality.as_ref().unwrap();
    assert_eq!((qh.min, qh.max, qh.mean), (0.0, 1000.0, 500.0));
    assert_eq!(qh.overrides[0].place, "Pyro System");
    let s = &h.items["sadaryx"];
    assert_eq!(s.mined[0].place, "Rock Cracker, Nyx");
    assert_eq!(s.mined[0].kind.as_deref(), Some("hand"));
    assert_eq!(s.quality.as_ref().unwrap().mean, 201.0);
    // Sell lines per resource, best first, with the fetch time; the match rate.
    let lar = &h.items["laranite"];
    let sells: Vec<(&str, f64)> = lar
        .sells
        .iter()
        .map(|s| (s.terminal.as_str(), s.price))
        .collect();
    assert_eq!(sells, [("Dumper's GrimHEX", 9612.0), ("Guns Ruin", 8700.0)]);
    assert_eq!(lar.sells[0].place.as_deref(), Some("Grim HEX"));
    assert_eq!(lar.sells[0].system.as_deref(), Some("Stanton"));
    assert_eq!(lar.sells_count(), 2);
    assert_eq!(h.items["sadaryx"].sells[0].price, 41.0);
    assert!(fs9.sells.is_empty());
    assert_eq!(
        h.prices_fetched.as_deref(),
        Some("2026-11-21T09:00:00.000Z")
    );
    let m = b.sell_match.as_ref().unwrap();
    assert_eq!((m.commodities, m.matched), (3, 2));
    assert_eq!(m.unmatched, ["Unobtainium"]);
    // Kept in memory: no second extraction.
    let runs = w.fake.runs.load(Ordering::SeqCst);
    let again = t.obtain_hints("LIVE").unwrap();
    assert_eq!(again.items, b.items);
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
    // Kept on disk: a new store (an app restart) reads the file, even where it could not
    // build the hints again (mining cannot be extracted, yet no warning and no extraction).
    let dir = w.root.join("cache/LIVE");
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path().join("obtain-hints.json"))
        .find(|p| p.exists())
        .expect("obtain-hints.json in the build folder");
    struct NoExtract;
    impl Extractor for NoExtract {
        fn extract(&self, _: &Path, d: &str, _: &Path) -> Result<Measurement, String> {
            Err(format!("{d}: no extraction expected"))
        }
    }
    let mut cfg = w.config();
    cfg.extractor = Arc::new(NoExtract);
    let restarted = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(cfg))
        .obtain_hints("LIVE")
        .unwrap();
    assert!(restarted.warnings.is_empty(), "{:?}", restarted.warnings);
    assert_eq!(restarted.hints, b.hints);
    assert!(file.exists());
    // A fresh price file counts at once, though the rest stays cached.
    let f = w.root.join("cache/uex/uex-commodity-prices.json");
    std::fs::write(&f, json!({"_fetched": "2026-11-22T09:00:00.000Z", "prices": [
        {"commodity_name": "Laranite", "id_terminal": 1, "terminal_name": "Guns Ruin", "price_sell": 9999}
    ]}).to_string()).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&f)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let fresh = t.obtain_hints("LIVE").unwrap();
    assert_eq!(fresh.hints.items["laranite"].sells[0].price, 9999.0);
    assert_eq!(
        fresh.hints.prices_fetched.as_deref(),
        Some("2026-11-22T09:00:00.000Z")
    );
    assert!(
        !fresh
            .hints
            .items
            .get("sadaryx")
            .is_some_and(|s| !s.sells.is_empty())
    );
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
    // Without the file: no sell lines, the rest as before.
    std::fs::remove_file(&f).unwrap();
    let none = t.obtain_hints("LIVE").unwrap();
    assert!(none.hints.items.values().all(|i| i.sells.is_empty()));
    assert!(none.hints.prices_fetched.is_none() && none.sell_match.is_none());
}

#[test]
fn obtain_hints_keep_what_did_not_fail() {
    // Mining spawns cannot be extracted: loot and shops stay, mining lines go, and the
    // partial answer is not kept (the next call tries again).
    struct NoMining(Arc<FakeExtractor>);
    impl Extractor for NoMining {
        fn extract(&self, p4k: &Path, d: &str, out: &Path) -> Result<Measurement, String> {
            if d == "game-mining-spawns" {
                return Err("Extracting game-mining-spawns may need about 1.4 GB".into());
            }
            self.0.extract(p4k, d, out)
        }
    }
    let w = World::new();
    let mut cfg = w.config();
    cfg.extractor = Arc::new(NoMining(w.fake.clone()));
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(cfg));
    let b = t.obtain_hints("LIVE").unwrap();
    assert_eq!(b.warnings.len(), 1, "{:?}", b.warnings);
    assert!(b.warnings[0].starts_with("mining and quality lines left out"));
    assert!(b.hints.items.contains_key("behr_lmg_ballistic_01"));
    assert!(!b.hints.items.contains_key("raw_quantainium"));
    // Only the cache: a dataset never extracted is a warning, not an extraction.
    let w2 = World::new();
    let t2 = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w2.config()));
    let c = t2.obtain_hints_with("LIVE", false).unwrap();
    assert_eq!(w2.fake.runs.load(Ordering::SeqCst), 0);
    assert!(c.hints.items.is_empty());
    assert_eq!(c.warnings.len(), 2, "{:?}", c.warnings);
    // An unknown channel is an error.
    assert!(t.obtain_hints("NOPE").is_err());
}

/// The launch hooks' budget: one deadline for every dataset of the obtain hints, not one
/// timeout each. The "worker" sleeps far longer than the deadline; it is killed at the
/// deadline, no other dataset starts after it, nothing half-written is left, and the
/// partial answer is built again next time.
#[cfg(unix)]
#[test]
fn obtain_hints_share_one_deadline_and_stop_a_slow_worker() {
    use starorg_mcp::worker::ProcessExtractor;
    use std::time::{Duration, Instant};
    let w = World::new();
    let bin = w.root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("slow-worker");
    std::fs::write(
        &exe,
        "#!/bin/sh\ncase \"$1\" in mcp-extract-worker) exec sleep 60;; esac\nexit 0\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // Another test's fork can briefly hold the new file open (ETXTBSY): run it once first.
    let mut tries = 0;
    while std::process::Command::new(&exe).status().is_err() && tries < 50 {
        tries += 1;
        std::thread::sleep(Duration::from_millis(20));
    }
    let budget = Duration::from_millis(1500);
    let deadline = Instant::now() + budget;
    let mut cfg = w.config();
    cfg.extractor = Arc::new(ProcessExtractor::new(exe).with_deadline(deadline));
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(cfg));
    let started = Instant::now();
    let b = t.obtain_hints_until("LIVE", true, Some(deadline)).unwrap();
    let took = started.elapsed();
    // Five datasets at 1.5 s each would be 7.5 s; one shared deadline is 1.5 s.
    assert!(
        took < budget + Duration::from_millis(1500),
        "took {took:?}, warnings {:?}",
        b.warnings
    );
    assert!(b.hints.items.is_empty());
    assert!(!b.warnings.is_empty());
    assert!(
        b.warnings
            .iter()
            .any(|w| w.contains("left for the next run")),
        "{:?}",
        b.warnings
    );
    // No dataset, no temp file, no stored partial answer.
    for d in walk(&w.root.join("cache")) {
        let name = d.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            !name.starts_with("game-") && !name.contains(".tmp") && name != "obtain-hints.json",
            "{} left behind",
            d.display()
        );
    }
    // After the deadline nothing starts at all.
    let started = Instant::now();
    assert!(t.obtain_hints_until("LIVE", true, Some(deadline)).is_err());
    assert!(started.elapsed() < Duration::from_millis(500));
}

/// Every file below `dir`.
#[cfg(unix)]
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// UEX answers for `starorg_net::uex::fetch_market`: Dumper's renamed, Laranite repriced.
struct FakeUex;

impl starorg_net::uex::UexTransport for FakeUex {
    fn get(
        &self,
        endpoint: &str,
        _token: Option<&str>,
    ) -> Result<starorg_net::uex::Answer, starorg_net::uex::UexError> {
        let data = match endpoint {
            "terminals" => json!([
                {"id": 1, "nickname": "Guns Ruin", "displayname": "Ruin Station", "orbit_name": "Terminus", "star_system_name": "Pyro", "is_available": 1, "is_available_live": 1},
                {"id": 2, "nickname": "Dumper's Depot", "displayname": "Grim HEX", "moon_name": "Yela", "orbit_name": "Crusader", "star_system_name": "Stanton", "is_available": 1, "is_available_live": 1}
            ]),
            "items_prices_all" => json!([
                {"item_name": "FS-9 LMG", "item_uuid": "id-fs9", "id_terminal": 2, "terminal_name": "Dumper's Depot", "price_buy": 6500, "date_modified": 1794268800}
            ]),
            "commodities_prices_all" => json!([
                {"commodity_name": "Laranite", "id_terminal": 2, "terminal_name": "Dumper's Depot", "price_sell": 12000, "date_modified": 1794268800}
            ]),
            other => panic!("unexpected endpoint {other}"),
        };
        Ok(starorg_net::uex::Answer {
            status: 200,
            body: serde_json::to_vec(&json!({"status": "ok", "data": data})).unwrap(),
        })
    }
}

#[test]
fn a_fetch_while_the_tools_run_refreshes_every_uex_answer() {
    let w = World::new();
    let t = starorg_mcp::tools::Tools::new(starorg_mcp::store::Store::new(w.config()));
    let gun = "EntityClassDefinition.behr_lmg_ballistic_01";
    let before = t.sources("LIVE", gun).unwrap().buy.unwrap();
    assert_eq!(before.offers[0].terminal, "Dumper's GrimHEX");
    let h = t.obtain_hints("LIVE").unwrap();
    assert_eq!(
        h.hints.items["behr_lmg_ballistic_01"].shops[0].terminal,
        "Dumper's GrimHEX"
    );
    let shop_names = |t: &starorg_mcp::tools::Tools| -> Vec<String> {
        t.shop_directory(Some("LIVE"))
            .systems
            .iter()
            .flat_map(|s| &s.locations)
            .flat_map(|l| &l.shops)
            .map(|s| s.name.clone())
            .collect()
    };
    assert!(shop_names(&t).contains(&"Dumper's GrimHEX".to_string()));
    let runs = w.fake.runs.load(Ordering::SeqCst);
    // "Fetch now" with the app's own code, then a newer modified time (a coarse clock may
    // give the old second).
    let uex = w.root.join("cache/uex");
    starorg_net::uex::fetch_market(&FakeUex, &uex, None).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    for f in starorg_net::uex::FILES {
        std::fs::File::options()
            .write(true)
            .open(uex.join(f))
            .unwrap()
            .set_modified(later)
            .unwrap();
    }
    // Same Tools: the card, the shop tool and the in-game hints all see the new names.
    let buy = t.sources("LIVE", gun).unwrap().buy.unwrap();
    assert_eq!(buy.total, 1);
    assert_eq!(buy.offers[0].terminal, "Dumper's Depot");
    assert_eq!(buy.offers[0].price_buy, 6500.0);
    let lar = t
        .sources("LIVE", "ResourceType.Laranite")
        .unwrap()
        .sell
        .unwrap();
    assert_eq!(lar.offers[0].terminal, "Dumper's Depot");
    let h = t.obtain_hints("LIVE").unwrap();
    let fs9 = &h.hints.items["behr_lmg_ballistic_01"];
    let shops: Vec<&str> = fs9.shops.iter().map(|s| s.terminal.as_str()).collect();
    assert_eq!(shops, ["Dumper's Depot"]);
    assert_eq!(h.hints.items["laranite"].sells[0].price, 12000.0);
    let args = json!({"item": "FS-9 LMG"});
    let found = t.call("where_to_buy", args.as_object().unwrap()).unwrap();
    assert!(found.to_string().contains("Dumper's Depot"), "{found}");
    // The Shops screen's index follows the same stamp.
    let names = shop_names(&t);
    assert!(names.contains(&"Dumper's Depot".to_string()), "{names:?}");
    assert!(
        !names.contains(&"Dumper's GrimHEX".to_string()),
        "{names:?}"
    );
    let items = t.shop_items(Some("LIVE"), 2);
    assert_eq!(items.shop.unwrap().name, "Dumper's Depot");
    // Nothing extracted again: only the UEX-derived values were rebuilt.
    assert_eq!(w.fake.runs.load(Ordering::SeqCst), runs);
}
