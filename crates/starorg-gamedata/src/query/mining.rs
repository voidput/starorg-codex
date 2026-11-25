//! The Mining tab (ours): every mineable resource of a build with what it refines into, each
//! place that spawns it with that place's quality curve, the rock there, how hard the ore is
//! to break and which kind of laser it takes.
//!
//! Built once per build ([`build_index`]) from datasets only:
//!
//! | Dataset | Gives |
//! |---|---|
//! | `game-commodities` | the resources, refined forms, quality curves, place overrides, shown steps |
//! | `game-mining-spawns` | where each ore spawns, in which rock, the rock's kind, tier, signature, share |
//! | `game-item-sources` | hand-placed spots (caves, events) through [`super::mined::where_mined`] |
//! | `game-mining` (optional) | `MineableElement` physics: resistance, instability, charge window |
//! | `game-weapons` (optional) | mining laser power (the first beam mode of each `WeaponMining`) |
//!
//! What the files do not hold is said, never guessed: refining yields are server-side, a
//! rock's mass is rolled when it spawns (the records carry a placeholder), and a curve
//! override names a place (`Pyro System`) that is matched to a spawn row only by its system
//! or body name. The difficulty grade and the laser recommendation are labelled estimates
//! with their inputs; see [`difficulty`] and [`recommend`].

use super::commodities::{CommodityData, CommodityRef, ResourceRow, group_label};
use super::en_us;
use super::mined::{MinedSpot, MiningSpawns, ore_core, where_mined};
use super::quality::{QualityRange, QualityStep, ore_alias, resource_core, share_at_least, steps};
use super::types::LootArea;
use crate::collate::compare_names;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// The quality a roll counts as high from (the shown steps reach 1000).
pub const HIGH_QUALITY: f64 = 700.0;

// ------------------------------------------------------------------ inputs

/// One `game-mining` `mineableElements` row.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ElementIn {
    /// The resource it is (`Raw_Quantainium`), from the record's `resourceType`.
    pub name: Option<String>,
    /// `MineableElement.Quantainium_Raw`; its short name is the spawn data's ore key.
    pub record_name: String,
    pub instability: Option<f64>,
    pub resistance: Option<f64>,
    pub optimal_window_midpoint: Option<f64>,
    pub optimal_window_randomness: Option<f64>,
    pub optimal_window_thinness: Option<f64>,
    pub explosion_multiplier: Option<f64>,
    pub cluster_factor: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct LaserModeIn {
    pub kind: Option<String>,
    pub dps: Option<f64>,
}

/// One `game-weapons` `shipWeapons` row, as far as this reads it.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WeaponIn {
    pub name: Option<String>,
    pub record_name: Option<String>,
    #[serde(rename = "type")]
    pub ty: Option<String>,
    pub size: Option<f64>,
    pub modes: Vec<LaserModeIn>,
}

/// A ship mining laser: its power is its first beam mode's figure (the mining beam; the
/// second is the extraction beam).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Laser {
    pub name: String,
    pub class_name: String,
    pub size: u8,
    pub power: f64,
}

/// The S1 and S2 mining lasers of `game-weapons` (S0 vehicle lasers carry no comparable
/// power figure; test and template records are left out).
pub fn lasers_from(weapons: &[WeaponIn]) -> Vec<Laser> {
    let mut out: Vec<Laser> = weapons
        .iter()
        .filter(|w| w.ty.as_deref() == Some("WeaponMining"))
        .filter_map(|w| {
            let class = w.record_name.as_deref()?.rsplit('.').next()?.to_string();
            let lower = class.to_ascii_lowercase();
            if lower.contains("_test") || lower.contains("template") {
                return None;
            }
            let size = w.size.filter(|s| *s == 1.0 || *s == 2.0)? as u8;
            let power = w
                .modes
                .iter()
                .find(|m| m.kind.as_deref() == Some("beam"))
                .and_then(|m| m.dps)
                .filter(|p| *p > 0.0)?;
            Some(Laser {
                name: w.name.clone().unwrap_or_else(|| class.clone()),
                class_name: class,
                size,
                power,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.size
            .cmp(&b.size)
            .then_with(|| b.power.total_cmp(&a.power))
            .then_with(|| compare_names(&a.name, &b.name))
    });
    out
}

// ------------------------------------------------------------------ output

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Grade {
    Easy,
    Medium,
    Hard,
}

/// The tool a rock takes, from its mining kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tool {
    /// FPS mineable: a multitool or hand laser.
    Hand,
    /// Ground vehicle mineable: a ROC.
    Roc,
    /// Ship mineable: a mining ship.
    Ship,
    /// The rock's kind is not in the data.
    Unknown,
}

/// The mining physics of one ore (`MineableElement`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementPhysics {
    /// The spawn data's ore key (`quantainium_raw`).
    pub ore: String,
    pub resistance: Option<f64>,
    pub instability: Option<f64>,
    /// Where the optimal charge window sits, 0..1 of the charge bar.
    pub window_midpoint: Option<f64>,
    /// How far the midpoint moves per rock (±, 0..1).
    pub window_randomness: Option<f64>,
    /// Higher is a narrower window.
    pub window_thinness: Option<f64>,
    pub explosion_multiplier: Option<f64>,
    pub cluster_factor: Option<f64>,
}

/// Easy, medium or hard (an estimate), with the inputs it is worked out from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Difficulty {
    pub grade: Grade,
    /// 0..2 per known input: resistance, instability, window thinness.
    pub points: u8,
    pub max_points: u8,
    pub resistance: Option<f64>,
    pub instability: Option<f64>,
    pub window_thinness: Option<f64>,
    /// Inputs the data does not give ("window thinness").
    pub missing: Vec<String>,
}

/// One mining ship class against a rock: its lasers' power and what the resistance leaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipOption {
    /// `Prospector`, `MOLE`.
    pub ship: String,
    pub laser_count: u8,
    pub laser_size: u8,
    /// The strongest laser of that size in the data.
    pub best_laser: Option<String>,
    /// laser power × lasers; `None` without the laser data.
    pub power: Option<f64>,
    /// power × (1 − resistance); `None` without the resistance or the laser data.
    pub effective: Option<f64>,
}

/// Which tool a rock takes and, for ship rocks, the ship classes' power against it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub tool: Tool,
    /// `Hand (multitool)`, `ROC`, `Ship: Prospector or MOLE`.
    pub label: String,
    /// One line: why, with the numbers.
    pub reason: String,
    /// The tool follows from the rock's kind; only the ship comparison is an estimate.
    pub estimate: bool,
    /// The formula and what it lacks, for ship rocks.
    pub formula: Option<String>,
    pub options: Vec<ShipOption>,
}

/// Where a place's curve comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CurveSource {
    /// No override names the place: the resource's own curve.
    Base,
    /// An override names the place's body, provider key or system.
    Override,
    /// A system override exists but the place's system is not known (Lagrange points,
    /// asteroid clusters): the base curve is shown, an override may apply.
    Unknown,
}

/// A place's quality: its curve (the override for the place, else the resource's own).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaceQuality {
    pub range: QualityRange,
    pub source: CurveSource,
    /// The override that applies here (`Pyro System`); `None`: base (or not known, see
    /// `source`).
    pub override_location: Option<String>,
    /// Every quality scale of the row is 1 (or none is given). A row with a scale below 1
    /// may roll lower than the curve says; how the game applies it is not in the data, so
    /// the shares are not changed, only marked, and such rows rank after the others.
    pub full_scale: bool,
    /// Estimated share of rolls at or above [`HIGH_QUALITY`].
    pub high: Option<f64>,
    /// The top two shown steps a roll here can reach, highest last, with shares.
    pub top: Vec<QualityStep>,
    /// Their shares together.
    pub top_share: Option<f64>,
}

