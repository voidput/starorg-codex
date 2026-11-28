//! Obtain hints for `global.ini` descriptions (spec obtain-hints): the explorer Sources
//! card's rows ([`crate::sources`]) cut down to the neutral [`starorg_core::model`] shapes
//! the WHERE TO GET generator reads. [`crate::tools::Tools::obtain_hints`] fills them for
//! every item and mined resource of a build.
//!
//! Only short place names and labels are kept, never shop prices. The one exception is the
//! commodity sell line (the best kiosk prices per SCU): it carries prices, and the generator
//! rounds them and stamps the line with the day they were fetched, as they go stale.

use crate::sources::{LootContainer, MinedReport, OreQuality};
use starorg_core::apply::{OBTAIN_LATER, ObtainFetch, ObtainProvider, ObtainSource};
use starorg_core::model::{
    LootHint, MinedHint, ObtainHints, QualityHint, QualityOverride, SellHint, ShopHint,
};
use starorg_gamedata::query::obtain::AreaSummary;
use starorg_gamedata::query::sells::{MatchStats, SellRow};
use starorg_gamedata::query::shops::LocatedShop;

/// Loot rows kept per item (the rest are counted in `loot_total`).
pub const MAX_LOOT: usize = 12;
/// Shop rows kept per item.
pub const MAX_SHOPS: usize = 12;
/// Kiosk rows kept per commodity: the text lists the top 3 and counts the rest
/// (`sells_total`).
pub const MAX_SELLS: usize = 3;

/// The hints of one build, with what could not be read.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltHints {
    pub hints: ObtainHints,
    /// One line per dataset group that failed (memory gate, extraction error): the lines it
    /// feeds are left out, the rest is kept.
    pub warnings: Vec<String>,
    /// Items and resources with at least one source.
    pub items: usize,
    /// How the UEX commodity names matched the build's resources, when the commodity prices
    /// were there.
    pub sell_match: Option<MatchStats>,
}

/// [`crate::tools::Tools::obtain_hints`] as apply's obtain-hints source (the CLI and the
/// app pass it in [`starorg_core::apply::ApplyOpts::obtain`]).
pub struct ToolsObtain {
    pub tools: std::sync::Arc<crate::tools::Tools>,
    /// Extract datasets that are not cached yet (apply); `false` reads only the cache
    /// (previews).
    pub extract: bool,
    /// Stop by then (see [`crate::tools::Tools::obtain_hints_until`]).
    pub deadline: Option<std::time::Instant>,
    /// Only hints already built whole and stored on disk, nothing worked out now (the launch
    /// hooks: their budget is for the game text; the post-exit background job builds the
    /// rest).
    pub stored_only: bool,
}

impl ToolsObtain {
    pub fn provider(tools: std::sync::Arc<crate::tools::Tools>, extract: bool) -> ObtainProvider {
        Self::provider_until(tools, extract, None)
    }

    /// [`Self::provider`] whose answers stop by `deadline`.
    pub fn provider_until(
        tools: std::sync::Arc<crate::tools::Tools>,
        extract: bool,
        deadline: Option<std::time::Instant>,
    ) -> ObtainProvider {
        ObtainProvider(std::sync::Arc::new(Self {
            tools,
            extract,
            deadline,
            stored_only: false,
        }))
    }

    /// Hints only when already built and stored ([`Self::stored_only`]); otherwise the lines
    /// are left out with an [`OBTAIN_LATER`] warning, so the output goes stale until they
    /// are.
    pub fn provider_stored(tools: std::sync::Arc<crate::tools::Tools>) -> ObtainProvider {
        ObtainProvider(std::sync::Arc::new(Self {
            tools,
            extract: false,
            deadline: None,
            stored_only: true,
        }))
    }
}

impl ObtainSource for ToolsObtain {
    fn fetch(&self, channel: &str) -> ObtainFetch {
        if self.stored_only {
            return match self.tools.stored_obtain_hints(channel) {
                Some(b) => ObtainFetch {
                    hints: Some(b.hints.clone()),
                    warnings: Vec::new(),
                },
                None => ObtainFetch {
                    hints: None,
                    warnings: vec![format!(
                        "WHERE TO GET left out: not built yet for this build; {OBTAIN_LATER}"
                    )],
                },
            };
        }
        match self
            .tools
            .obtain_hints_until(channel, self.extract, self.deadline)
        {
            Ok(b) => ObtainFetch {
                hints: Some(b.hints.clone()),
                warnings: b.warnings.clone(),
            },
            Err(e) => ObtainFetch {
                hints: None,
                warnings: vec![format!("WHERE TO GET left out: {e}")],
            },
        }
    }

