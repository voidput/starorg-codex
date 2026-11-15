//! The hulls of one build (addendum §6.2).
//!
//! A **hull** is a stock key that is referenced as `loc.name` by at least one ship record,
//! does not end in `short` (folded), whose stock value is real text, and that either starts
//! with `vehicle_Name` (folded) or is named by a player-hull record whatever its prefix, as
//! long as it contains `VehicleName` (the Wikelo hulls name themselves
//! `TheCollector_ShipMod_*_VehicleName`). Its **id** is the key as stock spells it without a flag suffix
//! (`vehicle_NameAEGS_Avenger_Titan`): stable across builds and channels, and exactly the
//! key the generator writes. Ids are compared folded (spec 4.3).

use crate::enhance::ships::is_player_hull;
use crate::ini::{StockIni, fold_key, strip_flag_suffix};
use crate::model::namesake::named_after;
use crate::model::{GameData, is_placeholder_name, loc_key};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hull {
    /// Hull id (= the long name key, stock spelling, no flag suffix).
    pub hull: String,
    /// Stock long name (`Aegis Gladius`), untrimmed.
    pub name: String,
    /// Stock value of `<hull>_short`, when that key exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    /// The short key as stock spells it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_key: Option<String>,
    /// Game maker code (`aegs`), most common among its records, year suffix dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker: Option<String>,
    /// At least one record passes the player-hull rule (spec 5.2).
    pub flyable: bool,
    /// Ground vehicle (wheeled movement class).
    pub ground: bool,
    /// Records of other variants (AI, PU, unmanned) that show the same name.
    pub npc_shared: u32,
    /// A player-flyable record is named after this key (tie-break for same display names).
    pub namesake: bool,
}

/// Every hull of a build, sorted by stock name the way the preview sorts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub hulls: Vec<Hull>,
    index: BTreeMap<String, usize>,
}

/// Sort key of the in-game preview (§6.6, unconfirmed): case-insensitive, character by
/// character.
pub fn sort_key(s: &str) -> String {
    s.to_lowercase()
}

/// Finds the short key of a hull. Stock spells most as `<hull>_short`; some drop the `_`
/// (`..._VehicleNameShort`), some spell the hull part differently (`KRIG_L-22` for the long
/// key's `KRIG_L22`, `AGES` for `AEGS`). Tried in that order; the loose forms only count when
/// exactly one key fits.
struct ShortKeys {
    /// Squashed key without its `short` ending -> stock lines.
    squashed: BTreeMap<String, Vec<usize>>,
    /// Same, with the maker segment's letters sorted (catches a transposed maker code).
    anagram: BTreeMap<String, Vec<usize>>,
}

/// `vehicle_NameAGES_Firebird` -> `vehicle_name` + `aegs` (letters sorted) + rest, squashed.
fn maker_anagram(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let rest = lower.strip_prefix("vehicle_name")?;
    let (maker, tail) = rest.split_once('_')?;
    let mut m: Vec<char> = maker.chars().collect();
    m.sort_unstable();
    Some(format!(
        "{}|{}",
        m.into_iter().collect::<String>(),
        super::squash(tail)
    ))
}

impl ShortKeys {
    fn new(stock: &StockIni) -> Self {
        let mut squashed: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut anagram: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (line, key, _) in stock.entries() {
            let base = strip_flag_suffix(key);
            let lower = base.to_ascii_lowercase();
            let Some(head) = lower.strip_suffix("short") else {
                continue;
            };
            let head = head.strip_suffix('_').unwrap_or(head);
            squashed.entry(super::squash(head)).or_default().push(line);
            if let Some(a) = maker_anagram(head) {
                anagram.entry(a).or_default().push(line);
            }
        }
        Self { squashed, anagram }
    }

    fn find(&self, stock: &StockIni, hull: &str) -> Option<usize> {
        for exact in [format!("{hull}_short"), format!("{hull}short")] {
            if let Ok(l) = stock.find(&exact) {
                return Some(l);
            }
        }
        let one = |v: Option<&Vec<usize>>| match v.map(Vec::as_slice) {
            Some([l]) => Some(*l),
            _ => None,
        };
        one(self.squashed.get(&super::squash(hull)))
            .or_else(|| one(self.anagram.get(&maker_anagram(hull)?)))
    }
}

impl Catalog {
    pub fn new(mut hulls: Vec<Hull>) -> Self {
        hulls.sort_by(|a, b| {
            sort_key(&a.name)
                .cmp(&sort_key(&b.name))
                .then_with(|| a.hull.cmp(&b.hull))
        });
        let index = hulls
            .iter()
            .enumerate()
            .map(|(i, h)| (fold_key(&h.hull), i))
            .collect();
        Self { hulls, index }
    }