/// One body or belt that spawns the mineral, in one rock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiningPlace {
    /// The ore key of this row (a mineral can have a hand and a ROC form).
    pub ore: String,
    /// Provider key (`Stanton1c`).
    pub location: String,
    /// The real star system (`Stanton2c` belt -> `Stanton`, Aaron Halo -> `Stanton`);
    /// `None` when the data does not say (Lagrange points, asteroid clusters).
    pub system: Option<String>,
    pub body: Option<String>,
    /// Readable name: the body, else one from the key (`Yela Belt`, `Lagrange A`).
    pub place: String,
    pub rock: String,
    /// "ship", "vehicle" or "hand".
    pub kind: Option<String>,
    pub tier: Option<String>,
    pub signature: Option<f64>,
    /// Spawn weight (see [`super::mined::MinedBelt::spawn_share`]): compare within a kind.
    pub spawn_share: Option<f64>,
    /// Share of the rock that is this ore, percent.
    pub min_percent: Option<f64>,
    pub max_percent: Option<f64>,
    /// The records' quality multipliers of this ore in this rock (how the game applies them
    /// is not in the data, so the curve below does not use them).
    pub quality_scales: Vec<f64>,
    /// Other ores the rock can hold.
    pub companions: Vec<String>,
    /// Always `None`: a rock's mass is rolled at spawn, the records hold a placeholder.
    pub rock_mass: Option<f64>,
    pub quality: PlaceQuality,
    pub difficulty: Option<Difficulty>,
    pub recommendation: Recommendation,
}

/// A curve override and how many spawn rows it applies to (0: it names a place no spawn
/// row is, such as a station).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverrideUse {
    pub location: String,
    pub range: QualityRange,
    pub places: usize,
}

/// The strongest and weakest mining laser of one size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaserClass {
    pub size: u8,
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub best: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MineralDetail {
    /// The resource record (`ResourceType.Raw_Quantainium`), else `ore:<key>` for an ore no
    /// resource matches.
    pub id: String,
    pub name: String,
    /// The Commodities row of the same resource; `None` for an ore without one.
    pub commodity: Option<CommodityRef>,
    pub refined: Option<CommodityRef>,
    pub ores: Vec<String>,
    /// The mining kinds of its places and spots' rocks: "hand", "vehicle", "ship", in that
    /// order.
    pub kinds: Vec<String>,
    /// The curve family (`Rare`, `FPSMineable`).
    pub tier: Option<String>,
    /// The resource's own curve; `None` without one.
    pub base: Option<QualityRange>,
    /// The shown steps on the base curve.
    pub steps: Vec<QualityStep>,
    pub base_high: Option<f64>,
    pub overrides: Vec<OverrideUse>,
    /// Empty while `game-mining` is not read.
    pub elements: Vec<ElementPhysics>,
    /// Most common first (by kind, then spawn weight).
    pub places: Vec<MiningPlace>,
    /// Hand-placed spots (caves, events); their curve is the base one.
    pub spots: Vec<MinedSpot>,
}

/// One list row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MineralSummary {
    pub id: String,
    pub name: String,
    /// The refined form's name.
    pub refined: Option<String>,
    /// "hand", "vehicle", "ship", in that order.
    pub kinds: Vec<String>,
    pub places: usize,
    pub spots: usize,
    /// The best place's estimated share of rolls at or above [`HIGH_QUALITY`]: of the places
    /// with a full quality scale and a known curve when there are any.
    pub best_high: Option<f64>,
    /// `best_high` comes from a place whose scale is below 1 or whose curve is not known:
    /// the real share may be lower.
    pub best_high_uncertain: bool,
    /// The hardest grade over its places.
    pub difficulty: Option<Grade>,
    pub has_override: bool,
}

/// Every mineral of a build, built once per build.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MiningIndex {
    pub minerals: Vec<MineralDetail>,
    pub lasers: Vec<LaserClass>,
    /// `game-mining` was read.
    pub physics: bool,
    /// `game-weapons` was read.
    pub lasers_read: bool,
    /// `game-mining-spawns` was read (else no body or belt is known yet).
    pub spawns_read: bool,
    /// `game-item-sources` was read (else no hand-placed spot is known yet).
    pub spots_read: bool,
}

impl PlaceQuality {
    /// Known curve and full scale: its share can be compared with the others'.
    pub fn trusted(&self) -> bool {
        self.full_scale && self.source != CurveSource::Unknown
    }
}

impl MiningIndex {
    pub fn summaries(&self) -> Vec<MineralSummary> {
        self.minerals.iter().map(summary).collect()
    }

    /// A mineral by id (`ResourceType.Sadaryx` or `Sadaryx`), by its commodity id, or by one
    /// of its ore keys.
    pub fn detail(&self, id: &str) -> Option<&MineralDetail> {
        let id = id.trim();
        let short = |s: &str| s.rsplit('.').next().unwrap_or(s).to_string();
        self.minerals
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(id))
            .or_else(|| {
                self.minerals.iter().find(|m| {
                    m.commodity
                        .as_ref()
                        .is_some_and(|c| c.id.eq_ignore_ascii_case(id))
                        || m.ores.iter().any(|o| o.eq_ignore_ascii_case(id))
                })
            })
            .or_else(|| {
                self.minerals
                    .iter()
                    .find(|m| m.commodity.is_some() && short(&m.id).eq_ignore_ascii_case(id))
            })
    }
}

fn summary(m: &MineralDetail) -> MineralSummary {
    let best = |trusted: bool| {
        m.places
            .iter()
            .filter(|p| !trusted || p.quality.trusted())
            .filter_map(|p| p.quality.high)
            .max_by(f64::total_cmp)
    };
    let (best_high, best_high_uncertain) = match (best(true), best(false)) {
        (Some(t), _) => (Some(t), false),
        (None, Some(u)) => (Some(u), true),
        (None, None) if m.places.is_empty() && !m.spots.is_empty() => (m.base_high, false),
        _ => (None, false),
    };
    MineralSummary {
        id: m.id.clone(),
        name: m.name.clone(),
        refined: m.refined.as_ref().map(|r| r.name.clone()),
        kinds: m.kinds.clone(),
        places: m.places.len(),
        spots: m.spots.len(),
        best_high,
        best_high_uncertain,
        difficulty: m
            .places
            .iter()
            .filter_map(|p| p.difficulty.as_ref().map(|d| d.grade))
            .max(),
        has_override: m.overrides.iter().any(|o| o.places > 0),
    }
}

// ------------------------------------------------------------------ the rules

/// Grade points of one input: below `easy` 0, below `hard` 1, else 2.
fn points(v: f64, easy: f64, hard: f64) -> u8 {
    if v < easy {
        0
    } else if v < hard {
        1
    } else {
        2
    }
}

