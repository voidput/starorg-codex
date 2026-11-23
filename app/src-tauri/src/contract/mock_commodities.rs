//! The mock's Commodities screen data: a synthetic `game-commodities` (resources in the
//! game's group shapes, cargo items, cargo damage), mining spawns and loot areas, and UEX
//! commodity kiosk prices at the mock shop terminals ([`super::mock_shops::terminal_rows`]).
//! The real list, detail, mining and price code ([`starorg_mcp::commodities`]) answers over
//! them. Names are made up or generic; no UEX or CIG data.

use super::mock::MOCK_UEX_FETCHED;
use super::mock_shops::{MOCK_UEX_FOLDER, terminal_rows};
use super::types::*;
use serde_json::{Value, json};
use starorg_mcp::commodities::{
    CommodityData, Kiosks, LootArea, MiningSpawns, commodity_detail, commodity_list,
    kiosks_from_files, quality_resources, where_mined,
};
use std::sync::OnceLock;

/// The build the mock data is of.
pub const MOCK_COMMODITY_BUILD: &str = "LIVE 4.10.201.48085";

fn res(name: &str, display: &str, group: &str, sub: &[&str], extra: Value) -> Value {
    let mut v = json!({
        "name": name, "recordName": format!("ResourceType.{name}"), "id": format!("res-{}", name.to_lowercase()),
        "displayName": display, "group": group, "subgroups": sub, "densityGramsPerCc": 1.0,
    });
    if let (Value::Object(m), Value::Object(x)) = (&mut v, extra) {
        m.extend(x);
    }
    v
}

fn quality(tier: &str, min: f64, max: f64, mean: f64, sd: f64) -> Value {
    json!({"tier": tier, "distribution": {"min": min, "max": max, "mean": mean, "stddev": sd},
           "bands": [{"start": 0, "end": 399, "value": 300}, {"start": 400, "end": 699, "value": 560},
                     {"start": 700, "end": 899, "value": 800}, {"start": 900, "end": 1000, "value": 950}]})
}

fn data_json() -> Value {
    let mut r = vec![
        res(
            "Synthium",
            "Synthium",
            "Metal",
            &[],
            json!({"description": "A synthetic test metal.", "densityGramsPerCc": 7.9}),
        ),
        res(
            "Ore_Synthium",
            "Synthium (Ore)",
            "Metal",
            &["UnrefinedOres"],
            json!({"refinesTo": "Synthium", "quality": quality("Common", 0.0, 1000.0, 400.0, 180.0)}),
        ),
        res("Mockite", "Mockite", "Metal", &[], json!({})),
        res(
            "Ore_Mockite",
            "Mockite (Ore)",
            "Metal",
            &["UnrefinedOres"],
            json!({"refinesTo": "Mockite", "quality": quality("Rare", 200.0, 1000.0, 500.0, 200.0)}),
        ),
        res("Fixtanium", "Fixtanium", "Mineral", &[], json!({})),
        res(
            "Raw_Fixtanium",
            "Fixtanium (Raw)",
            "Mineral",
            &["Raw_Minerals"],
            json!({
            "description": "An unstable test mineral.", "refinesTo": "Fixtanium",
            "volatility": {"volatility": 1, "healthDecayPerSecond": 0.0675},
            "quality": {"tier": "Legendary", "distribution": {"min": 501, "max": 1000, "mean": 500, "stddev": 203},
                        "locationOverrides": [{"location": "Nyx System", "min": 501, "max": 1000, "mean": 500, "stddev": 225}],
                        "bands": [{"start": 0, "end": 599, "value": 514}, {"start": 600, "end": 899, "value": 762}, {"start": 900, "end": 1000, "value": 974}]}}),
        ),
        res(
            "Stubbite",
            "Stubbite",
            "Mineral",
            &["Raw_Minerals"],
            json!({"quality": quality("FPSMineable", 201.0, 1000.0, 201.0, 298.0)}),
        ),
        res("Hydrogen", "Hydrogen", "Gas", &[], json!({})),
        res(
            "Testium_Gas",
            "Testium Gas",
            "Gas",
            &[],
            json!({"volatility": {"volatility": 0.5}}),
        ),
        res("Fluorine", "Fluorine", "Nonmetal", &["Halogen"], json!({})),
        res(
            "Medical_Mocks",
            "Medical Supplies",
            "ProcessedGoods",
            &[],
            json!({}),
        ),
        res(
            "Mock_Fabric",
            "Mock Fabric",
            "ProcessedGoods",
            &["SyntheticMaterials"],
            json!({}),
        ),
        res(
            "Mock_Rations",
            "Mock Rations",
            "Organic",
            &["Food"],
            json!({}),
        ),
        res("Glowcap", "Glowcap Mushrooms", "Organic", &[], json!({})),
        res(
            "Mock_Stims",
            "Mock Stims",
            "ProcessedGoods",
            &["Vice"],
            json!({}),
        ),
        res("Scrap", "Scrap", "Waste", &[], json!({})),
        res("HeatPlaceholder", "Heat", "HeatPlaceholder", &[], json!({})),
    ];
    // Enough bulk goods for the list to scroll.
    for i in 1..=24 {
        r.push(res(
            &format!("Bulk_Good_{i:02}"),
            &format!("Bulk Good {i:02}"),
            "ProcessedGoods",
            &["Bulk_Supplies"],
            json!({}),
        ));
    }
    let item = |name: &str, display: &str, ty: &str, resource: &str, scu: f64| {
        json!({"name": name, "recordName": format!("EntityClassDefinition.{name}"), "id": format!("item-{name}"),
               "displayName": display, "type": ty, "category": "mock", "resource": resource, "capacityScu": scu})
    };
    json!({
        "resources": r,
        "items": [
            item("synthium", "Synthium", "Metal", "Synthium", 0.01),
            item("synthium_ore", "Synthium (Ore)", "Metal", "Ore_Synthium", 0.01),
            item("fixtanium_raw", "Fixtanium (Raw)", "Mineral", "Raw_Fixtanium", 0.01),
            item("hydrogen", "Hydrogen", "Gas", "Hydrogen", 1.0),
            item("medical_mocks", "Medical Supplies", "Medical", "Medical_Mocks", 1.0),
        ],
        "damage": [
            {"name": "CommodityDamageConfigBase", "minimumSpeed": 15, "gracePeriod": 10, "explosionDamage": 375,
             "explosionMinRadius": 0.1, "explosionMaxRadius": 2.1},
            {"name": "CommodityDamageConfigMole", "minimumSpeed": 15, "gracePeriod": 10, "explosionDamage": 375,
             "explosionMinRadius": 0.1, "explosionMaxRadius": 2.1}
        ],
    })
}

