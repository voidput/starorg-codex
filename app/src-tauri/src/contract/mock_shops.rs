//! The mock's Shops screen data: synthetic UEX files (seven terminals in three systems) whose
//! rows sell the Developer mode catalogue's items ([`super::mock_dev::CATALOGUE`]), so a row
//! links to a record the mock Explorer can open. The real index code
//! ([`starorg_mcp::shops::build_index`]) groups and categorises them. No UEX or CIG data.

use super::mock_dev::{CATALOGUE, catalogue_guid, catalogue_name};
use super::types::*;
use serde_json::{Value, json};
use starorg_mcp::shops::{
    ItemTags, ShopCategory, ShopIndex, by_game_type, by_uex_category, index_from_files,
};
use std::sync::OnceLock;

/// Where the mock's UEX files are (as `obtain_status` says).
pub const MOCK_UEX_FOLDER: &str = "/home/pilot/.cache/starorg-codex/mcp/uex";

/// `date_modified` of a fresh row: the day before the mock's fetch (Unix seconds).
const FRESH: f64 = 1_795_176_000.0;
const DAY: f64 = 86_400.0;

/// (id, nickname, place, city or station, moon, planet or orbit, system, on LIVE).
type Term = (
    u64,
    &'static str,
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    &'static str,
    &'static str,
    bool,
);

const TERMINALS: &[Term] = &[
    (
        101,
        "Dumper's GrimHEX",
        "Grim HEX",
        None,
        Some("Yela"),
        "Crusader",
        "Stanton",
        true,
    ),
    (
        102,
        "Skutters Grim",
        "Grim HEX",
        None,
        Some("Yela"),
        "Crusader",
        "Stanton",
        true,
    ),
    (
        103,
        "Cafe Lorville",
        "Lorville",
        Some("Lorville"),
        None,
        "Hurston",
        "Stanton",
        true,
    ),
    (
        104,
        "CenterMass Area 18",
        "Area 18",
        Some("Area 18"),
        None,
        "ArcCorp",
        "Stanton",
        true,
    ),
    (
        105,
        "Guns Ruin",
        "Ruin Station",
        None,
        None,
        "Terminus",
        "Pyro",
        true,
    ),
    (
        106,
        "Platinum Bay Ruin",
        "Ruin Station",
        None,
        None,
        "Terminus",
        "Pyro",
        true,
    ),
    (
        107,
        "Levski Admin",
        "Levski",
        Some("Levski"),
        None,
        "Delamar",
        "Nyx",
        false,
    ),
];

fn terminal_rows() -> Value {
    Value::Array(
        TERMINALS
            .iter()
            .map(|&(id, nick, place, city, moon, planet, system, live)| {
                let mut t = json!({
                    "id": id, "nickname": nick, "name": format!("{nick} - {place}"),
                    "displayname": place, "type": "item", "star_system_name": system,
                    "planet_name": planet, "is_available": 1, "is_available_live": u8::from(live),
                });
                match city {
                    Some(c) => t["city_name"] = json!(c),
                    None => t["space_station_name"] = json!(place),
                }
                if let Some(m) = moon {
                    t["moon_name"] = json!(m);
                }
                t
            })
            .collect(),
    )
}

/// The UEX category id the mock files give an item type.
fn uex_category(ty: &str) -> u64 {
    match ty {
        "WeaponGun" => 32,
        "WeaponDefensive" => 35,
        "Cooler" => 19,
        "PowerPlant" => 21,
        "Shield" => 23,
        "QuantumDrive" => 22,
        "Missile" => 34,
        "MissileLauncher" => 33,
        "WeaponPersonal" => 18,
        "Gadget" => 28,
        "Char_Armor_Helmet" => 3,
        "Char_Armor_Torso" => 5,
        "Char_Clothing_Feet" => 8,
        "WeaponAttachment" => 17,
        "Food" => 63,
        "Drink" => 62,
        "FPS_Consumable" => 16,
        "WeaponMining" => 29,
        "MiningModifier" => 30,
        _ => 61,
    }
}

/// Catalogue rows the mock shops sell: items with a type UEX lists.
fn sold() -> Vec<usize> {
    (0..CATALOGUE.len())
        .filter(|&n| {
            let ty = CATALOGUE[n].1;
            !ty.is_empty()
                && !matches!(
                    ty,
                    "NOITEM_Vehicle"
                        | "Paints"
                        | "ShopDisplay"
                        | "NOITEM_Player"
                        | "Usable"
                        | "Door"
                )
        })
        .collect()
}

