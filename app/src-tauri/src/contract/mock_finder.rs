//! Synthetic blueprint catalogue and owned sets for the mock backend (addendum §7.19): two
//! channels with a read build (LIVE, PTU; HOTFIX not read), three characters, every kind,
//! four makers, sizes, grades and armor weights. All names are made up.

use super::types::OwnedBlueprint;
use starorg_core::blueprints::finder::{CatalogItem, Kind};
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
        record: format!("BP_CRAFT_SYN_{}", name.replace([' ', '\''], "_")),
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
    }
}

/// The build of a mock channel: LIVE and PTU have one (PTU one blueprint more, one less);
/// every other channel's game data is "not read".
pub fn catalog(channel: &str) -> Option<Vec<CatalogItem>> {
    let skip = match channel {
        "LIVE" => PTU_ONLY,
        "PTU" => LIVE_ONLY,
        _ => return None,
    };
    Some(
        ITEMS
            .iter()
            .enumerate()
            .filter(|(_, x)| x.0 != skip)
            .map(|(i, x)| item(i, x))
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
