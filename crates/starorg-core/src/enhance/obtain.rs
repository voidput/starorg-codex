//! Obtain hints (spec obtain-hints): a WHERE TO GET block after item and commodity
//! descriptions, from the neutral [`ObtainHints`] model (loot, shops, mining, ore quality)
//! and from `GameData` itself (craft: a blueprint produces the item; rewards: contracts that
//! award the item or its blueprint).
//!
//! The block goes after everything else on the key: when a stat block (or a quest hint)
//! already owns it, the Engine extends that edit ([`super::Category::extends`]). Lines
//! (`lines_off = ["obtain.loot", ...]`): `shops`, `sell`, `loot`, `craft`, `rewards`,
//! `mined`, `quality`. A line with nothing to say is left out; nothing at all means no block.
//!
//! The `sell` line is the only one with prices (commodity kiosks, from UEX). Prices go stale
//! between applies, so it needs its own switch (`obtain_prices`, on only in Everything), the
//! price is rounded (`~6.2k aUEC/SCU`) and the line is stamped with the day the prices were
//! fetched (`UEX, 21 Nov`).
//!
//! Text from the hints is data, not markup: EM tags, `~` variables, `%` and control
//! characters are taken out before it is placed, so a place name cannot break the checks.

use super::profile::{ObtainMode, Profile, em};
use super::{Category, Claim, Engine, Payload, format};
use crate::ini::{IniError, StockIni};
use crate::model::{
    Blueprint, GameData, ItemObtain, LootHint, MinedHint, ObtainHints, QualityHint, SellHint,
    ShopHint, loc_key,
};
use std::collections::BTreeMap;

/// The `lines_off` category of this block.
pub const LINE_CATEGORY: &str = "obtain";
/// Line names, in block order.
pub const LINES: &[&str] = &[
    "shops", "sell", "loot", "craft", "rewards", "mined", "quality",
];
/// The unit of a commodity price.
pub const PRICE_UNIT: &str = "aUEC/SCU";
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