/// The terminals that stock a category.
fn stockists(c: ShopCategory) -> &'static [u64] {
    use ShopCategory::*;
    match c {
        FpsWeapons | FpsAttachments => &[102, 104, 105],
        Armour => &[102, 104],
        ShipWeapons | Missiles => &[101, 105, 106],
        ShipComponents => &[101, 106],
        Mining => &[107, 101],
        Consumables => &[103, 107],
        Other => &[107],
    }
}

fn base_price(c: ShopCategory) -> f64 {
    use ShopCategory::*;
    match c {
        FpsWeapons => 4_200.0,
        FpsAttachments => 650.0,
        Armour => 2_900.0,
        ShipWeapons => 61_000.0,
        Missiles => 1_850.0,
        ShipComponents => 38_500.0,
        Mining => 22_000.0,
        Consumables => 9.0,
        Other => 140.0,
    }
}

fn price_rows() -> Value {
    let mut rows = Vec::new();
    for n in sold() {
        let r = &CATALOGUE[n];
        let cat = uex_category(r.1);
        let bucket = by_game_type(r.1).unwrap_or(by_uex_category(cat));
        // UEX names missiles without the kind word ("Syn Lance" for "Syn Lance Missile").
        let uex_name = match r.1 {
            "Missile" => r.5.rsplit_once(' ').map_or(r.5, |(a, _)| a).to_string(),
            _ => r.5.to_string(),
        };
        for (k, &t) in stockists(bucket).iter().enumerate() {
            let price =
                (base_price(bucket) * (1.0 + (n % 7) as f64 * 0.06) * (1.0 + k as f64 * 0.04))
                    .round();
            let mut row = json!({
                "item_name": uex_name, "item_uuid": catalogue_guid(n), "id_item": 9000 + n,
                "id_category": cat, "terminal_name": TERMINALS.iter().find(|x| x.0 == t).map(|x| x.1),
                "id_terminal": t, "price_buy": price,
                // Platinum Bay Ruin's rows were last confirmed weeks ago.
                "date_modified": if t == 106 { FRESH - 41.0 * DAY } else { FRESH - (n % 6) as f64 * DAY },
            });
            // Dumper's buys components and guns back.
            if t == 101 {
                row["price_sell"] = json!((price * 0.4).round());
            }
            rows.push(row);
        }
    }
    // Rows no record matches: UEX's category places them.
    rows.push(json!({"item_name": "SYN-9 Repeater", "item_uuid": "54000000-0000-4000-8000-0000000fffff", "id_category": 32,
                     "terminal_name": "Dumper's GrimHEX", "id_terminal": 101, "price_buy": 48_500, "date_modified": FRESH}));
    rows.push(json!({"item_name": "Synthetic Hull Patch", "id_category": 61, "terminal_name": "Levski Admin",
                     "id_terminal": 107, "price_buy": 75, "date_modified": FRESH - 3.0 * DAY}));
    Value::Array(rows)
}

/// The catalogue as item-tags rows (what the real backend reads from `game-item-tags`).
fn items() -> Vec<ItemTags> {
    sold()
        .into_iter()
        .map(|n| {
            let r = &CATALOGUE[n];
            ItemTags {
                name: r.5.into(),
                class_name: catalogue_name(n),
                id: catalogue_guid(n),
                ty: Some(r.1.into()),
                sub_type: Some(r.2.into()).filter(|s: &String| s != "UNDEFINED"),
                ..Default::default()
            }
        })
        .collect()
}

/// The mock's shop index, built once.
pub fn mock_shop_index() -> &'static ShopIndex {
    static S: OnceLock<ShopIndex> = OnceLock::new();
    S.get_or_init(|| {
        index_from_files(
            &json!({"_fetched": super::mock::MOCK_UEX_FETCHED, "prices": price_rows()}),
            &json!({"terminals": terminal_rows()}),
            &items(),
            Some("LIVE 4.10.200.58746".into()),
        )
    })
}

/// `shop_inventory` with the files there (`ready`) or not (`missing`).
pub fn mock_directory(ready: bool) -> ShopDirectory {
    if ready {
        mock_shop_index().directory(Some(MOCK_UEX_FOLDER.into()))
    } else {
        starorg_mcp::shops::missing_directory(Some(MOCK_UEX_FOLDER.into()))
    }
}

pub fn mock_items(ready: bool, terminal: u64) -> ShopItems {
    if ready {
        mock_shop_index().items(terminal, Some(MOCK_UEX_FOLDER.into()))
    } else {
        starorg_mcp::shops::missing_items(Some(MOCK_UEX_FOLDER.into()))
    }
}

/// Every mock shop's items, in directory order (the TS mock serves `shop_items` from these).
pub fn mock_all_items() -> Vec<ShopItems> {
    mock_shop_index()
        .shops
        .iter()
        .map(|s| mock_items(true, s.summary.id))
        .collect()
}
