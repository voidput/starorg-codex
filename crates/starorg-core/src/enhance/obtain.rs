//! Obtain hints (spec obtain-hints): a WHERE TO GET block after item and commodity
//! descriptions, from the neutral [`ObtainHints`] model (loot, shops, mining, ore quality)
//! and from `GameData` itself (craft: a blueprint produces the item; rewards: contracts that
//! award the item or its blueprint).
//!
//! The block goes after everything else on the key: when a stat block (or a quest hint)
//! already owns it, the Engine extends that edit ([`super::Category::extends`]). Lines
//! (`lines_off = ["obtain.loot", ...]`): `shops`, `loot`, `craft`, `rewards`, `mined`,
//! `quality`. A line with nothing to say is left out; nothing at all means no block.
//!
//! Text from the hints is data, not markup: EM tags, `~` variables, `%` and control
//! characters are taken out before it is placed, so a place name cannot break the checks.

use super::profile::{ObtainMode, Profile, em};
use super::{Category, Claim, Engine, Payload, format};
use crate::ini::{IniError, StockIni};
use crate::model::{
    Blueprint, GameData, ItemObtain, LootHint, MinedHint, ObtainHints, QualityHint, ShopHint,
    loc_key,
};
use std::collections::BTreeMap;

/// The `lines_off` category of this block.
pub const LINE_CATEGORY: &str = "obtain";
/// Line names, in block order.
pub const LINES: &[&str] = &["shops", "loot", "craft", "rewards", "mined", "quality"];
/// Places listed per line before `+N`.
pub const TOP: usize = 3;
/// The block header (Normal and Detailed).
pub const HEADER: &str = "WHERE TO GET";

/// A contract that awards the item (`blueprint`: it awards the item's blueprint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reward {
    pub title: String,
    pub blueprint: bool,
}

/// What `GameData` says about one class: its blueprints and the contracts awarding it.
#[derive(Debug, Clone, Default)]
pub struct Crafted<'a> {
    pub blueprints: Vec<&'a Blueprint>,
    pub rewards: Vec<Reward>,
}

/// Blueprints by produced class and rewarding contracts by class, lower case.
pub struct Index<'a> {
    blueprints: BTreeMap<String, Vec<&'a Blueprint>>,
    rewards: BTreeMap<String, BTreeMap<String, bool>>,
}

impl<'a> Index<'a> {
    pub fn new(gd: &'a GameData, stock: &StockIni) -> Self {
        let mut blueprints: BTreeMap<String, Vec<&Blueprint>> = BTreeMap::new();
        let mut by_short: BTreeMap<String, &Blueprint> = BTreeMap::new();
        for b in &gd.blueprints {
            by_short.insert(b.short_name().to_ascii_lowercase(), b);
            if let Some(p) = b.produces.as_deref() {
                blueprints
                    .entry(p.to_ascii_lowercase())
                    .or_default()
                    .push(b);
            }
        }
        let pools: BTreeMap<&str, _> = gd
            .blueprint_pools
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();
        // Title -> blueprint only (false once any contract with that title awards the item).
        let mut rewards: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
        let mut add = |class: &str, title: &str, bp: bool| {
            let e = rewards
                .entry(class.to_ascii_lowercase())
                .or_default()
                .entry(title.to_string())
                .or_insert(bp);
            *e &= bp;
        };
        for c in &gd.contracts {
            if c.not_for_release || c.work_in_progress {
                continue;
            }
            let Some(title) = super::quest::plain_title(stock, c.title_key.as_deref()) else {
                continue;
            };
            let title = clean(&title);
            if title.is_empty() {
                continue;
            }
            for r in &c.item_rewards {
                add(&r.item, &title, false);
            }
            for r in &c.blueprint_rewards {
                let Some(pool) = pools.get(r.pool.as_str()) else {
                    continue;
                };
                for e in &pool.blueprints {
                    let produced = by_short
                        .get(&e.blueprint.to_ascii_lowercase())
                        .and_then(|b| b.produces.as_deref());
                    if let Some(p) = produced {
                        add(p, &title, true);
                    }
                }
            }
        }
        Self {
            blueprints,
            rewards,
        }
    }