    /// The hulls of a build: `GameData.ships` + stock (no schema change).
    pub fn build(gd: &GameData, stock: &StockIni) -> Self {
        struct Acc {
            hull: String,
            name: String,
            makers: BTreeMap<String, u32>,
            flyable: bool,
            ground: bool,
            npc: u32,
            namesake: bool,
        }
        let is_player = |s: &crate::model::Ship| !s.is_template && is_player_hull(s.short_name());
        // Keys a player-hull record names itself by, whatever their prefix, as long as they
        // still say `VehicleName` (`vehicles_NameOrbital_Sentry`, a turret, does not).
        let player_keys: BTreeSet<String> = gd
            .ships
            .iter()
            .filter(|s| is_player(s))
            .filter_map(|s| s.loc.name.as_deref().and_then(loc_key))
            .map(fold_key)
            .filter(|k| k.replace('_', "").contains("vehiclename"))
            .collect();
        let mut by: BTreeMap<String, Acc> = BTreeMap::new();
        for s in &gd.ships {
            let Some(key) = s.loc.name.as_deref().and_then(loc_key) else {
                continue;
            };
            let folded = fold_key(key);
            if folded.ends_with("short")
                || !(folded.starts_with("vehicle_name") || player_keys.contains(&folded))
            {
                continue;
            }
            let Ok(line) = stock.find(key) else { continue };
            let value = stock.value_at(line).unwrap_or_default();
            if value.trim().is_empty() || is_placeholder_name(Some(value.trim())) {
                continue;
            }
            let stock_key = stock.key_at(line).unwrap_or_default();
            let a = by.entry(folded).or_insert_with(|| Acc {
                hull: strip_flag_suffix(stock_key).to_string(),
                name: value.to_string(),
                makers: BTreeMap::new(),
                flyable: false,
                ground: false,
                npc: 0,
                namesake: false,
            });
            let player = is_player(s);
            if player {
                a.flyable = true;
                if named_after(&a.hull, s.short_name()) {
                    a.namesake = true;
                }
            } else {
                a.npc += 1;
            }
            if s.movement_class
                .as_deref()
                .is_some_and(|m| m.to_ascii_lowercase().contains("wheel"))
            {
                a.ground = true;
            }
            if let Some(m) = &s.manufacturer {
                *a.makers
                    .entry(super::strip_year(m).to_ascii_lowercase())
                    .or_default() += 1;
            }
        }
        let shorts = ShortKeys::new(stock);
        let hulls = by
            .into_values()
            .map(|a| {
                let short_line = shorts.find(stock, &a.hull);
                let maker = a
                    .makers
                    .iter()
                    .max_by(|x, y| x.1.cmp(y.1).then_with(|| y.0.cmp(x.0)))
                    .map(|(m, _)| m.clone());
                Hull {
                    short: short_line.and_then(|l| stock.value_at(l).map(str::to_string)),
                    short_key: short_line
                        .and_then(|l| stock.key_at(l).map(|k| strip_flag_suffix(k).to_string())),
                    hull: a.hull,
                    name: a.name,
                    maker,
                    flyable: a.flyable,
                    ground: a.ground,
                    npc_shared: a.npc,
                    namesake: a.namesake,
                }
            })
            .collect();
        Self::new(hulls)
    }

    pub fn get(&self, id: &str) -> Option<&Hull> {
        self.index.get(&fold_key(id)).map(|&i| &self.hulls[i])
    }

    /// Player-flyable hulls only: what the UI offers and the importer matches.
    pub fn flyable(&self) -> impl Iterator<Item = &Hull> {
        self.hulls.iter().filter(|h| h.flyable)
    }

    pub fn is_empty(&self) -> bool {
        self.hulls.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Ship, ShipLoc};

    fn ship(record: &str, name: &str, maker: &str) -> Ship {
        Ship {
            record_name: format!("EntityClassDefinition.{record}"),
            loc: ShipLoc {
                name: Some(format!("@{name}")),
                ..Default::default()
            },
            manufacturer: Some(maker.into()),
            movement_class: Some("Spaceship".into()),
            ..Default::default()
        }
    }