    fn ready(&self, channel: &str) -> bool {
        !self.stored_only || self.tools.stored_obtain_hints(channel).is_some()
    }
}

/// Slot preset words (a word starting with the key) and the kind of place they name, first
/// match wins ([`area_place_name`]; the Loot screen drops the word from the area's name).
pub const AREA_PLACES: &[(&str, &str)] = &[
    ("cave", "Caves"),
    ("distributioncentre", "Distribution centres"),
    ("dcdelving", "Distribution centres"),
    ("asddelving", "ASD facilities"),
    ("colonialoutpost", "Outposts"),
    ("techoutpost", "Outposts"),
    ("derelictoutpost", "Derelict outposts"),
    ("spacederelict", "Derelicts"),
    ("derelict", "Derelicts"),
    ("junksite", "Junk sites"),
    ("asteroidbase", "Asteroid bases"),
    ("contestedzone", "Contested zones"),
    ("ghostarena", "Contested zones"),
    ("ugfs", "Underground facilities"),
    ("orbitalplatform", "Orbital platforms"),
    ("dishalignmentfacility", "Orbital platforms"),
    ("shippingcontainer", "Shipping containers"),
    ("station", "Stations"),
    ("kareah", "Security Post Kareah"),
    ("covalex", "Covalex hub"),
    ("jumptown", "Jumptown"),
    ("floatingislands", "Floating islands"),
    ("levski", "Levski"),
    ("battaglia", "Battaglia"),
    ("stormbreaker", "Stormbreaker"),
    ("kaboos", "Kaboos"),
    ("orbageddon", "Orbageddon"),
    ("apexgeddon", "Apexgeddon"),
    ("vanduulinc", "Vanduul incursion"),
    ("welcometonyx", "Nyx"),
    ("military", "Military sites"),
    ("criminal", "Criminal hideouts"),
];

/// A readable kind of place for a loot area (a slot preset path such as
/// `v3slotpreset_distributioncentres_highsecurtiy` or `caves/loot_caves_occupied_rock`):
/// `Distribution centres`, `Caves`. `None` for presets that name a loot theme, not a place
/// (`v3slotpreset_loot_generic`), and for test and event presets.
pub fn area_place_name(area: &str) -> Option<String> {
    let a = area.to_ascii_lowercase();
    if let Some(p) = starorg_gamedata::query::mined::area_place(&a) {
        return Some(p.to_string());
    }
    let words: Vec<&str> = a
        .split(['/', '_'])
        .filter(|w| {
            !w.is_empty()
                && !matches!(
                    *w,
                    "lootgeneration"
                        | "slotpreset"
                        | "multislotpreset"
                        | "v3slotpreset"
                        | "multiconfig"
                        | "harvestable"
                        | "loot"
                )
        })
        .collect();
    let has = |k: &str| words.iter().any(|w| w.starts_with(k));
    // A test preset has a word that is (or starts with) "test"; letters inside a word do
    // not count (`contestedzones` is a place).
    if words.first() == Some(&"special") || has("test") {
        return None;
    }
    AREA_PLACES
        .iter()
        .find(|(k, _)| has(k))
        .map(|(_, v)| (*v).to_string())
}

/// A container's display name, or a plain word for an internal class name
/// (`weapon_rack_1_volt_rifle_common_001` -> `Weapons rack`).
fn container_label(name: &str) -> String {
    if name.contains(' ') || !name.contains('_') {
        return name.to_string();
    }
    let n = name.to_ascii_lowercase();
    if n.contains("rack") {
        "Weapons rack".into()
    } else if n.contains("lootbox") {
        "Loot box".into()
    } else {
        "Container".into()
    }
}

/// `ShipWeapon` -> `ship weapon` (lower case, words split).
fn words_of(s: &str) -> String {
    spaced(s).to_lowercase()
}