/// A percent the checks accept anywhere a line or `  |  ` follows: `21%`, `0.5%`, and
/// `under 0.1%` for odds that would round to `0%`.
fn odds(fraction: f64) -> String {
    let v = fraction * 100.0;
    if v < 0.05 {
        "under 0.1%".into()
    } else if v < 10.0 {
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

/// A price rounded to two significant digits, with `k` / `M`: `~6.2k`, `~12k`, `~1.5M`,
/// `~850`, `~3.8`. Player-reported prices move, so the text never claims more.
pub fn rough_price(v: f64) -> String {
    if !v.is_finite() || v <= 0.0 {
        return "~0".into();
    }
    let mag = v.log10().floor() as i32;
    let step = 10f64.powi(mag - 1);
    let r = (v / step).round() * step;
    let (n, unit) = if r >= 1e6 {
        (r / 1e6, "M")
    } else if r >= 1e3 {
        (r / 1e3, "k")
    } else {
        (r, "")
    };
    let digits = if n >= 10.0 {
        format::int(n)
    } else if n >= 1.0 {
        format::d1(n)
    } else {
        format::sig2(n)
    };
    format!("~{digits}{unit}")
}

/// `21 Nov` for an RFC 3339 time (`2026-11-21T09:00:00Z`); `None` when unreadable.
pub fn short_day(iso: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month: usize = iso.get(5..7)?.parse().ok()?;
    let day: u32 = iso.get(8..10)?.parse().ok()?;
    let m = MONTHS.get(month.checked_sub(1)?)?;
    (1..=31).contains(&day).then(|| format!("{day} {m}"))
}

/// `(UEX, 21 Nov)`: where the prices are from and when they were fetched.
fn price_stamp(fetched: Option<&str>) -> String {
    match fetched.and_then(short_day) {
        Some(d) => format!("(UEX, {d})"),
        None => "(UEX)".into(),
    }
}

/// Lower case, letters and digits only.
fn plain_key(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// `HUR-L2`, or in Detailed `TDD Area 18 (Stanton)` / `HUR-L2 (HUR-L2 Faithful Dream
/// Station, Stanton)`: a place or system the terminal's name already holds is not repeated
/// (`Pyro Gateway (Nyx)`).
fn sell_place(s: &SellHint, detailed: bool) -> String {
    let terminal = clean(&s.terminal);
    if !detailed {
        return terminal;
    }
    let named = plain_key(&terminal);
    let inner = [opt(&s.place), opt(&s.system)]
        .into_iter()
        .flatten()
        .filter(|p| !named.contains(&plain_key(p)));
    with_paren(terminal, distinct(inner))
}

/// Each kiosk with its rounded price, the first carrying the unit:
/// `~9.6k aUEC/SCU at HUR-L2`, `~9.2k at Rod's Fuel`.
fn sell_entries(sells: &[&SellHint], detailed: bool) -> Vec<String> {
    sells
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let unit = if i == 0 {
                format!(" {PRICE_UNIT}")
            } else {
                String::new()
            };
            format!(
                "{}{unit} at {}",
                rough_price(s.price),
                sell_place(s, detailed)
            )
        })
        .collect()
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

/// Where most rolls land: the curve's mean, kept inside min-max (a curve cut at its minimum
/// has its mean below it, e.g. 501-1000 with mean 500).
fn peak(q: &QualityHint) -> f64 {
    q.mean.clamp(q.min, q.max.max(q.min))
}

fn quality(q: &QualityHint, detailed: bool) -> String {
    let mut s = format!(
        "{}–{}, most near {}",
        format::int(q.min),
        format::int(q.max),
        format::int(peak(q))
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
/// `prices_fetched` ([`ObtainHints::prices_fetched`]) stamps the sell line.
pub fn block(
    info: &ItemObtain,
    crafted: &Crafted,
    p: &Profile,
    prices_fetched: Option<&str>,
) -> Option<String> {
    let on = |line: &str| p.line_on(LINE_CATEGORY, line);
    let detailed = p.obtain_mode == ObtainMode::Detailed;
    let label = |t: &str| em(p.mission.label_em, &format!("{t}:"));
    let shops: Vec<String> = if p.obtain_shops && on("shops") {
        distinct(info.shops.iter().map(|s| shop(s, detailed)))
    } else {
        Vec::new()
    };
    let sells: Vec<&SellHint> = if p.obtain_prices && on("sell") {
        info.sells
            .iter()
            .filter(|s| s.price.is_finite() && s.price > 0.0 && !clean(&s.terminal).is_empty())
            .collect()
    } else {
        Vec::new()
    };
    let sells_extra = if sells.is_empty() {
        0
    } else {
        info.sells_count().saturating_sub(info.sells.len())
    };
    let stamp = price_stamp(prices_fetched);
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
        if let Some(best) = sell_entries(&sells[..sells.len().min(1)], false).first() {
            parts.push(format!("Sell {best} {stamp}"));
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
            parts.push(format!("Quality most near {}", format::int(peak(q))));
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
    if !sells.is_empty() {
        let entries = sell_entries(&sells, detailed);
        lines.push(format!(
            "{} {} {stamp}",
            label("Sell"),
            top(&entries, sells_extra, list_sep)
        ));
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
/// stock, or filled by missing-names); the namesake rule decides shared keys, and an owned key is extended.
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
            // A description stock lacks counts once missing-names filled it.
            if matches!(stock.find(k), Err(e) if !matches!(e, IniError::AmbiguousKey { .. }))
                && !en.is_filled(k)
            {
                return None;
            }
            let class = class.to_ascii_lowercase();
            let info = hints.items.get(&class).unwrap_or(&empty);
            let text = block(
                info,
                &idx.crafted(&class),
                p,
                hints.prices_fetched.as_deref(),
            )?;
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
            block(&gun(), &c, &profile(ObtainMode::Short), None).as_deref(),
            Some("<EM4>Get:</EM4> Buy · Loot (Rare) · Craft [BP] · Reward")
        );
        assert_eq!(
            block(&gun(), &c, &profile(ObtainMode::Normal), None).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Buy:</EM4> Center Mass (Area18), Live Fire Weapons (Port Olisar)\\n\
                 <EM4>Loot:</EM4> Weapon crate (Rare) at Bunkers, Ammo box (Rare) at Derelicts, Locker (Uncommon) at Outposts +3\\n\
                 <EM4>Craft:</EM4> from blueprint\\n\
                 <EM4>Reward:</EM4> Bunker Clearing [BP], Cargo Run"
            )
        );
        assert_eq!(
            block(&gun(), &c, &profile(ObtainMode::Detailed), None).as_deref(),
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
            let b = block(&gun(), &c, &profile(mode), None).unwrap();
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
            block(&ore(), &none, &profile(ObtainMode::Short), None).as_deref(),
            Some(
                "<EM4>Get:</EM4> Mined (hand) at Lyria, Pyro I, Aberdeen +1 · Quality most near 500"
            )
        );
        assert_eq!(
            block(&ore(), &none, &profile(ObtainMode::Normal), None).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Mined:</EM4> Lyria, Pyro I, Aberdeen +1 (hand)\\n\
                 <EM4>Quality:</EM4> 1–900, most near 500"
            )
        );
        assert_eq!(
            block(&ore(), &none, &profile(ObtainMode::Detailed), None).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Mined:</EM4> Lyria, Stanton (hand) 12–30%  |  Pyro I, Pyro (hand) 5%  |  Aberdeen, Stanton (hand) 1–8%  |  +1\\n\
                 <EM4>Quality:</EM4> 1–900, most near 500 ± 120; Lyria 600–900"
            )
        );
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            let b = block(&ore(), &none, &profile(mode), None).unwrap();
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
            block(&gun(), &c, &p, None).as_deref(),
            Some("<EM3>WHERE TO GET</EM3>\\n<EM4>Reward:</EM4> Bunker Clearing [BP], Cargo Run")
        );
        for l in LINES {
            p.lines_off.insert(format!("obtain.{l}"));
        }
        p.obtain_shops = true;
        assert_eq!(block(&gun(), &c, &p, None), None);
        assert_eq!(block(&ore(), &c, &p, None), None);
        // Colours off: plain labels.
        let mut p = profile(ObtainMode::Short);
        p.mission.label_em = 0;
        assert_eq!(
            block(&ore(), &Crafted::default(), &p, None).as_deref(),
            Some("Get: Mined (hand) at Lyria, Pyro I, Aberdeen +1 · Quality most near 500")
        );
    }

    #[test]
    fn empty_sources_give_no_block() {
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            assert_eq!(
                block(
                    &ItemObtain::default(),
                    &Crafted::default(),
                    &profile(mode),
                    None
                ),
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
                &profile(ObtainMode::Normal),
                None
            )
            .as_deref(),
            Some("<EM3>WHERE TO GET</EM3>\\n<EM4>Loot:</EM4> 4 containers")
        );
    }

    /// Refined laranite: five kiosks that buy it, best first, and one that pays nothing.
    fn laranite() -> ItemObtain {
        let k = |t: &str, place: Option<&str>, sys: &str, price: f64| SellHint {
            terminal: t.into(),
            place: place.map(str::to_string),
            system: Some(sys.into()),
            price,
        };
        ItemObtain {
            sells: vec![
                k(
                    "HUR-L2",
                    Some("HUR-L2 Faithful Dream Station"),
                    "Stanton",
                    9612.0,
                ),
                k("Rod's Fuel", Some("Rod's Fuel 'N Supplies"), "Pyro", 9180.0),
                k("TDD Area 18", Some("Area 18"), "Stanton", 9000.0),
                k("TDD New Babbage", Some("New Babbage"), "Stanton", 8900.0),
                k("Ruin Station", None, "Pyro", 8700.0),
                k("Free kiosk", None, "Pyro", 0.0),
            ],
            ..Default::default()
        }
    }

    fn prices(mode: ObtainMode) -> Profile {
        Profile {
            obtain_prices: true,
            ..profile(mode)
        }
    }

    const FETCHED: Option<&str> = Some("2026-11-21T09:00:00.000Z");

    #[test]
    fn sell_line_per_mode() {
        let none = Crafted::default();
        let b = |mode| block(&laranite(), &none, &prices(mode), FETCHED);
        assert_eq!(
            b(ObtainMode::Short).as_deref(),
            Some("<EM4>Get:</EM4> Sell ~9.6k aUEC/SCU at HUR-L2 (UEX, 21 Nov)")
        );
        assert_eq!(
            b(ObtainMode::Normal).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Sell:</EM4> ~9.6k aUEC/SCU at HUR-L2, ~9.2k at Rod's Fuel, ~9k at TDD Area 18 +2 (UEX, 21 Nov)"
            )
        );
        assert_eq!(
            b(ObtainMode::Detailed).as_deref(),
            Some(
                "<EM3>WHERE TO GET</EM3>\\n\
                 <EM4>Sell:</EM4> ~9.6k aUEC/SCU at HUR-L2 (HUR-L2 Faithful Dream Station, Stanton)  |  ~9.2k at Rod's Fuel (Rod's Fuel 'N Supplies, Pyro)  |  ~9k at TDD Area 18 (Stanton)  |  +2 (UEX, 21 Nov)"
            )
        );
        for mode in [ObtainMode::Short, ObtainMode::Normal, ObtainMode::Detailed] {
            let suffix = format!("\\n\\n{}", b(mode).unwrap());
            assert_eq!(
                check::check_generated("Ore.", "Ore.", "", &suffix),
                Ok(()),
                "{suffix}"
            );
        }
        // Only the top 3 kept, 12 in all: the rest are counted.
        let mut cut = laranite();
        cut.sells.truncate(3);
        cut.sells_total = 12;
        assert!(
            block(&cut, &none, &prices(ObtainMode::Normal), FETCHED)
                .unwrap()
                .ends_with("~9k at TDD Area 18 +9 (UEX, 21 Nov)")
        );
        // A system the terminal's name holds is not said twice.
        cut.sells = vec![SellHint {
            terminal: "Pyro Gateway (Nyx)".into(),
            place: Some("Pyro Gateway".into()),
            system: Some("Nyx".into()),
            price: 31_000.0,
        }];
        cut.sells_total = 0;
        assert!(
            block(&cut, &none, &prices(ObtainMode::Detailed), FETCHED)
                .unwrap()
                .ends_with("~31k aUEC/SCU at Pyro Gateway (Nyx) (UEX, 21 Nov)")
        );
        // No fetch day known: still says where the prices are from.
        assert!(
            block(&laranite(), &none, &prices(ObtainMode::Normal), None)
                .unwrap()
                .ends_with("+2 (UEX)")
        );
        // Sells come after shops and before the rest.
        let mut both = gun();
        both.sells = laranite().sells;
        let g = gd();
        let s = stock();
        let n = block(
            &both,
            &crafted(&g, &s),
            &prices(ObtainMode::Normal),
            FETCHED,
        )
        .unwrap();
        let at = |l: &str| n.find(l).unwrap();
        assert!(at("Buy:") < at("Sell:") && at("Sell:") < at("Loot:"), "{n}");
        let short = block(&both, &crafted(&g, &s), &prices(ObtainMode::Short), FETCHED).unwrap();
        assert!(
            short.contains("Buy · Sell ~9.6k aUEC/SCU at HUR-L2 (UEX, 21 Nov) · Loot (Rare)"),
            "{short}"
        );
    }

    #[test]
    fn sell_line_needs_its_switch_and_its_line() {
        let none = Crafted::default();
        // Off by default (Recommended and Minimal); on in Everything.
        assert!(!Profile::default().obtain_prices && !Profile::minimal().obtain_prices);
        assert!(Profile::everything().obtain_prices);
        assert_eq!(
            block(&laranite(), &none, &profile(ObtainMode::Normal), FETCHED),
            None
        );
        let mut p = prices(ObtainMode::Normal);
        p.lines_off.insert("obtain.sell".into());
        assert_eq!(block(&laranite(), &none, &p, FETCHED), None);
        // Only kiosks that pay: nothing to say without one.
        let free = ItemObtain {
            sells: vec![laranite().sells[5].clone()],
            ..Default::default()
        };
        assert_eq!(
            block(&free, &none, &prices(ObtainMode::Normal), FETCHED),
            None
        );
        // The mined line is untouched by the sell switch.
        let mut ore_sold = ore();
        ore_sold.sells = laranite().sells;
        let off = block(&ore_sold, &none, &profile(ObtainMode::Normal), FETCHED).unwrap();
        assert!(!off.contains("Sell") && off.contains("Mined:"), "{off}");
        // A place that is the system is said once.
        let one = ItemObtain {
            sells: vec![SellHint {
                terminal: "Ruin Station".into(),
                place: Some("Pyro".into()),
                system: Some("Pyro".into()),
                price: 8700.0,
            }],
            ..Default::default()
        };
        assert!(
            block(&one, &none, &prices(ObtainMode::Detailed), FETCHED)
                .unwrap()
                .ends_with("<EM4>Sell:</EM4> ~8.7k aUEC/SCU at Ruin Station (Pyro) (UEX, 21 Nov)")
        );
    }

    #[test]
    fn prices_round_and_days_read_short() {
        for (v, want) in [
            (6240.0, "~6.2k"),
            (9000.0, "~9k"),
            (9612.0, "~9.6k"),
            (12_345.0, "~12k"),
            (123_456.0, "~120k"),
            (999_999.0, "~1M"),
            (1_540_000.0, "~1.5M"),
            (845.0, "~850"),
            (38.0, "~38"),
            (3.84, "~3.8"),
            (0.456, "~0.46"),
        ] {
            assert_eq!(rough_price(v), want, "{v}");
        }
        assert_eq!(
            short_day("2026-11-21T09:00:00.000Z").as_deref(),
            Some("21 Nov")
        );
        assert_eq!(short_day("2026-01-05").as_deref(), Some("5 Jan"));
        assert_eq!(short_day("2026-13-05"), None);
        assert_eq!(short_day("soon"), None);
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
        let b = block(
            &info,
            &Crafted::default(),
            &profile(ObtainMode::Normal),
            None,
        )
        .unwrap();
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
        assert_eq!(odds(0.0004), "under 0.1%");
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
            prices_fetched: None,
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

    #[test]
    fn quality_peak_stays_inside_the_range() {
        let q = QualityHint {
            min: 501.0,
            max: 1000.0,
            mean: 500.0,
            stddev: Some(203.0),
            overrides: Vec::new(),
        };
        assert_eq!(quality(&q, false), "501–1,000, most near 501");
    }
}