    #[test]
    fn hulls_from_records_and_stock() {
        let stock = StockIni::parse(
            b"\xEF\xBB\xBFvehicle_NameAEGS_Gladius=Aegis Gladius\nvehicle_NameAEGS_Gladius_Short=Gladius\nvehicle_NameRSI_Ursa,P=RSI Ursa\nvehicle_NameORIG_890Jump=Origin 890 Jump\nvehicle_NameORIG_890Jump_Hijacked=Origin 890 Jump\nvehicle_NameTEST=<= PLACEHOLDER =>\nvehicle_NameConcept=Concept\n"
                .to_vec(),
        );
        let mut ursa = ship("RSI_Ursa", "vehicle_NameRSI_Ursa", "rsi_2025");
        ursa.movement_class = Some("ArcadeWheeled".into());
        let gd = GameData {
            ships: vec![
                ship("AEGS_Gladius", "vehicle_NameAEGS_Gladius", "aegs"),
                ship("AEGS_Gladius_PU_AI_UEE", "vehicle_NameAEGS_Gladius", "aegs"),
                ship(
                    "AEGS_Gladius_AI_Only",
                    "vehicle_NameAEGS_Gladius_Short",
                    "aegs",
                ),
                ursa,
                ship("ORIG_890Jump", "vehicle_NameORIG_890Jump", "orig"),
                ship(
                    "ORIG_890Jump_Drug_01",
                    "vehicle_NameORIG_890Jump_Hijacked",
                    "orig",
                ),
                ship("TEST_Ship", "vehicle_NameTEST", "aegs"),
            ],
            ..Default::default()
        };
        let c = Catalog::build(&gd, &stock);
        let ids: Vec<&str> = c.hulls.iter().map(|h| h.hull.as_str()).collect();
        assert_eq!(
            ids,
            [
                "vehicle_NameAEGS_Gladius",
                "vehicle_NameORIG_890Jump",
                "vehicle_NameORIG_890Jump_Hijacked",
                "vehicle_NameRSI_Ursa"
            ]
        );
        let g = c.get("VEHICLE_NAMEaegs_gladius").unwrap();
        assert_eq!(g.short.as_deref(), Some("Gladius"));
        assert_eq!(
            g.short_key.as_deref(),
            Some("vehicle_NameAEGS_Gladius_Short")
        );
        assert_eq!(g.npc_shared, 1);
        assert!(g.flyable && g.namesake && !g.ground);
        let u = c.get("vehicle_NameRSI_Ursa").unwrap();
        assert_eq!(u.maker.as_deref(), Some("rsi"));
        assert!(u.ground && u.short.is_none());
        assert!(c.get("vehicle_NameORIG_890Jump").unwrap().namesake);
        assert!(!c.get("vehicle_NameORIG_890Jump_Hijacked").unwrap().namesake);
        assert!(c.get("vehicle_NameConcept").is_none());
    }

    #[test]
    fn player_hulls_named_by_other_keys_and_odd_short_keys() {
        let stock = StockIni::parse(
            [
                "TheCollector_ShipMod_SYN_Alpha_VehicleName=Synth Alpha Special",
                "TheCollector_ShipMod_SYN_Alpha_VehicleNameShort=Alpha SP",
                "TheCollector_ShipMod_SYN_Npc_VehicleName=Synth Npc Special",
                "vehicle_NameSYNA_Bravo_Collector=Synth Bravo Collector",
                "vehicle_NameSNYA_Bravo_Collector_short=Bravo CL",
                "vehicle_NameSYNA_C22_Wolf=Synth C-22 Wolf",
                "vehicle_NameSYNA_C-22_Wolf_short=Wolf",
                "vehicle_NameSYNA_Delta=Synth Delta",
                "vehicle_NameSYNA_Delta_short=Delta",
                "vehicle_NameSYNA_Del_ta_short=Other",
                "mission_Name_Synth=Some mission",
                "vehicles_NameSynth_Sentry=Synth Sentry",
            ]
            .join("\n")
            .into_bytes(),
        );
        let gd = GameData {
            ships: vec![
                ship(
                    "SYNA_Alpha_Collector_Mod",
                    "TheCollector_ShipMod_SYN_Alpha_VehicleName",
                    "syna",
                ),
                // A non-player record naming itself by an odd key stays out.
                ship(
                    "SYNA_Npc_PU_AI_CIV",
                    "TheCollector_ShipMod_SYN_Npc_VehicleName",
                    "syna",
                ),
                ship("SYNA_Mission_AI", "mission_Name_Synth", "syna"),
                ship("SYNA_Sentry", "vehicles_NameSynth_Sentry", "syna"),
                ship(
                    "SYNA_Bravo_Collector",
                    "vehicle_NameSYNA_Bravo_Collector",
                    "syna",
                ),
                ship("SYNA_C22_Wolf", "vehicle_NameSYNA_C22_Wolf", "syna"),
                ship("SYNA_Delta", "vehicle_NameSYNA_Delta", "syna"),
            ],
            ..Default::default()
        };
        let c = Catalog::build(&gd, &stock);
        let a = c
            .get("TheCollector_ShipMod_SYN_Alpha_VehicleName")
            .expect("player hull with a non-vehicle_Name key");
        assert!(a.flyable);
        assert_eq!(a.name, "Synth Alpha Special");
        assert_eq!(
            a.short_key.as_deref(),
            Some("TheCollector_ShipMod_SYN_Alpha_VehicleNameShort")
        );
        assert!(c.get("TheCollector_ShipMod_SYN_Npc_VehicleName").is_none());
        assert!(c.get("mission_Name_Synth").is_none());
        assert!(
            c.get("vehicles_NameSynth_Sentry").is_none(),
            "player record, but not a VehicleName key"
        );
        let short = |id: &str| c.get(id).unwrap().short_key.clone();
        assert_eq!(
            short("vehicle_NameSYNA_Bravo_Collector").as_deref(),
            Some("vehicle_NameSNYA_Bravo_Collector_short"),
            "transposed maker code"
        );
        assert_eq!(
            short("vehicle_NameSYNA_C22_Wolf").as_deref(),
            Some("vehicle_NameSYNA_C-22_Wolf_short"),
            "punctuation differs"
        );
        assert_eq!(
            short("vehicle_NameSYNA_Delta").as_deref(),
            Some("vehicle_NameSYNA_Delta_short"),
            "the exact spelling wins over a loose one"
        );
    }
}