fn spawns_json() -> Value {
    json!({
        "rocks": [
            {"rock": "MineableRock_Synthium", "kind": "ship", "ores": [{"element": "synthium_ore"}]},
            {"rock": "MineableRock_Asteroid_Fixtanium", "kind": "ship", "ores": [{"element": "fixtanium_raw"}]},
            {"rock": "MineableRock_GroundVehicle_Mockite", "kind": "vehicle", "ores": [{"element": "mockite_ore"}]},
            {"rock": "MineableRock_FPS_Stubbite", "kind": "hand", "ores": [{"element": "minableelement_fps_stubbite"}]}
        ],
        "oreSpawns": [
            {"ore": "synthium_ore", "location": "Stanton1", "system": "Stanton", "body": "Hurston", "rock": "MineableRock_Synthium", "kind": "ship", "spawnShare": 0.4, "minPercent": 20, "maxPercent": 45},
            {"ore": "synthium_ore", "location": "Stanton2c", "system": "Stanton", "body": "Yela", "rock": "MineableRock_Synthium", "kind": "ship", "spawnShare": 0.25},
            {"ore": "fixtanium_raw", "location": "StantonBelt_Aaron", "system": "Stanton", "body": "Aaron Halo", "rock": "MineableRock_Asteroid_Fixtanium", "kind": "ship", "spawnShare": 0.08, "minPercent": 10, "maxPercent": 30},
            {"ore": "fixtanium_raw", "location": "Pyro1", "system": "Pyro", "body": "Pyro I", "rock": "MineableRock_Asteroid_Fixtanium", "kind": "ship", "spawnShare": 0.05},
            {"ore": "mockite_ore", "location": "Stanton3a", "system": "Stanton", "body": "Lyria", "rock": "MineableRock_GroundVehicle_Mockite", "kind": "vehicle", "spawnShare": 0.3}
        ]
    })
}

fn areas_json() -> Value {
    json!([
        {"name": "caves/loot_caves_occupied", "configs": [
            {"name": "FPS mineables", "slots": [
                {"entity": "MineableRock_FPS_Stubbite", "weight": 3},
                {"entity": "Other_Rock", "weight": 1}
            ]}
        ]},
        {"name": "events/rockcracker_mineables", "configs": [
            {"name": "Event rocks", "slots": [{"entity": "MineableRock_FPS_Stubbite", "weight": 1}]}
        ]}
    ])
}

