//! Synthetic blueprint catalogue and owned sets for the mock backend (addendum §7.19): two
//! channels with a read build (LIVE, PTU; HOTFIX not read), three characters, every kind,
//! four makers, sizes, grades and armor weights, and a few missions that award blueprints
//! ("Earn it": guaranteed, by chance, some variants only, untitled). All names are made up.

use super::types::OwnedBlueprint;
use starorg_core::blueprints::finder::{self, CatalogItem, Earn, Kind, Mission};
use starorg_core::ini::StockIni;
use starorg_core::model::{
    BlueprintPool, BlueprintPoolEntry, BlueprintReward, Contract, GameData, ReputationReward,
    ReputationStanding,
};
use std::collections::HashMap;

/// name, kind, type, class, maker, size, grade.
type Item = (
    &'static str,
    Kind,
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
    Option<u8>,
    Option<&'static str>,
);

const ITEMS: &[Item] = &[
    (
        "Synthetic Cooler S1",
        Kind::ShipComponent,
        Some("Cooler"),
        None,
        Some("SYNA"),
        Some(1),
        Some("A"),
    ),
    (
        "Synthetic Cooler S2",
        Kind::ShipComponent,
        Some("Cooler"),
        None,
        Some("SYNA"),
        Some(2),
        Some("B"),
    ),
    (
        "Synthetic Cooler S3",
        Kind::ShipComponent,
        Some("Cooler"),
        None,
        Some("MOCK"),
        Some(3),
        Some("C"),
    ),
    (
        "Sample Power Plant S1",
        Kind::ShipComponent,
        Some("Power plant"),
        None,
        Some("MOCK"),
        Some(1),
        Some("D"),
    ),
    (
        "Sample Power Plant S2",
        Kind::ShipComponent,
        Some("Power plant"),
        None,
        Some("SYNA"),
        Some(2),
        Some("A"),
    ),
    (
        "Sample Shield S1",
        Kind::ShipComponent,
        Some("Shield"),
        None,
        Some("SYNA"),
        Some(1),
        Some("C"),
    ),
    (
        "Sample Shield S2",
        Kind::ShipComponent,
        Some("Shield"),
        None,
        Some("MOCK"),
        Some(2),
        Some("B"),
    ),
    (
        "Test Quantum Drive S2",
        Kind::ShipComponent,
        Some("Quantum drive"),
        None,
        Some("MOCK"),
        Some(2),
        Some("A"),
    ),
    (
        "Test Radar S1",
        Kind::ShipComponent,
        Some("Radar"),
        None,
        Some("SYNA"),
        Some(1),
        Some("B"),
    ),
    (
        "Sample Mining Laser S1",
        Kind::ShipComponent,
        Some("Mining laser"),
        None,
        Some("MOCK"),
        Some(1),
        Some("C"),
    ),
    (
        "PTU Prototype Shield",
        Kind::ShipComponent,
        Some("Shield"),
        None,
        Some("SYNA"),
        Some(3),
        Some("A"),
    ),
    (
        "Synthetic Cannon S3",
        Kind::ShipWeapon,
        Some("Laser cannon"),
        None,
        Some("SYNB"),
        Some(3),
        None,
    ),
    (
        "Synthetic Repeater S2",
        Kind::ShipWeapon,
        Some("Laser repeater"),
        None,
        Some("SYNB"),
        Some(2),
        None,
    ),
    (
        "Sample Gatling S4",
        Kind::ShipWeapon,
        Some("Ballistic gatling"),
        None,
        Some("MOCK"),
        Some(4),
        None,
    ),
    (
        "Test Scattergun S1",
        Kind::ShipWeapon,
        Some("Laser scattergun"),
        None,
        Some("SYNB"),
        Some(1),
        None,
    ),
    (
        "Sample Mass Driver S5",
        Kind::ShipWeapon,
        Some("Ballistic mass driver"),
        None,
        Some("MOCK"),
        Some(5),
        None,
    ),
    (
        "Synthetic Rifle Mk1",
        Kind::FpsWeapon,
        Some("Rifle"),
        None,
        Some("SYNB"),
        Some(3),
        None,
    ),
    (
        "Synthetic Rifle Mk2",
        Kind::FpsWeapon,
        Some("Rifle"),
        None,
        Some("SYNB"),
        Some(3),
        None,
    ),
    (
        "Test Pistol Variant",
        Kind::FpsWeapon,
        Some("Pistol"),
        None,
        Some("SYNB"),
        Some(1),
        None,
    ),
    (
        "Sample SMG",
        Kind::FpsWeapon,
        Some("SMG"),
        None,
        Some("MOCK"),
        Some(2),
        None,
    ),
    (
        "Sample Shotgun",
        Kind::FpsWeapon,
        Some("Shotgun"),
        None,
        Some("MOCK"),
        Some(3),
        None,
    ),
    (
        "Test Sniper",
        Kind::FpsWeapon,
        Some("Sniper rifle"),
        None,
        Some("SYNB"),
        Some(4),
        None,
    ),
    (
        "Sample Medical Gun",
        Kind::FpsWeapon,
        None,
        None,
        Some("MOCK"),
        Some(1),
        None,
    ),
    (
        "Test Multitool",
        Kind::FpsWeapon,
        None,
        None,
        Some("MOCK"),
        Some(1),
        None,
    ),
    (
        "Test Knife",
        Kind::FpsWeapon,
        None,
        None,
        Some("TSTO"),
        Some(1),
        None,
    ),
    (
        "Synthetic Armor Core",
        Kind::Armor,
        Some("Torso"),
        Some("Medium"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Armor Arms",
        Kind::Armor,
        Some("Arms"),
        Some("Medium"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Armor Legs",
        Kind::Armor,
        Some("Legs"),
        Some("Medium"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Helmet Grey",
        Kind::Armor,
        Some("Helmet"),
        Some("Light"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Helmet Red",
        Kind::Armor,
        Some("Helmet"),
        Some("Heavy"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Test Heavy Core",
        Kind::Armor,
        Some("Torso"),
        Some("Heavy"),
        Some("MOCK"),
        None,
        None,
    ),
    (
        "Test Heavy Arms",
        Kind::Armor,
        Some("Arms"),
        Some("Heavy"),
        Some("MOCK"),
        None,
        None,
    ),
    (
        "Test Light Legs",
        Kind::Armor,
        Some("Legs"),
        Some("Light"),
        None,
        None,
        None,
    ),
    (
        "Sample Undersuit",
        Kind::Armor,
        Some("Undersuit"),
        None,
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Backpack",
        Kind::Armor,
        Some("Backpack"),
        Some("Heavy"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Test Rifle Magazine",
        Kind::FpsAmmo,
        Some("Ballistic ammo"),
        None,
        Some("SYNB"),
        None,
        None,
    ),
    (
        "Sample Battery",
        Kind::FpsAmmo,
        Some("Laser ammo"),
        None,
        Some("SYNB"),
        None,
        None,
    ),
    (
        "Sample Crate",
        Kind::MissionItem,
        None,
        None,
        None,
        None,
        None,
    ),
    // One name, two blueprints: a log name matches neither for sure.
    (
        "Sample Visor",
        Kind::Armor,
        Some("Helmet"),
        Some("Light"),
        Some("TSTO"),
        None,
        None,
    ),
    (
        "Sample Visor",
        Kind::Armor,
        Some("Helmet"),
        Some("Heavy"),
        Some("TSTO"),
        None,
        None,
    ),
];

const MAKERS: &[(&str, &str)] = &[
    ("SYNA", "Synthetic Aerospace"),
    ("SYNB", "Sample Arms Co"),
    ("TSTO", "Test Outfitters"),
    ("MOCK", "Mock Dynamics"),
];

/// Only in LIVE's build / only in PTU's.
const LIVE_ONLY: &str = "Sample Crate";
const PTU_ONLY: &str = "PTU Prototype Shield";

fn item(i: usize, x: &Item) -> CatalogItem {
    let (name, kind, ty, class, maker, size, grade) = *x;
    CatalogItem {
        id: format!("00000000-0000-4000-8000-{:012x}", 0xb0 + i),
        record: format!(
            "BP_CRAFT_SYN_{}{}",
            name.replace([' ', '\''], "_"),
            if ITEMS[..i].iter().any(|x| x.0 == name) {
                "_B"
            } else {
                ""
            }
        ),
        name: name.into(),
        kind,
        type_label: ty.map(str::to_string),
        class: class.map(str::to_string),
        manufacturer: maker.map(str::to_string),
        manufacturer_name: maker
            .and_then(|m| MAKERS.iter().find(|(c, _)| *c == m))
            .map(|(_, n)| n.to_string()),
        size,
        grade: grade.map(str::to_string),
        earn: None,
    }
}

/// Record short name of a mock blueprint (as [`item`] names it).
fn record(name: &str) -> String {
    format!("BP_CRAFT_SYN_{}", name.replace([' ', '\''], "_"))
}

/// Synthetic contracts and pools of a read build, through the core's `earn_index` like a
/// real build's. PTU has one mission more (a trial that awards a blueprint LIVE's missions
/// do not), so "All channels" shows missions of several builds.
fn earn(channel: &str) -> HashMap<String, Vec<Earn<Mission>>> {
    let pool = |name: &str, list: &[(&str, Option<f64>)]| BlueprintPool {
        name: name.into(),
        blueprints: list
            .iter()
            .map(|(b, w)| BlueprintPoolEntry {
                blueprint: record(b),
                weight: *w,
            })
            .collect(),
    };
    let contract = |debug: &str, title: Option<&str>, pool: Option<(&str, Option<f64>)>| Contract {
        generator: "SYN_MockGenerator".into(),
        debug_name: debug.into(),
        title_key: title.map(|t| format!("@{t}")),
        blueprint_rewards: pool
            .map(|(p, chance)| BlueprintReward {
                pool: p.into(),
                chance,
            })
            .into_iter()
            .collect(),
        ..Default::default()
    };
    let rep = |faction: &str, name: Option<&str>| ReputationReward {
        faction: Some(faction.into()),
        faction_name_key: name.map(str::to_string),
        amount: 250.0,
        on_success: true,
        on_failure: false,
    };
    let mut easy = contract(
        "SYN_Bounty_Easy",
        Some("syn_mission_bounty"),
        Some(("BP_MOCK_BOUNTY", Some(1.0))),
    );
    easy.reputation.push(rep(
        "factionreputation_lawful_synthetic_security",
        Some("@syn_faction_security"),
    ));
    easy.systems = vec!["Stanton".into()];
    let mut hard = contract(
        "SYN_Bounty_Hard",
        Some("syn_mission_bounty"),
        Some(("BP_MOCK_BOUNTY", Some(1.0))),
    );
    hard.reputation.push(rep(
        "factionreputation_lawful_synthetic_security",
        Some("@syn_faction_security"),
    ));
    hard.systems = vec!["Pyro".into()];
    let mut salvage = contract(
        "SYN_Salvage",
        Some("syn_mission_salvage"),
        Some(("BP_MOCK_SALVAGE", Some(0.3))),
    );
    salvage
        .reputation
        .push(rep("factionreputation_unlawful_test_cds", None));
    salvage.chain = true;
    salvage.once_only = true;
    salvage.min_standing = Some("syn_standing_trusted".into());
    let mut gd = GameData {
        contracts: vec![
            easy,
            hard,
            salvage,
            contract(
                "SYN_Courier_Long",
                Some("syn_mission_courier"),
                Some(("BP_MOCK_COURIER", Some(1.0))),
            ),
            contract("SYN_Courier_Short", Some("syn_mission_courier"), None),
            contract("SYN_Debug_Reward", None, Some(("BP_MOCK_DEBUG", Some(1.0)))),
        ],
        blueprint_pools: vec![
            pool(
                "BP_MOCK_BOUNTY",
                &[
                    ("Synthetic Rifle Mk2", Some(1.0)),
                    ("Sample Shield S1", Some(1.0)),
                    ("Synthetic Cooler S3", Some(2.0)),
                    ("Sample Helmet Red", Some(1.0)),
                    ("PTU Prototype Shield", Some(1.0)),
                ],
            ),
            pool(
                "BP_MOCK_SALVAGE",
                &[
                    ("Synthetic Cannon S3", None),
                    ("Sample Gatling S4", None),
                    ("Test Sniper", None),
                    ("Synthetic Rifle Mk2", None),
                ],
            ),
            pool(
                "BP_MOCK_COURIER",
                &[
                    ("Sample Armor Legs", Some(1.0)),
                    ("Sample Armor Arms", Some(1.0)),
                ],
            ),
            pool("BP_MOCK_DEBUG", &[("Test Knife", None)]),
        ],
        reputation_standings: vec![ReputationStanding {
            name: "syn_standing_trusted".into(),
            record_name: "SReputationStandingParams.syn_standing_trusted".into(),
            display_name_key: Some("@syn_standing_trusted".into()),
            min_reputation: Some(1000.0),
            ..Default::default()
        }],
        ..Default::default()
    };
    if channel == "PTU" {
        gd.contracts.push(contract(
            "SYN_PTU_Trial",
            Some("syn_mission_trial"),
            Some(("BP_MOCK_TRIAL", Some(1.0))),
        ));
        gd.blueprint_pools
            .push(pool("BP_MOCK_TRIAL", &[("Synthetic Rifle Mk1", Some(1.0))]));
    }
    let stock = StockIni::parse(
        b"syn_mission_bounty=Eliminate the Sample Warlord\n\
syn_mission_salvage=Recover the Cargo near ~mission(Location|Address)\n\
syn_mission_courier=Synthetic Courier Run\n\
syn_mission_trial=Prototype Trial Run\n\
syn_standing_trusted=Trusted\n\
syn_faction_security=Synthetic Security Bureau\n"
            .to_vec(),
    );
    finder::earn_index(&gd, &stock).unwrap_or_default()
}

/// The build of a mock channel: LIVE and PTU have one (PTU one blueprint more, one less);
/// every other channel's game data is "not read".
pub fn catalog(channel: &str) -> Option<Vec<CatalogItem>> {
    let skip = match channel {
        "LIVE" => PTU_ONLY,
        "PTU" => LIVE_ONLY,
        _ => return None,
    };
    let earn = earn(channel);
    Some(
        ITEMS
            .iter()
            .enumerate()
            .filter(|(_, x)| x.0 != skip)
            .map(|(i, x)| {
                let mut c = item(i, x);
                c.earn = Some(
                    earn.get(&c.record.to_ascii_lowercase())
                        .cloned()
                        .unwrap_or_default(),
                );
                c
            })
            .collect(),
    )
}

fn owned(list: &[(&str, &str, &str)]) -> Vec<OwnedBlueprint> {
    list.iter()
        .map(|(n, src, t)| OwnedBlueprint {
            name: (*n).into(),
            ambiguous: false,
            favorite: false,
            sources: vec![(*src).into()],
            acquired: Some((*t).into()),
        })
        .collect()
}

/// `unattributed.json` of a mock channel: seen before any login line in its log file.
pub fn unattributed(channel: &str) -> Vec<OwnedBlueprint> {
    match channel {
        "LIVE" => owned(&[("Test Scattergun S1", "log", "2026-10-20T18:00:00Z")]),
        _ => Vec::new(),
    }
}

/// Owned sets per character of one channel (geids of `mock::CHARACTERS`). PTU progress is
/// not LIVE progress: each channel has its own.
pub fn seed(channel: &str, chars: &[&str; 3]) -> HashMap<String, Vec<OwnedBlueprint>> {
    let [a, b, c] = *chars;
    let sets: [Vec<OwnedBlueprint>; 3] = match channel {
        "PTU" => [
            owned(&[
                ("Synthetic Rifle Mk1", "log", "2026-11-12T18:00:00Z"),
                ("PTU Prototype Shield", "log", "2026-11-13T20:30:00Z"),
            ]),
            Vec::new(),
            owned(&[
                ("Sample Helmet Grey", "log", "2026-11-09T21:05:00Z"),
                ("Synthetic Repeater S2", "log", "2026-11-11T19:45:00Z"),
            ]),
        ],
        "HOTFIX" => [
            owned(&[("Synthetic Rifle Mk1", "log", "2026-11-13T09:00:00Z")]),
            Vec::new(),
            Vec::new(),
        ],
        _ => [
            owned(&[
                ("Synthetic Rifle Mk1", "log", "2026-11-10T20:11:00Z"),
                ("Synthetic Armor Core", "log", "2026-11-08T17:20:00Z"),
                ("Test Pistol Variant", "log", "2026-11-06T22:02:00Z"),
                ("Sample Helmet Grey", "log", "2026-11-07T19:40:00Z"),
                (
                    "Synthetic Cooler S2",
                    "import:smart-citizen",
                    "2026-10-28T12:00:00Z",
                ),
                (
                    "Synthetic Cannon S3",
                    "starorg:Test Org",
                    "2026-11-02T08:15:00Z",
                ),
            ]),
            owned(&[
                ("Synthetic Rifle Mk1", "log", "2026-11-11T22:30:00Z"),
                ("Sample Undersuit", "log", "2026-11-09T18:00:00Z"),
                ("Sample Visor", "log", "2026-11-04T21:10:00Z"),
                (
                    "Sample Shield S1",
                    "import:smart-citizen",
                    "2026-10-30T10:00:00Z",
                ),
            ]),
            Vec::new(),
        ],
    };
    [a, b, c]
        .into_iter()
        .map(str::to_string)
        .zip(sets)
        .collect()
}