    pub fn crafted(&self, class: &str) -> Crafted<'a> {
        let k = class.to_ascii_lowercase();
        Crafted {
            blueprints: self.blueprints.get(&k).cloned().unwrap_or_default(),
            rewards: self
                .rewards
                .get(&k)
                .map(|m| {
                    m.iter()
                        .map(|(t, bp)| Reward {
                            title: t.clone(),
                            blueprint: *bp,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Hint text as plain text: no EM tags, variables, `%`, escapes or control characters;
/// whitespace collapsed.
pub fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || "<>~%\\".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(clean).filter(|x| !x.is_empty())
}

/// `name (a, b)`, or `name` when there is nothing to put in brackets.
fn with_paren(name: String, inner: Vec<String>) -> String {
    if inner.is_empty() {
        name
    } else {
        format!("{name} ({})", inner.join(", "))
    }
}

/// Distinct values, in first-seen order.
fn distinct(items: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in items {
        if !i.is_empty() && !out.contains(&i) {
            out.push(i);
        }
    }
    out
}

/// The first [`TOP`] of `items` joined with `sep`, then `+N` for the rest and `extra`.
fn top(items: &[String], extra: usize, sep: &str) -> String {
    let mut s = items
        .iter()
        .take(TOP)
        .cloned()
        .collect::<Vec<_>>()
        .join(sep);
    let more = items.len().saturating_sub(TOP) + extra;
    if more > 0 {
        let gap = if sep == ", " { " " } else { sep };
        s.push_str(&format!("{gap}+{more}"));
    }
    s
}

/// A percent the checks accept anywhere a line or `  |  ` follows: `21%`, `0.5%`.
fn odds(fraction: f64) -> String {
    let v = fraction * 100.0;
    if v < 10.0 {
        format!("{}%", format::d1(v))
    } else {
        format::pct(v)
    }
}

/// `12–30%`, `30%`.
fn pct_range(lo: Option<f64>, hi: Option<f64>) -> Option<String> {
    let f = |v: f64| format::d1(v);
    match (format::nz(lo), format::nz(hi)) {
        (Some(a), Some(b)) if f(a) != f(b) => Some(format!("{}–{}%", f(a), f(b))),
        (Some(a), _) | (None, Some(a)) => Some(format!("{}%", f(a))),
        (None, None) => None,
    }
}

/// `1m 30s`, `45s`, `2h 5m`.
fn duration(secs: f64) -> String {
    let s = secs.round().max(0.0) as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    match (h, m, sec) {
        (0, 0, s) => format!("{s}s"),
        (0, m, 0) => format!("{m}m"),
        (0, m, s) => format!("{m}m {s}s"),
        (h, 0, _) => format!("{h}h"),
        (h, m, _) => format!("{h}h {m}m"),
    }
}

fn shop(s: &ShopHint, detailed: bool) -> String {
    let mut inner: Vec<String> = opt(&s.place).into_iter().collect();
    if detailed {
        inner.extend(opt(&s.system));
    }
    with_paren(clean(&s.terminal), inner)
}

fn loot(l: &LootHint, detailed: bool) -> String {
    let name = [Some(clean(&l.contents)), opt(&l.size)]
        .into_iter()
        .flatten()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut s = with_paren(name, opt(&l.rarity).into_iter().collect());
    let mut at: Vec<String> = opt(&l.place).into_iter().collect();
    if detailed {
        at.extend(opt(&l.system));
    }
    if !at.is_empty() {
        s = format!("{s} at {}", at.join(", "));
    }
    if detailed && let Some(o) = l.odds.filter(|o| o.is_finite() && *o > 0.0) {
        s = format!("{s} · {}", odds(o.min(1.0)));
    }
    s
}

fn kinds(mined: &[MinedHint]) -> Option<String> {
    let k = distinct(mined.iter().filter_map(|m| opt(&m.kind)));
    (!k.is_empty()).then(|| k.join("/"))
}

fn mined_place(m: &MinedHint, detailed: bool) -> String {
    let place = clean(&m.place);
    if !detailed {
        return place;
    }
    let mut s = match opt(&m.system) {
        Some(sys) if sys != place => format!("{place}, {sys}"),
        _ => place,
    };
    if let Some(k) = opt(&m.kind) {
        s = format!("{s} ({k})");
    }
    if let Some(r) = pct_range(m.min_pct, m.max_pct) {
        s = format!("{s} {r}");
    }
    s
}

fn quality(q: &QualityHint, detailed: bool) -> String {
    let mut s = format!(
        "{}–{}, avg {}",
        format::int(q.min),
        format::int(q.max),
        format::int(q.mean)
    );
    if !detailed {
        return s;
    }
    if let Some(sd) = format::pos(q.stddev) {
        s.push_str(&format!(" ± {}", format::int(sd)));
    }
    let over: Vec<String> = q
        .overrides
        .iter()
        .map(|o| {
            format!(
                "{} {}–{}",
                clean(&o.place),
                format::int(o.min),
                format::int(o.max)
            )
        })
        .filter(|x| !x.starts_with(' '))
        .collect();
    if !over.is_empty() {
        s.push_str(&format!("; {}", top(&over, 0, ", ")));
    }
    s
}

/// The block for one class, `None` when no line has anything to say. Starts at the header
/// (Normal, Detailed) or the `Get:` line (Short); the caller puts it after the stock text.
pub fn block(info: &ItemObtain, crafted: &Crafted, p: &Profile) -> Option<String> {
    let on = |line: &str| p.line_on(LINE_CATEGORY, line);
    let detailed = p.obtain_mode == ObtainMode::Detailed;
    let label = |t: &str| em(p.mission.label_em, &format!("{t}:"));
    let shops: Vec<String> = if p.obtain_shops && on("shops") {
        distinct(info.shops.iter().map(|s| shop(s, detailed)))
    } else {
        Vec::new()
    };
    let loot_all: Vec<String> = if on("loot") {
        distinct(info.loot.iter().map(|l| loot(l, detailed)))
    } else {
        Vec::new()
    };
    let loot_extra = if on("loot") {
        info.loot_count().saturating_sub(info.loot.len())
    } else {
        0
    };
    let craft = on("craft") && !crafted.blueprints.is_empty();
    let rewards: Vec<String> = if on("rewards") {
        crafted
            .rewards
            .iter()
            .map(|r| {
                if r.blueprint {
                    format!("{} [BP]", r.title)
                } else {
                    r.title.clone()
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mined: &[MinedHint] = if on("mined") { &info.mined } else { &[] };
    let places = distinct(mined.iter().map(|m| mined_place(m, false)));
    let quality_hint = info.quality.as_ref().filter(|_| on("quality"));

    if p.obtain_mode == ObtainMode::Short {
        let mut parts: Vec<String> = Vec::new();
        if !shops.is_empty() {
            parts.push("Buy".into());
        }
        if !loot_all.is_empty() || loot_extra > 0 {
            let rarity = info.loot.iter().find_map(|l| opt(&l.rarity));
            parts.push(with_paren("Loot".into(), rarity.into_iter().collect()));
        }
        if craft {
            parts.push("Craft [BP]".into());
        }
        if !rewards.is_empty() {
            parts.push("Reward".into());
        }
        if !places.is_empty() {
            let head = with_paren("Mined".into(), kinds(mined).into_iter().collect());
            parts.push(format!("{head} at {}", top(&places, 0, ", ")));
        }
        if let Some(q) = quality_hint {
            parts.push(format!("Quality avg {}", format::int(q.mean)));
        }
        if parts.is_empty() {
            return None;
        }
        return Some(format!("{} {}", label("Get"), parts.join(" · ")));
    }

    let mut lines: Vec<String> = Vec::new();
    let list_sep = if detailed { format::SEP } else { ", " };
    if !shops.is_empty() {
        lines.push(format!("{} {}", label("Buy"), top(&shops, 0, list_sep)));
    }
    if !loot_all.is_empty() {
        lines.push(format!(
            "{} {}",
            label("Loot"),
            top(&loot_all, loot_extra, list_sep)
        ));
    } else if loot_extra > 0 {
        lines.push(format!("{} {loot_extra} containers", label("Loot")));
    }
    if craft {
        let n = crafted.blueprints.len();
        let mut s = if n == 1 {
            "from blueprint".to_string()
        } else {
            format!("from {n} blueprints")
        };
        if detailed
            && let Some(t) = crafted
                .blueprints
                .iter()
                .find_map(|b| format::pos(b.tiers.first()?.craft_time_seconds))
        {
            s.push_str(&format!(", {}", duration(t)));
        }
        lines.push(format!("{} {s}", label("Craft")));
    }
    if !rewards.is_empty() {
        lines.push(format!("{} {}", label("Reward"), top(&rewards, 0, ", ")));
    }
    if !places.is_empty() {
        let s = if detailed {
            let all = distinct(mined.iter().map(|m| mined_place(m, true)));
            top(&all, 0, list_sep)
        } else {
            let mut s = top(&places, 0, ", ");
            if let Some(k) = kinds(mined) {
                s.push_str(&format!(" ({k})"));
            }
            s
        };
        lines.push(format!("{} {s}", label("Mined")));
    }
    if let Some(q) = quality_hint {
        lines.push(format!("{} {}", label("Quality"), quality(q, detailed)));
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "{}\\n{}",
        em(p.mission.header_em, HEADER),
        lines.join("\\n")
    ))
}

/// Offer a block for every class the hints give a description key for (and that key is in
/// stock); the namesake rule decides shared keys, and an owned key is extended.
pub fn generate(en: &mut Engine, gd: &GameData, hints: &ObtainHints) {
    let stock = en.stock;
    let p = en.profile;
    let idx = Index::new(gd, stock);
    let empty = ItemObtain::default();
    let claims: Vec<Claim> = hints
        .desc
        .iter()
        .filter_map(|(class, key)| {
            let k = loc_key(key)?;
            if matches!(stock.find(k), Err(e) if !matches!(e, IniError::AmbiguousKey { .. })) {
                return None;
            }
            let class = class.to_ascii_lowercase();
            let info = hints.items.get(&class).unwrap_or(&empty);
            let text = block(info, &idx.crafted(&class), p)?;
            Some(Claim {
                record: class,
                key: key.clone(),
                payload: Some(Payload::Append(format!("\\n\\n{text}"))),
                placeholder: false,
                player: true,
            })
        })
        .collect();
    en.assign(Category::ObtainHints, claims);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enhance::{Reason, check, generate_with, owned::NoneOwned};
    use crate::model::{
        BlueprintPool, BlueprintPoolEntry, BlueprintReward, BlueprintTier, Contract, ItemReward,
        QualityOverride,
    };

    fn gun() -> ItemObtain {
        let l = |c: &str, r: &str, place: &str, o: f64| LootHint {
            contents: c.into(),
            size: None,
            rarity: Some(r.into()),
            place: Some(place.into()),
            system: Some("Stanton".into()),
            odds: Some(o),
        };
        ItemObtain {
            shops: vec![
                ShopHint {
                    terminal: "Center Mass".into(),
                    place: Some("Area18".into()),
                    system: Some("Stanton".into()),
                },
                ShopHint {
                    terminal: "Live Fire Weapons".into(),
                    place: Some("Port Olisar".into()),
                    system: None,
                },
            ],
            loot: vec![
                l("Weapon crate", "Rare", "Bunkers", 0.21),
                l("Ammo box", "Rare", "Derelicts", 0.05),
                l("Locker", "Uncommon", "Outposts", 0.02),
                l("Footlocker", "Common", "Caves", 0.01),
            ],
            loot_total: 6,
            ..Default::default()
        }
    }

    fn ore() -> ItemObtain {
        let m = |place: &str, sys: &str, lo: f64, hi: f64| MinedHint {
            place: place.into(),
            system: Some(sys.into()),
            kind: Some("hand".into()),
            min_pct: Some(lo),
            max_pct: Some(hi),
        };
        ItemObtain {
            mined: vec![
                m("Lyria", "Stanton", 12.0, 30.0),
                m("Pyro I", "Pyro", 5.0, 5.0),
                m("Aberdeen", "Stanton", 1.0, 8.0),
                m("Wala", "Stanton", 2.0, 4.0),
            ],
            quality: Some(QualityHint {
                min: 1.0,
                max: 900.0,
                mean: 500.0,
                stddev: Some(120.0),
                overrides: vec![QualityOverride {
                    place: "Lyria".into(),
                    min: 600.0,
                    max: 900.0,
                }],
            }),
            ..Default::default()
        }
    }

    fn bp() -> Blueprint {
        Blueprint {
            record_name: "CraftingBlueprintRecord.BP_CRAFT_gun".into(),
            produces: Some("BEHR_Rifle_01".into()),
            tiers: vec![BlueprintTier {
                craft_time_seconds: Some(90.0),
            }],
            ..Default::default()
        }
    }

    fn gd() -> GameData {
        GameData {
            blueprints: vec![bp()],
            blueprint_pools: vec![BlueprintPool {
                name: "POOL_A".into(),
                blueprints: vec![BlueprintPoolEntry {
                    blueprint: "BP_CRAFT_gun".into(),
                    weight: Some(1.0),
                }],
            }],
            contracts: vec![
                Contract {
                    generator: "G".into(),
                    debug_name: "c1".into(),
                    title_key: Some("@t_bunker".into()),
                    blueprint_rewards: vec![BlueprintReward {
                        pool: "POOL_A".into(),
                        chance: None,
                    }],
                    ..Default::default()
                },
                Contract {
                    generator: "G".into(),
                    debug_name: "c2".into(),
                    title_key: Some("@t_haul".into()),
                    item_rewards: vec![ItemReward {
                        item: "behr_rifle_01".into(),
                        name_key: None,
                        amount: 1.0,
                    }],
                    ..Default::default()
                },
                Contract {
                    generator: "G".into(),
                    debug_name: "c3".into(),
                    title_key: Some("@t_wip".into()),
                    work_in_progress: true,
                    item_rewards: vec![ItemReward {
                        item: "behr_rifle_01".into(),
                        name_key: None,
                        amount: 1.0,
                    }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn stock_text() -> String {
        "\u{feff}item_Descbehr_rifle_01=A rifle.\\n\r\nitem_Descagri=Agricium ore.\r\nt_bunker=Bunker Clearing\r\nt_haul=Cargo Run\r\nt_wip=Never\r\nitem_DescOther=Other.\r\n".into()
    }

    fn stock() -> StockIni {
        StockIni::parse(stock_text().into_bytes())
    }

    fn profile(mode: ObtainMode) -> Profile {
        Profile {
            obtain_mode: mode,
            obtain_shops: true,
            ..Profile::default()
        }
    }

    fn crafted<'a>(gd: &'a GameData, s: &StockIni) -> Crafted<'a> {
        Index::new(gd, s).crafted("behr_rifle_01")
    }

    #[test]
    fn gun_text_per_mode() {
        let (g, s) = (gd(), stock());
        let c = crafted(&g, &s);
        assert_eq!(c.blueprints.len(), 1);
        assert_eq!(
            c.rewards,
            vec![
                Reward {
                    title: "Bunker Clearing".into(),
                    blueprint: true
                },
                Reward {
                    title: "Cargo Run".into(),
                    blueprint: false
                }
            ]
        );
        assert_eq!(
            block(&gun(), &c, &profile(ObtainMode::Short)).as_deref(),
            Some("<EM4>Get:</EM4> Buy · Loot (Rare) · Craft [BP] · Reward")
        );
        assert_eq!(
            block(&gun(), &c, &profile(ObtainMode::Normal)).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Buy:</EM4> Center Mass (Area18), Live Fire Weapons (Port Olisar)\\n\
                 <EM4>Loot:</EM4> Weapon crate (Rare) at Bunkers, Ammo box (Rare) at Derelicts, Locker (Uncommon) at Outposts +3\\n\
                 <EM4>Craft:</EM4> from blueprint\\n\
                 <EM4>Reward:</EM4> Bunker Clearing [BP], Cargo Run"
            )
        );
        assert_eq!(
            block(&gun(), &c, &profile(ObtainMode::Detailed)).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Buy:</EM4> Center Mass (Area18, Stanton)  |  Live Fire Weapons (Port Olisar)\\n\
                 <EM4>Loot:</EM4> Weapon crate (Rare) at Bunkers, Stanton · 21%  |  Ammo box (Rare) at Derelicts, Stanton · 5%  |  Locker (Uncommon) at Outposts, Stanton · 2%  |  +3\\n\
                 <EM4>Craft:</EM4> from blueprint, 1m 30s\\n\
                 <EM4>Reward:</EM4> Bunker Clearing [BP], Cargo Run"
            )
        );
        // Every mode passes the generated-value checks after a stock description.
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            let b = block(&gun(), &c, &profile(mode)).unwrap();
            let suffix = format!("\\n\\n{b}");
            assert_eq!(
                check::check_generated("A rifle.", "A rifle.", "", &suffix),
                Ok(()),
                "{suffix}"
            );
        }
    }

    #[test]
    fn ore_text_per_mode() {
        let none = Crafted::default();
        assert_eq!(
            block(&ore(), &none, &profile(ObtainMode::Short)).as_deref(),
            Some("<EM4>Get:</EM4> Mined (hand) at Lyria, Pyro I, Aberdeen +1 · Quality avg 500")
        );
        assert_eq!(
            block(&ore(), &none, &profile(ObtainMode::Normal)).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Mined:</EM4> Lyria, Pyro I, Aberdeen +1 (hand)\\n\
                 <EM4>Quality:</EM4> 1–900, avg 500"
            )
        );
        assert_eq!(
            block(&ore(), &none, &profile(ObtainMode::Detailed)).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Mined:</EM4> Lyria, Stanton (hand) 12–30%  |  Pyro I, Pyro (hand) 5%  |  Aberdeen, Stanton (hand) 1–8%  |  +1\\n\
                 <EM4>Quality:</EM4> 1–900, avg 500 ± 120; Lyria 600–900"
            )
        );
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            let b = block(&ore(), &none, &profile(mode)).unwrap();
            let suffix = format!("\\n\\n{b}");
            assert_eq!(
                check::check_generated("Ore.", "Ore.", "", &suffix),
                Ok(()),
                "{suffix}"
            );
        }
    }

    #[test]
    fn lines_toggle_and_shops_need_their_switch() {
        let (g, s) = (gd(), stock());
        let c = crafted(&g, &s);
        let mut p = profile(ObtainMode::Normal);
        p.obtain_shops = false;
        for l in ["loot", "craft"] {
            p.lines_off.insert(format!("obtain.{l}"));
        }
        assert_eq!(
            block(&gun(), &c, &p).as_deref(),
            Some("<EM3>WHERE TO GET</EM3>\\n<EM4>Reward:</EM4> Bunker Clearing [BP], Cargo Run")
        );
        for l in LINES {
            p.lines_off.insert(format!("obtain.{l}"));
        }
        p.obtain_shops = true;
        assert_eq!(block(&gun(), &c, &p), None);
        assert_eq!(block(&ore(), &c, &p), None);
        // Colours off: plain labels.
        let mut p = profile(ObtainMode::Short);
        p.mission.label_em = 0;
        assert_eq!(
            block(&ore(), &Crafted::default(), &p).as_deref(),
            Some("Get: Mined (hand) at Lyria, Pyro I, Aberdeen +1 · Quality avg 500")
        );
    }

    #[test]
    fn empty_sources_give_no_block() {
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            assert_eq!(
                block(&ItemObtain::default(), &Crafted::default(), &profile(mode)),
                None
            );
        }
        // Only a container count: still a source.
        let only_count = ItemObtain {
            loot_total: 4,
            ..Default::default()
        };
        assert_eq!(
            block(
                &only_count,
                &Crafted::default(),
                &profile(ObtainMode::Normal)
            )
            .as_deref(),
            Some("<EM3>WHERE TO GET</EM3>\\n<EM4>Loot:</EM4> 4 containers")
        );
    }

    #[test]
    fn hint_text_is_made_plain() {
        assert_eq!(
            clean("  <EM4>Area 18</EM4>\t~x 50%\\n "),
            "EM4 Area 18 /EM4 x 50 n"
        );
        let info = ItemObtain {
            shops: vec![ShopHint {
                terminal: "Shop <EM4>%s".into(),
                place: Some(" ".into()),
                system: None,
            }],
            ..Default::default()
        };
        let b = block(&info, &Crafted::default(), &profile(ObtainMode::Normal)).unwrap();
        assert_eq!(
            check::check_generated("x", "x", "", &format!("\\n\\n{b}")),
            Ok(())
        );
        assert!(b.ends_with("<EM4>Buy:</EM4> Shop EM4 s"), "{b}");
    }

    #[test]
    fn formats() {
        assert_eq!(duration(90.0), "1m 30s");
        assert_eq!(duration(45.0), "45s");
        assert_eq!(duration(120.0), "2m");
        assert_eq!(duration(7500.0), "2h 5m");
        assert_eq!(odds(0.005), "0.5%");
        assert_eq!(odds(0.214), "21%");
        assert_eq!(pct_range(Some(5.0), Some(5.0)).as_deref(), Some("5%"));
        assert_eq!(pct_range(None, None), None);
    }

    fn hints() -> ObtainHints {
        ObtainHints {
            items: BTreeMap::from([
                ("behr_rifle_01".to_string(), gun()),
                ("agri".to_string(), ore()),
                ("other".to_string(), ItemObtain::default()),
            ]),
            desc: BTreeMap::from([
                (
                    "behr_rifle_01".to_string(),
                    "@item_Descbehr_rifle_01".to_string(),
                ),
                ("agri".to_string(), "@item_Descagri".to_string()),
                ("other".to_string(), "@item_DescOther".to_string()),
                ("missing".to_string(), "@item_DescMissing".to_string()),
            ]),
        }
    }

    #[test]
    fn generate_writes_blocks_through_the_engine() {
        let (g, s) = (gd(), stock());
        let p = Profile::default();
        let out = generate_with(&g, &s, &p, &NoneOwned, None, Some(&hints()));
        assert_eq!(out.count(Category::ObtainHints), 2);
        let cov = out.coverage.get(Category::ObtainHints);
        // `other` has nothing to say, `missing` is not in stock: neither is a candidate.
        assert_eq!((cov.candidates, cov.written), (2, 2));
        let v = |key: &str| {
            let line = s.find(key).unwrap();
            out.edits[&line].value(s.value_at(line).unwrap())
        };
        assert!(
            v("item_Descbehr_rifle_01").starts_with("A rifle.\\n\\n<EM3>WHERE TO GET</EM3>\\n"),
            "{}",
            v("item_Descbehr_rifle_01")
        );
        assert!(v("item_Descagri").contains("<EM4>Mined:</EM4> Lyria"));
        // Off, or no hints: nothing.
        let off = Profile {
            obtain: false,
            ..Profile::default()
        };
        let none = generate_with(&g, &s, &off, &NoneOwned, None, Some(&hints()));
        assert_eq!(none.count(Category::ObtainHints), 0);
        let none = generate_with(&g, &s, &p, &NoneOwned, None, None);
        assert_eq!(none.count(Category::ObtainHints), 0);
    }

    #[test]
    fn extends_a_stat_block_after_its_text() {
        let s = stock();
        let key = "item_Descbehr_rifle_01";
        let obtain = Payload::Append("\\n\\n<EM3>WHERE TO GET</EM3>\\nGet: Loot".into());
        for above in [false, true] {
            let p = Profile {
                stats_above: above,
                ..Profile::default()
            };
            let mut en = Engine::new(&s, &p);
            let stats = Payload::Block(vec!["Damage: 10".into()]);
            assert!(en.offer(Category::FpsWeapons, key, Some("r".into()), &stats));
            assert!(en.offer(Category::ObtainHints, key, Some("r".into()), &obtain));
            let line = s.find(key).unwrap();
            let e = &en.out.edits[&line];
            assert_eq!(e.category, Category::FpsWeapons);
            assert_eq!(e.extended_by, vec![Category::ObtainHints]);
            let want = if above {
                "--- STATS ---\\nDamage: 10\\n\\n------\\n\\nA rifle.\\n\\n<EM3>WHERE TO GET</EM3>\\nGet: Loot"
            } else {
                "A rifle.\\n\\n--- STATS ---\\nDamage: 10\\n\\n<EM3>WHERE TO GET</EM3>\\nGet: Loot"
            };
            assert_eq!(e.value(s.value_at(line).unwrap()), want);
            assert_eq!(en.out.count(Category::FpsWeapons), 1);
            assert_eq!(en.out.count(Category::ObtainHints), 1);
            assert_eq!(en.out.coverage.get(Category::ObtainHints).written, 1);
            // Once per category: a second extension is owned-by-other.
            assert!(!en.offer(Category::ObtainHints, key, None, &obtain));
            // Other categories keep the one-edit rule.
            assert!(!en.offer(
                Category::QuestHints,
                key,
                None,
                &Payload::Append("\\nX".into())
            ));
            assert_eq!(
                en.out
                    .coverage
                    .get(Category::QuestHints)
                    .left_for(Reason::OwnedByOther),
                1
            );
        }
    }

    #[test]
    fn failing_extension_drops_only_itself() {
        let s = stock();
        let p = Profile::default();
        let key = "item_Descbehr_rifle_01";
        let line = s.find(key).unwrap();
        let mut en = Engine::new(&s, &p);
        let stats = Payload::Block(vec!["Damage: 10".into()]);
        assert!(en.offer(Category::FpsWeapons, key, None, &stats));
        let before = en.out.edits[&line].clone();
        for bad in [
            "\\nA: 25% off".to_string(),
            "\\n<EM4>open".to_string(),
            format!("\\n{}", "x".repeat(check::MAX_LEN)),
        ] {
            assert!(!en.offer(Category::ObtainHints, key, None, &Payload::Append(bad)));
            assert_eq!(en.out.edits[&line], before);
        }
        let cov = en.out.coverage.get(Category::ObtainHints);
        assert_eq!(cov.left_for(Reason::DroppedByValidation), 3);
        assert_eq!(cov.written, 0);
        assert_eq!(
            en.out
                .warnings
                .iter()
                .filter(|w| w.kind == "dropped-edit")
                .count(),
            3
        );
        assert_eq!(en.out.count(Category::ObtainHints), 0);
        // A name tag (untrimmed edit) is never extended.
        let mut en = Engine::new(&s, &p);
        let tag = Payload::Tag {
            prefix: "[X] ".into(),
            suffix: String::new(),
        };
        assert!(en.offer(Category::ComponentTags, "item_DescOther", None, &tag));
        assert!(!en.offer(
            Category::ObtainHints,
            "item_DescOther",
            None,
            &Payload::Append("\\n\\nGet: Loot".into())
        ));
        assert_eq!(
            en.out
                .coverage
                .get(Category::ObtainHints)
                .left_for(Reason::OwnedByOther),
            1
        );
    }
}