/// (resource as UEX names it, terminal, sell aUEC/SCU, buy aUEC/SCU, days old).
const PRICES: &[(&str, u64, f64, f64, f64)] = &[
    ("Synthium", 101, 2950.0, 0.0, 1.0),
    ("Synthium", 103, 3120.0, 0.0, 2.0),
    ("Synthium", 104, 0.0, 2700.0, 3.0),
    ("Synthium (Ore)", 103, 1480.0, 0.0, 1.0),
    ("Fixtanium", 104, 21000.0, 0.0, 1.0),
    ("Fixtanium", 105, 22400.0, 0.0, 40.0),
    ("Fixtanium (Raw)", 105, 10900.0, 0.0, 1.0),
    ("Hydrogen", 101, 105.0, 98.0, 2.0),
    ("Hydrogen", 103, 0.0, 92.0, 2.0),
    ("Hydrogen", 107, 112.0, 0.0, 5.0),
    ("Medical Supplies", 104, 1850.0, 1610.0, 1.0),
    ("Medical Supplies", 106, 2010.0, 0.0, 9.0),
    ("Scrap", 102, 140.0, 0.0, 3.0),
];

/// `date_modified` of a fresh row: the day before the mock's fetch (Unix seconds).
const FRESH: f64 = 1_795_176_000.0;

fn prices_json() -> Value {
    let rows: Vec<Value> = PRICES
        .iter()
        .map(|&(name, terminal, sell, buy, age)| {
            let mut r = json!({"commodity_name": name, "id_terminal": terminal,
                               "date_modified": FRESH - age * 86_400.0});
            if sell > 0.0 {
                r["price_sell"] = json!(sell);
                r["scu_sell_stock"] = json!((sell / 10.0).round());
            }
            if buy > 0.0 {
                r["price_buy"] = json!(buy);
            }
            r
        })
        .collect();
    json!({"_fetched": MOCK_UEX_FETCHED, "prices": rows})
}

struct MockData {
    data: CommodityData,
    spawns: MiningSpawns,
    areas: Vec<LootArea>,
    kiosks: Kiosks,
}

fn mock() -> &'static MockData {
    static M: OnceLock<MockData> = OnceLock::new();
    M.get_or_init(|| {
        let data: CommodityData = serde_json::from_value(data_json()).expect("mock commodities");
        let kiosks = kiosks_from_files(
            &prices_json(),
            &json!({"terminals": terminal_rows()}),
            &quality_resources(&data),
            Some(MOCK_UEX_FOLDER.into()),
        );
        MockData {
            spawns: serde_json::from_value(spawns_json()).expect("mock spawns"),
            areas: serde_json::from_value(areas_json()).expect("mock areas"),
            kiosks,
            data,
        }
    })
}

fn market(prices: bool) -> MarketMeta {
    let k = &mock().kiosks.meta;
    MarketMeta {
        loaded: prices && k.loaded,
        fetched: k.fetched.clone().filter(|_| prices),
        folder: Some(MOCK_UEX_FOLDER.into()),
    }
}

/// The list: `read` (the channel's game data read), `prices` (the UEX files there).
pub fn mock_commodity_list(read: Option<bool>, prices: bool) -> CommodityDirectory {
    let state = match read {
        None => CommodityDataState::NoChannel,
        Some(false) => CommodityDataState::NotRead,
        Some(true) => CommodityDataState::Ready,
    };
    let ready = state == CommodityDataState::Ready;
    CommodityDirectory {
        state,
        build: ready.then(|| MOCK_COMMODITY_BUILD.into()),
        commodities: if ready {
            commodity_list(&mock().data)
        } else {
            Vec::new()
        },
        market: market(prices),
    }
}

/// One resource, as [`mock_commodity_list`].
pub fn mock_commodity_detail(read: Option<bool>, prices: bool, id: &str) -> CommodityInfo {
    let state = match read {
        None => CommodityDataState::NoChannel,
        Some(false) => CommodityDataState::NotRead,
        Some(true) => CommodityDataState::Ready,
    };
    if state != CommodityDataState::Ready {
        return CommodityInfo {
            state,
            commodity: None,
        };
    }
    let m = mock();
    let commodity = commodity_detail(
        &m.data,
        id,
        |q| Some(where_mined(&m.spawns, &m.areas, q)),
        |r| {
            if prices {
                m.kiosks.market(&r.name)
            } else {
                Kiosks::missing(Some(MOCK_UEX_FOLDER.into())).market(&r.name)
            }
        },
    );
    CommodityInfo { state, commodity }
}

/// Every resource's detail (the TS mock serves `commodity_detail` from this list).
pub fn mock_all_details(prices: bool) -> Vec<CommodityDetail> {
    commodity_list(&mock().data)
        .iter()
        .filter_map(|s| mock_commodity_detail(Some(true), prices, &s.id).commodity)
        .collect()
}