/// What a spawn spot holds, from its `LootContents` tag (`Weapons` -> `Weapons crate`,
/// `Specific/WeaponsRack/...` -> `Weapons rack`), else the container's name; a size word
/// (`Large`) goes in front, a ship size (`S3`) stays apart.
fn contents_label(tags: &[String], container: &str) -> (String, Option<String>) {
    let after = |what: &str| {
        let p = format!("LootGeneration/{what}/");
        tags.iter()
            .find_map(|t| t.strip_prefix(&p).map(str::to_string))
    };
    let thing = match after("LootContents") {
        Some(c) => {
            let parts: Vec<&str> = c.split('/').collect();
            match parts.as_slice() {
                ["Specific", x, ..] => {
                    let w = words_of(x);
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                }
                ["Generic", ..] => "Crate".to_string(),
                ["Cargo", ..] => "Cargo box".to_string(),
                ["Loose", ..] => "Loose item".to_string(),
                [x, ..] => {
                    let w = words_of(x);
                    let mut c = w.chars();
                    let w = c
                        .next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default();
                    format!("{w} crate")
                }
                [] => container.to_string(),
            }
        }
        None => container.to_string(),
    };
    match after("LootSize") {
        Some(z) if z.starts_with('S') && z[1..].chars().all(|c| c.is_ascii_digit()) => {
            (thing, Some(z))
        }
        Some(z) => {
            let z = if z == "XLarge" { "XL".to_string() } else { z };
            let lower_first = {
                let mut c = thing.chars();
                c.next()
                    .map(|f| f.to_lowercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            };
            (format!("{z} {lower_first}"), None)
        }
        None => (thing, None),
    }
}

/// An area's best spawn as a loot hint: what the spot holds and its size, the rarity, the
/// kind of place and the odds per container.
pub fn loot_hint(a: &AreaSummary) -> LootHint {
    let c: LootContainer = crate::sources::loot_container(a);
    let place = area_place_name(&a.area);
    let (contents, size) = match c.kind.as_str() {
        "loose" => ("Lying loose".to_string(), None),
        _ => contents_label(&a.best.spawn_tags, &container_label(&c.container)),
    };
    LootHint {
        contents,
        size,
        rarity: c.rarity.filter(|r| !r.eq_ignore_ascii_case("reward")),
        place,
        system: None,
        odds: (c.odds_per_box > 0.0).then_some(c.odds_per_box),
    }
}

/// Places listed in one loot row before the rest are only counted.
pub const PLACES_PER_ROW: usize = 2;

/// Loot hints of every area, best first: one row per kind of container (contents, size,
/// rarity) with its best odds and its first [`PLACES_PER_ROW`] places joined by ` / `
/// (`Weapons rack (Common) at ASD facilities / Kaboos`; the line stays short, so further
/// places of a row are not listed). Returns the first `max` rows and how many there are.
pub fn loot_hints(areas: &[AreaSummary], max: usize) -> (Vec<LootHint>, usize) {
    let mut rows: Vec<(LootHint, Vec<String>)> = Vec::new();
    for a in areas {
        let h = loot_hint(a);
        let place = h.place.clone();
        let i = match rows
            .iter()
            .position(|(x, _)| x.contents == h.contents && x.size == h.size && x.rarity == h.rarity)
        {
            Some(i) => {
                if h.odds > rows[i].0.odds {
                    rows[i].0.odds = h.odds;
                }
                i
            }
            None => {
                rows.push((h, Vec::new()));
                rows.len() - 1
            }
        };
        if let Some(p) = place
            && !rows[i].1.contains(&p)
        {
            rows[i].1.push(p);
        }
    }
    let total = rows.len();
    let out = rows
        .into_iter()
        .take(max)
        .map(|(mut h, places)| {
            h.place = (!places.is_empty()).then(|| {
                places
                    .iter()
                    .take(PLACES_PER_ROW)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / ")
            });
            h
        })
        .collect();
    (out, total)
}

/// Shops that sell the item (a buy price above 0, on LIVE), cheapest first, names only.
pub fn shop_hints(shops: &[LocatedShop], max: usize) -> Vec<ShopHint> {
    let mut rows: Vec<(&LocatedShop, f64)> = shops
        .iter()
        .filter(|s| s.live != Some(false))
        .filter_map(|s| Some((s, s.row.price_buy.filter(|p| *p > 0.0)?)))
        .collect();
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out: Vec<ShopHint> = Vec::new();
    for (s, _) in rows {
        let place = s
            .location
            .split(',')
            .map(str::trim)
            .find(|p| !p.is_empty() && *p != s.terminal)
            .map(str::to_string);
        let h = ShopHint {
            terminal: s.terminal.clone(),
            place,
            system: s.system.clone(),
        };
        if !out.contains(&h) {
            out.push(h);
        }
        if out.len() == max {
            break;
        }
    }
    out
}

/// Kiosks that buy the commodity, best price first (as [`best_sells`] ranked them: only
/// kiosks that pay, none UEX marks off LIVE), with a short place.
///
/// [`best_sells`]: starorg_gamedata::query::sells::best_sells
pub fn sell_hints(rows: &[SellRow], max: usize) -> Vec<SellHint> {
    rows.iter()
        .take(max)
        .map(|r| SellHint {
            terminal: r.terminal.clone(),
            place: r
                .location
                .split(',')
                .map(str::trim)
                .find(|p| !p.is_empty() && *p != r.terminal)
                .map(str::to_string),
            system: r.system.clone(),
            price: r.price_sell,
        })
        .collect()
}

/// A real star system name for a mining location's `system` field, which is the location
/// key's stem (`Stanton2c`, `Aaronhalo`, `Lagrange`).
pub fn system_name(system: Option<&str>, location: &str) -> Option<String> {
    let s = system.unwrap_or(location).to_ascii_lowercase();
    ["Stanton", "Pyro", "Nyx"]
        .iter()
        .find(|n| s.starts_with(&n.to_ascii_lowercase()))
        .map(|n| (*n).to_string())
        .or_else(|| s.starts_with("aaronhalo").then(|| "Stanton".to_string()))
}

/// `KeegerBelt` -> `Keeger Belt`.
fn spaced(s: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if c == '_' {
            out.push(' ');
        } else {
            if c.is_ascii_uppercase() && prev.is_some_and(|p| p.is_ascii_lowercase()) {
                out.push(' ');
            }
            out.push(c);
        }
        prev = Some(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A readable place for a mining location: its body when the strings name one, else a
/// name from the location key (`Lagrange_B` -> `Lagrange points`, `Nyx_KeegerBelt` ->
/// `Keeger Belt`, `Stanton2c_Belt` -> `Yela Belt` given Yela's body name).
pub fn belt_place(
    location: &str,
    body: Option<&str>,
    body_of: impl Fn(&str) -> Option<String>,
) -> String {
    if let Some(b) = body.filter(|b| !b.trim().is_empty()) {
        return b.trim().to_string();
    }
    let lower = location.to_ascii_lowercase();
    if lower.starts_with("lagrange") {
        return "Lagrange points".into();
    }
    if lower.starts_with("asteroidcluster") {
        return "Asteroid clusters".into();
    }
    if let Some(stem) = location.strip_suffix("_Belt")
        && let Some(b) = body_of(stem)
    {
        return format!("{b} Belt");
    }
    let (sys, rest) = location.split_once('_').unwrap_or(("", location));
    let rest_lower = rest.to_ascii_lowercase();
    if rest_lower.starts_with("cool") || rest_lower.starts_with("warm") {
        return format!("{} asteroid fields", spaced(sys))
            .trim()
            .to_string();
    }
    let s = spaced(rest);
    if s.is_empty() { spaced(location) } else { s }
}

/// Mining places of one ore report: the belts and bodies that roll it (richest first), then
/// the hand-placed spots (caves, stations). `rock_kind` names how a spot's rock is mined.
pub fn mined_hints(
    r: &MinedReport,
    body_of: impl Fn(&str) -> Option<String>,
    rock_kind: impl Fn(&str) -> Option<String>,
) -> Vec<MinedHint> {
    let mut out: Vec<MinedHint> = r
        .belts
        .iter()
        .map(|b| MinedHint {
            place: belt_place(&b.location, b.body.as_deref(), &body_of),
            system: system_name(b.system.as_deref(), &b.location),
            kind: b.kind.clone(),
            min_pct: b.min_percent,
            max_pct: b.max_percent,
        })
        .collect();
    out.extend(r.spots.iter().map(|s| {
        MinedHint {
            place: s
                .place
                .clone()
                .or_else(|| area_place_name(&s.area.replace(": ", "/").replace(' ', "_")))
                .unwrap_or_else(|| s.area.clone()),
            system: None,
            kind: rock_kind(&s.rock),
            min_pct: None,
            max_pct: None,
        }
    }));
    merge_mined(out)
}

/// One row per place, system and kind (several rocks of one belt roll the ore), its
/// percent range widened to cover them all; first-seen order.
pub fn merge_mined(rows: Vec<MinedHint>) -> Vec<MinedHint> {
    let lo = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let hi = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, y) => x.or(y),
    };
    let mut out: Vec<MinedHint> = Vec::new();
    for m in rows {
        match out
            .iter_mut()
            .find(|o| o.place == m.place && o.system == m.system && o.kind == m.kind)
        {
            Some(o) => {
                o.min_pct = lo(o.min_pct, m.min_pct);
                o.max_pct = hi(o.max_pct, m.max_pct);
            }
            None => out.push(m),
        }
    }
    out
}

/// The quality spread, when the resource has a full distribution.
pub fn quality_hint(q: &OreQuality) -> Option<QualityHint> {
    let r = &q.range;
    Some(QualityHint {
        min: r.min?,
        max: r.max?,
        mean: r.mean?,
        stddev: r.stddev,
        overrides: q
            .overrides
            .iter()
            .filter_map(|o| {
                Some(QualityOverride {
                    place: o.location.clone(),
                    min: o.range.min?,
                    max: o.range.max?,
                })
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belt_places_read_like_places() {
        let none = |_: &str| None;
        assert_eq!(belt_place("Stanton1", Some("Hurston"), none), "Hurston");
        assert_eq!(belt_place("Lagrange_B", None, none), "Lagrange points");
        assert_eq!(belt_place("Nyx_KeegerBelt", None, none), "Keeger Belt");
        assert_eq!(belt_place("Nyx_GlaciemRing", None, none), "Glaciem Ring");
        assert_eq!(
            belt_place("Pyro_DeepSpaceAsteroids", None, none),
            "Deep Space Asteroids"
        );
        assert_eq!(
            belt_place("Pyro_Cool01", None, none),
            "Pyro asteroid fields"
        );
        assert_eq!(
            belt_place("AsteroidCluster_Low_Yield", None, none),
            "Asteroid clusters"
        );
        let yela = |s: &str| (s == "Stanton2c").then(|| "Yela".to_string());
        assert_eq!(belt_place("Stanton2c_Belt", None, yela), "Yela Belt");
        assert_eq!(
            system_name(Some("Stanton2c"), "x").as_deref(),
            Some("Stanton")
        );
        assert_eq!(
            system_name(Some("Aaronhalo"), "x").as_deref(),
            Some("Stanton")
        );
        assert_eq!(system_name(Some("Lagrange"), "Lagrange_A"), None);
    }

    #[test]
    fn sell_hints_keep_order_price_and_a_short_place() {
        let row = |t: &str, loc: &str, p: f64| SellRow {
            terminal: t.into(),
            location: loc.into(),
            system: Some("Stanton".into()),
            price_sell: p,
            scu_stock: None,
            updated: None,
            stale: false,
        };
        let rows = [
            row("HUR-L2", "HUR-L2 Faithful Dream Station, Stanton", 9612.0),
            row("TDD Area 18", "Area 18, ArcCorp, Stanton", 9000.0),
            row("Ruin Station", "Ruin Station, Pyro", 8700.0),
        ];
        let h = sell_hints(&rows, 2);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].terminal, "HUR-L2");
        assert_eq!(h[0].place.as_deref(), Some("HUR-L2 Faithful Dream Station"));
        assert_eq!(h[1].place.as_deref(), Some("Area 18"));
        assert_eq!(h[1].price, 9000.0);
        // A place that is the terminal's own name is skipped.
        assert_eq!(sell_hints(&rows[2..], 5)[0].place.as_deref(), Some("Pyro"));
    }

    #[test]
    fn area_and_contents_labels() {
        assert_eq!(
            area_place_name("v3slotpreset_distributioncentres_highsecurtiy").as_deref(),
            Some("Distribution centres")
        );
        assert_eq!(
            area_place_name("caves/loot_caves_occupied_rock").as_deref(),
            Some("Caves")
        );
        assert_eq!(
            area_place_name("v3slotpreset_rockcracker").as_deref(),
            Some("Rock Cracker, Nyx")
        );
        assert_eq!(area_place_name("v3slotpreset_loot_generic"), None);
        assert_eq!(area_place_name("cave_full_test"), None);
        assert_eq!(area_place_name("special_holidaybox"), None);
        // `contestedzones` holds the letters of "test" and is no test preset (LIVE 4.10:
        // these five areas had no place).
        for a in [
            "contestedzones/contestedzone_1a",
            "contestedzones/contestedzone_1a_rewardroom_001",
            "contestedzones/contestedzone_1b_rewardroom_002",
            "contestedzones/ghostarena_01",
            "v3slotpreset_contestedzones",
        ] {
            assert_eq!(
                area_place_name(a).as_deref(),
                Some("Contested zones"),
                "{a}"
            );
        }
        assert_eq!(area_place_name("testarea_loot_caves"), None);
        let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            contents_label(
                &t(&[
                    "LootGeneration/LootContents/Weapons",
                    "LootGeneration/LootSize/Large"
                ]),
                "Standard Container"
            ),
            ("Large weapons crate".to_string(), None)
        );
        assert_eq!(
            contents_label(
                &t(&[
                    "LootGeneration/LootContents/ShipWeapon",
                    "LootGeneration/LootSize/S3"
                ]),
                "x"
            ),
            ("Ship weapon crate".to_string(), Some("S3".to_string()))
        );
        assert_eq!(
            contents_label(
                &t(&["LootGeneration/LootContents/Specific/WeaponsRack/WeaponsRack_LMG"]),
                "x"
            ),
            ("Weapons rack".to_string(), None)
        );
        assert_eq!(
            contents_label(&[], "Large Container"),
            ("Large Container".to_string(), None)
        );
        assert_eq!(
            container_label("weapon_rack_1_volt_rifle_common_001"),
            "Weapons rack"
        );
        assert_eq!(container_label("Stash"), "Stash");
    }

    #[test]
    fn loot_rows_group_places_per_container_kind() {
        use starorg_gamedata::query::obtain::{SpawnHit, SpawnKind};
        let area = |name: &str, score: f64, contents: &str| AreaSummary {
            area: name.into(),
            best: SpawnHit {
                area: name.into(),
                config: Some("Weapons Large Rare".into()),
                spawn_tags: vec![
                    format!("LootGeneration/LootContents/{contents}"),
                    "LootGeneration/LootSize/Large".into(),
                ],
                entity: "crate".into(),
                entity_name: Some("Standard Container".into()),
                slot_share: 1.0,
                kind: SpawnKind::Loot,
                table: None,
                per_roll: None,
                max_results: None,
                score,
                paths: None,
            },
            configs: 1,
        };
        let areas = vec![
            area("caves/loot_caves_occupied", 0.3, "Weapons"),
            area("v3slotpreset_loot_generic", 0.25, "Weapons"),
            area("lootgeneration_slotpreset_stations", 0.2, "Weapons"),
            area("caves/loot_caves_unoccupied", 0.1, "Weapons"),
            area("lootgeneration_slotpreset_kareah", 0.1, "Weapons"),
            area("lootgeneration_slotpreset_junksites", 0.1, "Weapons"),
            area("lootgeneration_slotpreset_ugfs", 0.05, "Armour"),
        ];
        let (rows, total) = loot_hints(&areas, 12);
        assert_eq!(total, 2);
        assert_eq!(rows[0].contents, "Large weapons crate");
        assert_eq!(rows[0].rarity.as_deref(), Some("Rare"));
        assert_eq!(rows[0].place.as_deref(), Some("Caves / Stations"));
        assert_eq!(rows[0].odds, Some(0.3));
        assert_eq!(rows[1].place.as_deref(), Some("Underground facilities"));
        let (one, total) = loot_hints(&areas, 1);
        assert_eq!((one.len(), total), (1, 2));
    }
}