/// How hard an ore is to break (an estimate). Each known input scores 0-2:
///
/// - resistance (the share of laser power the rock shrugs off, -1..1): < 0.2 is 0, < 0.6 is 1;
/// - instability (how wildly the charge swings, 0..1000): < 100 is 0, < 500 is 1;
/// - window thinness (higher is a narrower optimal window): < 0.6 is 0, < 1.5 is 1.
///
/// The grade is the share of the points the known inputs can reach: under 25 % easy,
/// under 60 % medium, else hard (all three known: 0-1 easy, 2-3 medium, 4-6 hard).
/// `None` without the element, or without both resistance and instability.
pub fn difficulty(e: Option<&ElementIn>) -> Option<Difficulty> {
    let e = e?;
    if e.resistance.is_none() && e.instability.is_none() {
        return None;
    }
    let inputs = [
        ("resistance", e.resistance, 0.2, 0.6),
        ("instability", e.instability, 100.0, 500.0),
        ("window thinness", e.optimal_window_thinness, 0.6, 1.5),
    ];
    let (mut got, mut max, mut missing) = (0u8, 0u8, Vec::new());
    for (name, v, easy, hard) in inputs {
        match v {
            Some(v) => {
                got += points(v, easy, hard);
                max += 2;
            }
            None => missing.push(name.to_string()),
        }
    }
    let ratio = f64::from(got) / f64::from(max);
    let grade = if ratio < 0.25 {
        Grade::Easy
    } else if ratio < 0.6 {
        Grade::Medium
    } else {
        Grade::Hard
    };
    Some(Difficulty {
        grade,
        points: got,
        max_points: max,
        resistance: e.resistance,
        instability: e.instability,
        window_thinness: e.optimal_window_thinness,
        missing,
    })
}

/// Ship classes compared on a ship rock: lasers and their size.
const SHIPS: [(&str, u8, u8); 2] = [("Prospector", 1, 1), ("MOLE", 3, 2)];

/// "39 %", "120 %".
fn pct(v: f64) -> String {
    format!("{} %", en_us((v * 100.0).round()))
}

/// The tool a rock takes, from its mining kind (data, not an estimate), and for ship rocks
/// the Prospector (1× S1) and MOLE (3× S2) against it, an estimate:
///
/// `effective power = best laser power of the size × lasers × (1 − resistance)`
///
/// Whether that breaks a rock depends on the rock's mass, which the game files do not hold
/// (rolled at spawn), so no ship is picked: the numbers are shown and the reason says so.
pub fn recommend(
    kind: Option<&str>,
    e: Option<&ElementIn>,
    lasers: &[LaserClass],
    lasers_read: bool,
) -> Recommendation {
    let plain = |tool, label: &str, reason: &str| Recommendation {
        tool,
        label: label.into(),
        reason: reason.into(),
        estimate: false,
        formula: None,
        options: Vec::new(),
    };
    match kind {
        Some("hand") => {
            return plain(
                Tool::Hand,
                "Hand (multitool)",
                "A hand-mined rock (FPS mineable): a multitool with a mining head.",
            );
        }
        Some("vehicle") => {
            return plain(
                Tool::Roc,
                "ROC",
                "A ground vehicle rock: a ROC or ROC-DS mines it.",
            );
        }
        Some("ship") => {}
        _ => {
            return plain(
                Tool::Unknown,
                "Not known",
                "Not known: the data does not say which kind of laser this rock takes.",
            );
        }
    }
    let r = e.and_then(|e| e.resistance);
    let options: Vec<ShipOption> = SHIPS
        .iter()
        .map(|&(ship, count, size)| {
            let class = lasers.iter().find(|l| l.size == size);
            let power = class.map(|c| c.max * f64::from(count));
            ShipOption {
                ship: ship.into(),
                laser_count: count,
                laser_size: size,
                best_laser: class.map(|c| c.best.clone()),
                power,
                effective: power.zip(r).map(|(p, r)| (p * (1.0 - r)).max(0.0)),
            }
        })
        .collect();
    let numbers = match (r, lasers_read && !lasers.is_empty()) {
        (Some(r), true) => {
            let eff: Vec<String> = options
                .iter()
                .filter_map(|o| Some(format!("{} ~{}", o.ship, en_us(o.effective?.round()))))
                .collect();
            format!(
                "resistance {} leaves {} of laser power ({})",
                en_us(r),
                pct((1.0 - r).max(0.0)),
                eff.join(", ")
            )
        }
        (Some(r), false) => format!(
            "resistance {}; laser power not known (ship weapons not read)",
            en_us(r)
        ),
        (None, true) => "resistance not known (mining physics not read)".into(),
        (None, false) => "resistance and laser power not known".into(),
    };
    Recommendation {
        tool: Tool::Ship,
        label: "Ship: Prospector or MOLE".into(),
        reason: format!(
            "Rock mass not known, so whether one S1 laser is enough is not known; {numbers}."
        ),
        estimate: true,
        formula: Some(
            "effective power = best laser power × lasers × (1 − resistance); breaking also \
             needs the rock's mass, which is rolled at spawn and not in the game files"
                .into(),
        ),
        options,
    }
}

/// Laser classes (S1, S2) of a laser list.
pub fn laser_classes(lasers: &[Laser]) -> Vec<LaserClass> {
    let mut out: Vec<LaserClass> = Vec::new();
    for l in lasers {
        match out.iter_mut().find(|c| c.size == l.size) {
            Some(c) => {
                c.count += 1;
                c.min = c.min.min(l.power);
                if l.power > c.max {
                    c.max = l.power;
                    c.best = l.name.clone();
                }
            }
            None => out.push(LaserClass {
                size: l.size,
                count: 1,
                min: l.power,
                max: l.power,
                best: l.name.clone(),
            }),
        }
    }
    out.sort_by_key(|c| c.size);
    out
}

/// How specifically a curve override (`Pyro System`, `QV Breaker Station`) names this spawn
/// row's place: 2 when it is the row's body or provider key, 1 when "<name> System" is the
/// row's (real) system, `None` when it is not this place. No other match is tried, so a
/// station or a moon the rows do not name matches nothing.
pub fn override_match(
    location: &str,
    system: Option<&str>,
    body: Option<&str>,
    key: &str,
) -> Option<u8> {
    let l = location.trim();
    if body.is_some_and(|b| b.trim().eq_ignore_ascii_case(l)) || key.eq_ignore_ascii_case(l) {
        return Some(2);
    }
    let lower = l.to_lowercase();
    let sys = lower.strip_suffix(" system")?;
    system
        .is_some_and(|s| s.trim().eq_ignore_ascii_case(sys.trim()))
        .then_some(1)
}

/// [`override_match`] as a yes or no.
pub fn override_applies(
    location: &str,
    system: Option<&str>,
    body: Option<&str>,
    key: &str,
) -> bool {
    override_match(location, system, body, key).is_some()
}

/// Spawn data "systems" that are groups of places, not star systems, and the system they are
/// in when that is known (Aaron Halo is Stanton's belt); `None`: not known (Lagrange points
/// and clusters sit in more than one system).
const GROUP_SYSTEMS: [(&str, Option<&str>); 6] = [
    ("aaronhalo", Some("Stanton")),
    ("lagrange", None),
    ("asteroidcluster", None),
    ("resourcerush", None),
    ("shipgraveyard", None),
    ("spacederelict", None),
];

/// The real star system of a spawn row's `system`: a group ([`GROUP_SYSTEMS`]) maps to its
/// system or to `None`; a body key used as a system (`Stanton2c`, the Yela belt) to its
/// leading letters (`Stanton`); anything else is kept.
pub fn real_system(system: Option<&str>) -> Option<String> {
    let s = system?.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some((_, real)) = GROUP_SYSTEMS.iter().find(|(g, _)| *g == lower) {
        return real.map(str::to_string);
    }
    let letters = s.chars().take_while(char::is_ascii_alphabetic).count();
    if letters > 0 && s[letters..].starts_with(|c: char| c.is_ascii_digit()) {
        return Some(s[..letters].to_string());
    }
    Some(s.to_string())
}

/// A spawn row's readable name: its body, else `<body of the key's prefix> <rest>`
/// (`Stanton2c_Belt` -> `Yela Belt`), else the key as words (`Lagrange A`).
pub fn place_label(
    body: Option<&str>,
    key: &str,
    body_of: &dyn Fn(&str) -> Option<String>,
) -> String {
    if let Some(b) = body.filter(|b| !b.trim().is_empty()) {
        return b.to_string();
    }
    if let Some((prefix, rest)) = key.split_once('_')
        && let Some(b) = body_of(prefix)
    {
        return format!("{b} {}", group_label(rest));
    }
    let l = group_label(key);
    if l.is_empty() { key.to_string() } else { l }
}

/// A place's quality on curve `range` (the override's, else the base).
pub fn place_quality(
    range: &QualityRange,
    bands: &[super::quality::BandIn],
    override_location: Option<String>,
) -> PlaceQuality {
    let all = steps(bands, range);
    let top: Vec<QualityStep> = all.iter().rev().take(2).rev().cloned().collect();
    let top_share = if top.is_empty() || top.iter().any(|s| s.share.is_none()) {
        None
    } else {
        Some(top.iter().filter_map(|s| s.share).sum())
    };
    PlaceQuality {
        range: range.clone(),
        source: if override_location.is_some() {
            CurveSource::Override
        } else {
            CurveSource::Base
        },
        override_location,
        full_scale: true,
        high: share_at_least(range, HIGH_QUALITY),
        top,
        top_share,
    }
}

// ------------------------------------------------------------------ the index

fn title(core: &str) -> String {
    let mut c = core.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn display(r: &ResourceRow) -> String {
    r.display_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| r.name.clone())
}

fn cref(r: &ResourceRow) -> CommodityRef {
    CommodityRef {
        id: r.record_name.clone(),
        name: display(r),
    }
}

/// An ore key the index skips: templates and test elements.
fn skipped(ore: &str) -> bool {
    let o = ore.to_ascii_lowercase();
    o.contains("template") || o.starts_with("testelement")
}

/// The resource an ore is: the element's `resourceType` when `game-mining` is read, else the
/// resource whose name core is the ore's (raw or ore forms first, then any with a curve).
fn resource_for<'a>(
    data: &'a CommodityData,
    ore: &str,
    element: Option<&ElementIn>,
) -> Option<&'a ResourceRow> {
    if let Some(name) = element
        .and_then(|e| e.name.as_deref())
        .filter(|n| !n.is_empty())
        && let Some(r) = data
            .resources
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
    {
        return Some(r);
    }
    let core = ore_alias(ore_core(ore));
    let rank = |r: &ResourceRow| match (r.quality.is_some(), r.refines_to.is_some()) {
        (true, _) => 0,
        (false, true) => 1,
        _ => 2,
    };
    data.resources
        .iter()
        .filter(|r| resource_core(&r.name) == core)
        .filter(|r| r.quality.is_some() || r.refines_to.is_some())
        .min_by_key(|r| rank(r))
}

fn physics_of(ore: &str, e: &ElementIn) -> ElementPhysics {
    ElementPhysics {
        ore: ore.to_string(),
        resistance: e.resistance,
        instability: e.instability,
        window_midpoint: e.optimal_window_midpoint,
        window_randomness: e.optimal_window_randomness,
        window_thinness: e.optimal_window_thinness,
        explosion_multiplier: e.explosion_multiplier,
        cluster_factor: e.cluster_factor,
    }
}

/// The index: every resource with a quality curve or a mined ore, with its places.
/// `elements` (`game-mining`) and `weapons` (`game-weapons`) are optional; without them the
/// physics, difficulty and laser numbers say "not known".
///
/// `spawns` (`game-mining-spawns`) and `areas` (`game-item-sources`) may be missing too: the
/// index then says so ([`MiningIndex::spawns_read`], [`MiningIndex::spots_read`]) instead of
/// "no place".
pub fn build_index(
    data: &CommodityData,
    spawns: Option<&MiningSpawns>,
    areas: Option<&[LootArea]>,
    elements: Option<&[ElementIn]>,
    weapons: Option<&[WeaponIn]>,
) -> MiningIndex {
    let (spawns_read, spots_read) = (spawns.is_some(), areas.is_some());
    let empty = MiningSpawns::default();
    let spawns = spawns.unwrap_or(&empty);
    let areas = areas.unwrap_or_default();
    let by_ore: HashMap<String, &ElementIn> = elements
        .unwrap_or_default()
        .iter()
        .map(|e| {
            let short = e.record_name.rsplit('.').next().unwrap_or(&e.record_name);
            (short.to_ascii_lowercase(), e)
        })
        .collect();
    let lasers = laser_classes(&lasers_from(weapons.unwrap_or_default()));
    let lasers_read = weapons.is_some();

    // Every ore key, in a stable order.
    let mut ores: Vec<String> = Vec::new();
    for o in spawns.ore_spawns.iter().map(|s| s.ore.as_str()).chain(
        spawns
            .rocks
            .iter()
            .flat_map(|r| r.ores.iter().map(|o| o.element.as_str())),
    ) {
        if !skipped(o) && !ores.iter().any(|x| x.eq_ignore_ascii_case(o)) {
            ores.push(o.to_string());
        }
    }
    ores.sort();

    // Group ores by the resource they are; an ore no resource matches stands alone.
    let mut groups: BTreeMap<String, (Option<&ResourceRow>, Vec<String>)> = BTreeMap::new();
    for ore in &ores {
        let r = resource_for(data, ore, by_ore.get(&ore.to_ascii_lowercase()).copied());
        let key = r.map_or_else(|| format!("ore:{ore}"), |r| r.record_name.clone());
        groups
            .entry(key)
            .or_insert((r, Vec::new()))
            .1
            .push(ore.clone());
    }
    for r in data.resources.iter().filter(|r| r.quality.is_some()) {
        groups
            .entry(r.record_name.clone())
            .or_insert((Some(r), Vec::new()));
    }

    // A rock's ores by display name, for companions.
    let ore_name = |ore: &str| -> String {
        resource_for(data, ore, by_ore.get(&ore.to_ascii_lowercase()).copied())
            .map(display)
            .unwrap_or_else(|| title(&ore_core(ore)))
    };

    let mut minerals: Vec<MineralDetail> = groups
        .into_iter()
        .filter_map(|(id, (res, ores))| {
            let m = mineral(
                data,
                spawns,
                areas,
                &by_ore,
                &lasers,
                lasers_read,
                &ore_name,
                id,
                res,
                ores,
            );
            // An ore without a resource and without a place says nothing.
            (m.commodity.is_some() || !m.places.is_empty() || !m.spots.is_empty()).then_some(m)
        })
        .collect();
    minerals.sort_by(|a, b| compare_names(&a.name, &b.name).then_with(|| a.id.cmp(&b.id)));
    MiningIndex {
        minerals,
        lasers,
        physics: elements.is_some(),
        lasers_read,
        spawns_read,
        spots_read,
    }
}

#[allow(clippy::too_many_arguments)]
fn mineral(
    data: &CommodityData,
    spawns: &MiningSpawns,
    areas: &[LootArea],
    by_ore: &HashMap<String, &ElementIn>,
    lasers: &[LaserClass],
    lasers_read: bool,
    ore_name: &dyn Fn(&str) -> String,
    id: String,
    res: Option<&ResourceRow>,
    ores: Vec<String>,
) -> MineralDetail {
    let q = res.and_then(|r| r.quality.as_ref());
    let base = q.and_then(|q| q.distribution.clone());
    let bands: &[super::quality::BandIn] = q.map_or(&[], |q| q.bands.as_slice());
    let element = |ore: &str| by_ore.get(&ore.to_ascii_lowercase()).copied();

    // One row per ore, location and rock: a rock several spawn groups roll adds up.
    let mut places: Vec<MiningPlace> = Vec::new();
    for s in spawns
        .ore_spawns
        .iter()
        .filter(|s| ores.iter().any(|o| o.eq_ignore_ascii_case(&s.ore)))
    {
        if let Some(p) = places
            .iter_mut()
            .find(|p| p.ore == s.ore && p.location == s.location && p.rock == s.rock)
        {
            p.spawn_share = match (p.spawn_share, s.spawn_share) {
                (Some(x), Some(y)) => Some(x + y),
                (x, y) => x.or(y),
            };
            p.min_percent = match (p.min_percent, s.min_percent) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            p.max_percent = match (p.max_percent, s.max_percent) {
                (Some(x), Some(y)) => Some(x.max(y)),
                (x, y) => x.or(y),
            };
            if let Some(qs) = s.quality_scale
                && !p.quality_scales.contains(&qs)
            {
                p.quality_scales.push(qs);
            }
            continue;
        }
        let system = real_system(s.system.as_deref());
        // The most specific override wins: a body or key over a system.
        let over = q.and_then(|q| {
            q.location_overrides
                .iter()
                .filter_map(|o| {
                    override_match(
                        &o.location,
                        system.as_deref(),
                        s.body.as_deref(),
                        &s.location,
                    )
                    .map(|rank| (rank, o))
                })
                .max_by_key(|(rank, _)| *rank)
                .map(|(_, o)| o)
        });
        // A system override this row's unknown system might be.
        let maybe = system.is_none()
            && q.is_some_and(|q| {
                q.location_overrides
                    .iter()
                    .any(|o| o.location.trim().to_lowercase().ends_with(" system"))
            });
        let mut quality = match (over, &base) {
            (Some(o), _) => place_quality(&o.range, bands, Some(o.location.clone())),
            (None, Some(b)) => place_quality(b, bands, None),
            (None, None) => PlaceQuality {
                range: QualityRange::default(),
                source: CurveSource::Base,
                override_location: None,
                full_scale: true,
                high: None,
                top: Vec::new(),
                top_share: None,
            },
        };
        if over.is_none() && maybe {
            quality.source = CurveSource::Unknown;
        }
        let body_of = |key: &str| {
            spawns
                .ore_spawns
                .iter()
                .find(|x| x.location.eq_ignore_ascii_case(key))
                .and_then(|x| x.body.clone())
        };
        let rock = spawns
            .rocks
            .iter()
            .find(|r| r.rock.eq_ignore_ascii_case(&s.rock));
        let mut companions: Vec<String> = Vec::new();
        for o in rock.map(|r| r.ores.as_slice()).unwrap_or_default() {
            if o.element.eq_ignore_ascii_case(&s.ore) || skipped(&o.element) {
                continue;
            }
            let n = ore_name(&o.element);
            if !companions.contains(&n) {
                companions.push(n);
            }
        }
        let e = element(&s.ore);
        places.push(MiningPlace {
            ore: s.ore.clone(),
            location: s.location.clone(),
            place: place_label(s.body.as_deref(), &s.location, &body_of),
            system,
            body: s.body.clone(),
            rock: s.rock.clone(),
            kind: s.kind.clone(),
            tier: s.tier.clone(),
            signature: s.signature.or_else(|| rock.and_then(|r| r.signature)),
            spawn_share: s.spawn_share,
            min_percent: s.min_percent,
            max_percent: s.max_percent,
            quality_scales: s.quality_scale.into_iter().collect(),
            companions,
            rock_mass: None,
            quality,
            difficulty: difficulty(e),
            recommendation: recommend(s.kind.as_deref(), e, lasers, lasers_read),
        });
    }
    for p in &mut places {
        p.quality.full_scale = p.quality_scales.iter().all(|x| *x == 1.0);
    }
    let kind_rank = |k: Option<&str>| match k {
        Some("ship") => 0,
        Some("vehicle") => 1,
        Some("hand") => 2,
        _ => 3,
    };
    places.sort_by(|a, b| {
        kind_rank(a.kind.as_deref())
            .cmp(&kind_rank(b.kind.as_deref()))
            .then_with(|| {
                b.spawn_share
                    .unwrap_or(0.0)
                    .total_cmp(&a.spawn_share.unwrap_or(0.0))
            })
            .then_with(|| compare_names(&a.location, &b.location))
    });

    let mut spots: Vec<MinedSpot> = Vec::new();
    for ore in &ores {
        for rep in where_mined(spawns, areas, ore) {
            for s in rep.spots {
                if !spots.iter().any(|x| x.area == s.area && x.rock == s.rock) {
                    spots.push(s);
                }
            }
        }
    }
    spots.sort_by(|a, b| {
        b.slot_share
            .total_cmp(&a.slot_share)
            .then_with(|| a.area.cmp(&b.area))
    });

    let rock_kind = |rock: &str| {
        spawns
            .rocks
            .iter()
            .find(|r| r.rock.eq_ignore_ascii_case(rock))
            .and_then(|r| r.kind.clone())
    };
    let kinds: Vec<String> = ["hand", "vehicle", "ship"]
        .into_iter()
        .filter(|k| {
            places.iter().any(|p| p.kind.as_deref() == Some(*k))
                || spots
                    .iter()
                    .any(|s| rock_kind(&s.rock).as_deref() == Some(*k))
        })
        .map(str::to_string)
        .collect();
    let overrides = q.map_or_else(Vec::new, |q| {
        q.location_overrides
            .iter()
            .map(|o| OverrideUse {
                location: o.location.clone(),
                range: o.range.clone(),
                places: places
                    .iter()
                    .filter(|p| p.quality.override_location.as_deref() == Some(o.location.as_str()))
                    .count(),
            })
            .collect()
    });
    let elements = ores
        .iter()
        .filter_map(|o| Some(physics_of(o, element(o)?)))
        .collect();
    let refined = res.and_then(|r| r.refines_to.as_deref()).map(|t| {
        data.resources
            .iter()
            .find(|x| x.name.eq_ignore_ascii_case(t))
            .map(cref)
            .unwrap_or_else(|| CommodityRef {
                id: format!("ResourceType.{t}"),
                name: group_label(t),
            })
    });
    let name = res.map_or_else(
        || title(&ore_core(ores.first().map_or("", String::as_str))),
        display,
    );
    MineralDetail {
        id,
        name,
        commodity: res.map(cref),
        refined,
        tier: q.and_then(|q| q.tier.clone()),
        steps: base.as_ref().map_or_else(Vec::new, |b| steps(bands, b)),
        base_high: base.as_ref().and_then(|b| share_at_least(b, HIGH_QUALITY)),
        base,
        overrides,
        elements,
        places,
        spots,
        kinds,
        ores,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::types::{SpawnConfig, SpawnSlot};
    use serde_json::json;

    fn data() -> CommodityData {
        serde_json::from_value(json!({"resources": [
            {"name": "Raw_Quantainium", "recordName": "ResourceType.Raw_Quantainium", "displayName": "Quantainium (Raw)",
             "group": "Mineral", "subgroups": ["Raw_Minerals"], "refinesTo": "Quantanium",
             "quality": {"tier": "Legendary", "distribution": {"min": 501, "max": 1000, "mean": 500, "stddev": 200},
                "locationOverrides": [{"location": "Pyro System", "min": 501, "max": 1000, "mean": 600, "stddev": 250},
                                      {"location": "QV Breaker Station", "min": 700, "max": 1000, "mean": 800, "stddev": 100},
                                      {"location": "Monox", "min": 501, "max": 1000, "mean": 900, "stddev": 50}],
                "bands": [{"start": 0, "end": 599, "value": 514}, {"start": 600, "end": 899, "value": 762},
                          {"start": 900, "end": 1000, "value": 974}]}},
            {"name": "Quantanium", "recordName": "ResourceType.Quantanium", "displayName": "Quantanium", "group": "Mineral"},
            {"name": "Ore_Aluminum", "recordName": "ResourceType.Ore_Aluminum", "displayName": "Aluminum (Ore)",
             "group": "Metal", "refinesTo": "Aluminum",
             "quality": {"distribution": {"min": 0, "max": 1000}, "bands": [{"start": 0, "end": 1000, "value": 500}]}},
            {"name": "Aluminum", "recordName": "ResourceType.Aluminum", "group": "Metal"},
            {"name": "Sadaryx", "recordName": "ResourceType.Sadaryx", "group": "Mineral",
             "quality": {"tier": "FPSMineable", "distribution": {"min": 201, "max": 1000, "mean": 201, "stddev": 298},
                "bands": [{"start": 0, "end": 399, "value": 348}, {"start": 400, "end": 1000, "value": 800}]}},
            {"name": "QuasiEgg", "recordName": "ResourceType.QuasiEgg", "group": "Organic",
             "quality": {"distribution": {"min": 0, "max": 500}}}
        ]}))
        .unwrap()
    }

    fn spawns() -> MiningSpawns {
        serde_json::from_value(json!({
            "rocks": [
                {"rock": "Rock_Q", "kind": "ship", "signature": 3900, "ores": [
                    {"element": "quantainium_raw", "minPercent": 20, "maxPercent": 50},
                    {"element": "aluminium_ore"}, {"element": "testelement_balance"}]},
                {"rock": "MineableRock_FPS_Sadaryx", "kind": "hand", "ores": [{"element": "minableelement_fps_sadaryx"}]},
                {"rock": "Rock_Gem", "kind": "vehicle", "ores": [{"element": "minableelement_groundvehicle_mockite"}]}
            ],
            "oreSpawns": [
                {"ore": "quantainium_raw", "location": "Stanton1", "system": "Stanton", "body": "Hurston",
                 "rock": "Rock_Q", "kind": "ship", "tier": "Legendary", "spawnShare": 0.01, "minPercent": 20, "maxPercent": 40, "qualityScale": 1},
                {"ore": "quantainium_raw", "location": "Stanton1", "system": "Stanton", "body": "Hurston",
                 "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.02, "minPercent": 25, "maxPercent": 50, "qualityScale": 0.49},
                {"ore": "quantainium_raw", "location": "Pyro2", "system": "Pyro", "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.005},
                {"ore": "quantainium_raw", "location": "Pyro3", "system": "Pyro", "body": "Monox", "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.004},
                {"ore": "quantainium_raw", "location": "Lagrange_A", "system": "Lagrange", "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.003},
                {"ore": "quantainium_raw", "location": "Stanton1_Belt", "system": "Stanton1", "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.002, "qualityScale": 0.49},
                {"ore": "aluminium_ore", "location": "Stanton1", "system": "Stanton", "body": "Hurston", "rock": "Rock_Q", "kind": "ship", "spawnShare": 0.3},
                {"ore": "minableelement_groundvehicle_mockite", "location": "Stanton3a", "system": "Stanton", "body": "Lyria",
                 "rock": "Rock_Gem", "kind": "vehicle", "spawnShare": 0.4}
            ]
        }))
        .unwrap()
    }

    fn areas() -> Vec<LootArea> {
        vec![LootArea {
            name: "v3slotpreset_rockcracker".into(),
            configs: vec![SpawnConfig {
                name: Some("Sadaryx minable".into()),
                slots: vec![SpawnSlot {
                    entity: "mineablerock_fps_sadaryx".into(),
                    weight: 1.0,
                    ..Default::default()
                }],
                ..Default::default()
            }],
        }]
    }

    fn elements() -> Vec<ElementIn> {
        serde_json::from_value(json!([
            {"name": "Raw_Quantainium", "recordName": "MineableElement.Quantainium_Raw", "instability": 1000,
             "resistance": 0.95, "optimalWindowMidpoint": 0.5, "optimalWindowRandomness": 0.2, "optimalWindowThinness": 2.3},
            {"name": "Ore_Aluminum", "recordName": "MineableElement.Aluminium_Ore", "instability": 0, "resistance": -0.4,
             "optimalWindowThinness": -0.5},
            {"name": "Sadaryx", "recordName": "MineableElement.MinableElement_FPS_Sadaryx", "instability": 200, "resistance": 0}
        ]))
        .unwrap()
    }

    fn weapons() -> Vec<WeaponIn> {
        serde_json::from_value(json!([
            {"name": "Arbor MH1", "recordName": "EntityClassDefinition.Mining_Laser_Arbor_S1", "type": "WeaponMining", "size": 1,
             "modes": [{"kind": "beam", "dps": 1890}, {"kind": "beam", "dps": 1850}]},
            {"name": "Helix I", "recordName": "EntityClassDefinition.Mining_Laser_Helix_S1", "type": "WeaponMining", "size": 1,
             "modes": [{"kind": "beam", "dps": 3900}]},
            {"name": "Helix I", "recordName": "EntityClassDefinition.Mining_Laser_Helix_S1_Test_Active", "type": "WeaponMining", "size": 1,
             "modes": [{"kind": "beam", "dps": 9999}]},
            {"name": "Helix II", "recordName": "EntityClassDefinition.Mining_Laser_Helix_S2", "type": "WeaponMining", "size": 2,
             "modes": [{"kind": "beam", "dps": 4930}]},
            {"name": "Arbor MHV", "recordName": "EntityClassDefinition.Mining_Laser_Arbor_S0", "type": "WeaponMining", "size": 0,
             "modes": [{"kind": "beam", "dps": 0.8}]},
            {"name": "Gun", "recordName": "EntityClassDefinition.Gun_S1", "type": "WeaponGun", "size": 1,
             "modes": [{"kind": "single", "dps": 400}]}
        ]))
        .unwrap()
    }

    fn full() -> MiningIndex {
        let (e, w) = (elements(), weapons());
        build_index(&data(), Some(&spawns()), Some(&areas()), Some(&e), Some(&w))
    }

    #[test]
    fn index_groups_ores_by_resource_and_links_forms() {
        let ix = full();
        let names: Vec<&str> = ix.minerals.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Aluminum (Ore)",
                "Mockite",
                "Quantainium (Raw)",
                "QuasiEgg",
                "Sadaryx"
            ]
        );
        let q = ix.detail("ResourceType.Raw_Quantainium").unwrap();
        assert_eq!(q.refined.as_ref().unwrap().id, "ResourceType.Quantanium");
        assert_eq!(q.commodity.as_ref().unwrap().id, q.id);
        assert_eq!(q.ores, ["quantainium_raw"]);
        // Found by an ore key too; an ore without a resource keeps its own id.
        assert_eq!(ix.detail("quantainium_raw").unwrap().id, q.id);
        let gem = ix
            .detail("ore:minableelement_groundvehicle_mockite")
            .unwrap();
        assert!(gem.commodity.is_none() && gem.base.is_none());
        // The test element is no mineral; the rock's other ore is a companion.
        assert!(ix.minerals.iter().all(|m| !m.id.contains("testelement")));
        assert_eq!(q.places[0].companions, ["Aluminum (Ore)"]);
        assert_eq!(ix.lasers.len(), 2);
    }

    #[test]
    fn spelling_differences_match_without_the_element_link() {
        let ix = build_index(&data(), Some(&spawns()), Some(&areas()), None, None);
        let al = ix.detail("ResourceType.Ore_Aluminum").unwrap();
        assert_eq!(al.ores, ["aluminium_ore"]);
        assert!(al.elements.is_empty() && !ix.physics && !ix.lasers_read);
        assert!(al.places[0].difficulty.is_none());
    }

    #[test]
    fn places_merge_groups_and_take_their_override() {
        let q = full();
        let q = q.detail("ResourceType.Raw_Quantainium").unwrap();
        assert_eq!(q.places.len(), 5);
        let hurston = q.places.iter().find(|p| p.location == "Stanton1").unwrap();
        assert!((hurston.spawn_share.unwrap() - 0.03).abs() < 1e-9);
        assert_eq!(
            (hurston.min_percent, hurston.max_percent),
            (Some(20.0), Some(50.0))
        );
        assert_eq!(hurston.quality_scales, [1.0, 0.49]);
        assert_eq!(hurston.signature, Some(3900.0));
        assert_eq!(hurston.quality.override_location, None, "same as base");
        assert_eq!(hurston.quality.range.mean, Some(500.0));
        let pyro = q.places.iter().find(|p| p.location == "Pyro2").unwrap();
        assert_eq!(
            pyro.quality.override_location.as_deref(),
            Some("Pyro System")
        );
        assert_eq!(pyro.quality.range.mean, Some(600.0));
        // The Pyro curve sits higher: more high rolls.
        assert!(pyro.quality.high.unwrap() > hurston.quality.high.unwrap());
        // The station override names no spawn row.
        let uses: Vec<(&str, usize)> = q
            .overrides
            .iter()
            .map(|o| (o.location.as_str(), o.places))
            .collect();
        assert_eq!(
            uses,
            [("Pyro System", 1), ("QV Breaker Station", 0), ("Monox", 1)]
        );
        // The body override beats the system one; the Lagrange point's system is not known.
        let monox = q.places.iter().find(|p| p.location == "Pyro3").unwrap();
        assert_eq!(monox.quality.override_location.as_deref(), Some("Monox"));
        let lag = q
            .places
            .iter()
            .find(|p| p.location == "Lagrange_A")
            .unwrap();
        assert_eq!(lag.quality.source, CurveSource::Unknown);
        assert_eq!(
            (lag.system.as_deref(), lag.place.as_str()),
            (None, "Lagrange A")
        );
        assert!(!lag.quality.trusted());
        // A belt keyed after a body: its real system and a readable name; a scale below 1 marks it.
        let belt = q
            .places
            .iter()
            .find(|p| p.location == "Stanton1_Belt")
            .unwrap();
        assert_eq!(
            (belt.system.as_deref(), belt.place.as_str()),
            (Some("Stanton"), "Hurston Belt")
        );
        assert!(!belt.quality.full_scale && !belt.quality.trusted());
        assert_eq!(hurston.quality.source, CurveSource::Base);
        assert!(!hurston.quality.full_scale, "1 and 0.49 merged");
        assert!(q.places.iter().all(|p| p.rock_mass.is_none()));
    }

    #[test]
    fn top_steps_and_high_share() {
        let r = QualityRange {
            min: Some(501.0),
            max: Some(1000.0),
            mean: Some(500.0),
            stddev: Some(200.0),
        };
        let bands = &data().resources[0].quality.clone().unwrap().bands;
        let p = place_quality(&r, bands, None);
        assert_eq!(
            p.top.iter().map(|s| s.shown).collect::<Vec<_>>(),
            [762.0, 974.0]
        );
        let top = p.top_share.unwrap();
        assert!(top > 0.0 && top < 1.0);
        let h = p.high.unwrap();
        assert!(h > 0.0 && h < 1.0, "{h}");
        // Certain answers need no spread.
        let low = QualityRange {
            min: Some(0.0),
            max: Some(500.0),
            ..Default::default()
        };
        assert_eq!(share_at_least(&low, HIGH_QUALITY), Some(0.0));
        let all = QualityRange {
            min: Some(700.0),
            max: Some(1000.0),
            ..Default::default()
        };
        assert_eq!(share_at_least(&all, HIGH_QUALITY), Some(1.0));
        let open = QualityRange {
            min: Some(0.0),
            max: Some(1000.0),
            ..Default::default()
        };
        assert_eq!(share_at_least(&open, HIGH_QUALITY), None);
        // A one-step curve without spread: steps but no shares.
        let one = place_quality(
            &open,
            &data().resources[2].quality.clone().unwrap().bands,
            None,
        );
        assert_eq!(one.top.len(), 1);
        assert_eq!(one.top_share, None);
    }

    #[test]
    fn spots_and_kinds_in_the_summary() {
        let ix = full();
        let s = ix.summaries();
        let sad = s.iter().find(|m| m.name == "Sadaryx").unwrap();
        assert_eq!((sad.places, sad.spots), (0, 1));
        assert_eq!(sad.kinds, ["hand"]);
        assert_eq!(
            sad.best_high,
            ix.detail("ResourceType.Sadaryx").unwrap().base_high
        );
        let q = s.iter().find(|m| m.name == "Quantainium (Raw)").unwrap();
        assert_eq!(q.kinds, ["ship"]);
        assert_eq!(q.difficulty, Some(Grade::Hard));
        assert!(q.has_override);
        assert_eq!(q.refined.as_deref(), Some("Quantanium"));
        let gem = s.iter().find(|m| m.name == "Mockite").unwrap();
        assert_eq!(gem.kinds, ["vehicle"]);
        // A curve and no place at all.
        let egg = s.iter().find(|m| m.name == "QuasiEgg").unwrap();
        assert_eq!((egg.places, egg.spots, egg.best_high), (0, 0, None));
    }

    #[test]
    fn difficulty_grades_and_missing_inputs() {
        let e = |r: Option<f64>, i: Option<f64>, t: Option<f64>| ElementIn {
            resistance: r,
            instability: i,
            optimal_window_thinness: t,
            ..Default::default()
        };
        let d = |r, i, t| difficulty(Some(&e(r, i, t))).map(|d| (d.grade, d.points, d.max_points));
        assert_eq!(
            d(Some(0.95), Some(1000.0), Some(2.3)),
            Some((Grade::Hard, 6, 6))
        );
        assert_eq!(
            d(Some(-0.4), Some(50.0), Some(-0.9)),
            Some((Grade::Easy, 0, 6))
        );
        assert_eq!(
            d(Some(0.5), Some(400.0), Some(0.5)),
            Some((Grade::Medium, 2, 6))
        );
        assert_eq!(
            d(Some(0.3), Some(40.0), Some(0.5)),
            Some((Grade::Easy, 1, 6))
        );
        // Thresholds are lower bounds of the next band.
        assert_eq!(
            d(Some(0.2), Some(100.0), Some(0.6)),
            Some((Grade::Medium, 3, 6))
        );
        assert_eq!(
            d(Some(0.6), Some(500.0), Some(1.5)),
            Some((Grade::Hard, 6, 6))
        );
        // A missing window: graded on what is known, and said.
        let m = difficulty(Some(&e(Some(0.95), Some(1000.0), None))).unwrap();
        assert_eq!((m.grade, m.points, m.max_points), (Grade::Hard, 4, 4));
        assert_eq!(m.missing, ["window thinness"]);
        assert_eq!(d(None, None, Some(3.0)), None);
        assert!(difficulty(None).is_none());
    }

    #[test]
    fn recommendation_from_kind_and_the_ship_estimate() {
        let w = weapons();
        let classes = laser_classes(&lasers_from(&w));
        assert_eq!(
            classes
                .iter()
                .map(|c| (c.size, c.count, c.min, c.max, c.best.as_str()))
                .collect::<Vec<_>>(),
            [
                (1, 2, 1890.0, 3900.0, "Helix I"),
                (2, 1, 4930.0, 4930.0, "Helix II")
            ]
        );
        let hand = recommend(Some("hand"), None, &classes, true);
        assert_eq!((hand.tool, hand.estimate), (Tool::Hand, false));
        assert_eq!(
            recommend(Some("vehicle"), None, &classes, true).tool,
            Tool::Roc
        );
        let unknown = recommend(None, None, &classes, true);
        assert_eq!(unknown.tool, Tool::Unknown);
        assert!(unknown.reason.starts_with("Not known"));

        let q = ElementIn {
            resistance: Some(0.95),
            ..Default::default()
        };
        let r = recommend(Some("ship"), Some(&q), &classes, true);
        assert_eq!((r.tool, r.estimate), (Tool::Ship, true));
        let p = &r.options[0];
        assert_eq!(
            (p.ship.as_str(), p.laser_count, p.power),
            ("Prospector", 1, Some(3900.0))
        );
        assert!((p.effective.unwrap() - 195.0).abs() < 1e-6);
        let mole = &r.options[1];
        assert_eq!((mole.laser_count, mole.power), (3, Some(14_790.0)));
        assert!((mole.effective.unwrap() - 739.5).abs() < 1e-6);
        assert!(r.reason.contains("Rock mass not known"), "{}", r.reason);
        assert!(r.reason.contains("leaves 5 %"), "{}", r.reason);
        assert!(r.formula.as_deref().unwrap().contains("1 − resistance"));

        // Negative resistance helps; never below zero for resistance above 1.
        let easy = ElementIn {
            resistance: Some(-0.4),
            ..Default::default()
        };
        let e = recommend(Some("ship"), Some(&easy), &classes, true);
        assert!((e.options[0].effective.unwrap() - 5460.0).abs() < 1e-6);
        let wall = ElementIn {
            resistance: Some(1.5),
            ..Default::default()
        };
        assert_eq!(
            recommend(Some("ship"), Some(&wall), &classes, true).options[0].effective,
            Some(0.0)
        );

        // Missing inputs: said, no numbers made up.
        let none = recommend(Some("ship"), None, &classes, true);
        assert!(
            none.options
                .iter()
                .all(|o| o.effective.is_none() && o.power.is_some())
        );
        assert!(none.reason.contains("resistance not known"));
        let unread = recommend(Some("ship"), Some(&q), &[], false);
        assert!(
            unread
                .options
                .iter()
                .all(|o| o.power.is_none() && o.effective.is_none())
        );
        assert!(unread.reason.contains("laser power not known"));
        assert!(
            recommend(Some("ship"), None, &[], false)
                .reason
                .contains("resistance and laser power not known")
        );
    }

    #[test]
    fn untrusted_places_never_give_the_best_share_when_others_do() {
        let ix = full();
        let s = ix.summaries();
        let q = s.iter().find(|m| m.name == "Quantainium (Raw)").unwrap();
        // Pyro2 (Pyro System override, full scale) and Pyro3 (Monox) are trusted.
        let d = ix.detail("ResourceType.Raw_Quantainium").unwrap();
        let monox = d.places.iter().find(|p| p.location == "Pyro3").unwrap();
        assert_eq!(q.best_high, monox.quality.high);
        assert!(!q.best_high_uncertain);
        assert!(ix.spawns_read && ix.spots_read);
        let none = build_index(&data(), None, None, None, None);
        assert!(!none.spawns_read && !none.spots_read);
        assert!(
            none.minerals
                .iter()
                .all(|m| m.places.is_empty() && m.spots.is_empty())
        );
    }

    #[test]
    fn real_systems_and_place_names() {
        assert_eq!(real_system(Some("Stanton")).as_deref(), Some("Stanton"));
        assert_eq!(real_system(Some("Stanton2c")).as_deref(), Some("Stanton"));
        assert_eq!(real_system(Some("Aaronhalo")).as_deref(), Some("Stanton"));
        assert_eq!(real_system(Some("Nyx")).as_deref(), Some("Nyx"));
        for g in [
            "Lagrange",
            "Asteroidcluster",
            "Resourcerush",
            "Shipgraveyard",
            "Spacederelict",
        ] {
            assert_eq!(real_system(Some(g)), None, "{g}");
        }
        assert_eq!(real_system(None), None);
        let body_of = |k: &str| (k == "Stanton2c").then(|| "Yela".to_string());
        assert_eq!(
            place_label(Some("Hurston"), "Stanton1", &body_of),
            "Hurston"
        );
        assert_eq!(place_label(None, "Stanton2c_Belt", &body_of), "Yela Belt");
        assert_eq!(
            place_label(None, "AsteroidCluster_Low_Yield", &body_of),
            "Asteroid Cluster Low Yield"
        );
        assert_eq!(
            place_label(None, "Nyx_GlaciemRing", &body_of),
            "Nyx Glaciem Ring"
        );
        assert_eq!(
            override_match("Monox", Some("Pyro"), Some("Monox"), "Pyro2"),
            Some(2)
        );
        assert_eq!(
            override_match("Pyro System", Some("Pyro"), Some("Monox"), "Pyro2"),
            Some(1)
        );
    }

    #[test]
    fn overrides_match_system_or_body_only() {
        assert!(override_applies("Pyro System", Some("Pyro"), None, "Pyro2"));
        assert!(override_applies(
            "pyro system",
            Some("PYRO"),
            Some("Pyro II"),
            "Pyro2"
        ));
        assert!(!override_applies(
            "Pyro System",
            Some("Stanton"),
            Some("Pyro"),
            "Pyro2"
        ));
        assert!(!override_applies("Pyro System", None, None, "Pyro2"));
        assert!(override_applies(
            "Daymar",
            Some("Stanton"),
            Some("Daymar"),
            "Stanton2b"
        ));
        assert!(override_applies(
            "Stanton2b",
            Some("Stanton"),
            None,
            "Stanton2b"
        ));
        assert!(!override_applies(
            "QV Breaker Station",
            Some("Pyro"),
            Some("Pyro I"),
            "Pyro1"
        ));
    }
}
