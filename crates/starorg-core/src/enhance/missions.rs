//! Mission titles and descriptions (spec 6): blueprint pools, item rewards, reputation,
//! required standing and contract type, computed over every contract that shares a key.

use super::format::{int, pct};
use super::owned::{BlueprintRef, Owned, OwnedBlueprints};
use super::profile::{Mission, em};
use super::{Category, Engine, Payload, items, stock_name, tags};
use crate::ini::fold_key;
use crate::model::{Blueprint, Contract, GameData, loc_key};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// `N` or `A–B` over the values present; `None` when there are none.
fn range(values: impl IntoIterator<Item = f64>) -> Option<String> {
    let v: Vec<f64> = values.into_iter().collect();
    let lo = v.iter().copied().reduce(f64::min)?;
    let hi = v.iter().copied().reduce(f64::max)?;
    Some(if int(lo) == int(hi) {
        int(lo)
    } else {
        format!("{}–{}", int(lo), int(hi))
    })
}

fn chance(c: &Contract) -> Option<f64> {
    c.blueprint_rewards
        .iter()
        .map(|r| r.chance.unwrap_or(1.0))
        .reduce(f64::max)
}

/// Difficulty words of contract debug names, in difficulty order, with the text shown.
const DIFFICULTY: [(&str, &str); 6] = [
    ("VeryEasy", "Very Easy"),
    ("Easy", "Easy"),
    ("Medium", "Medium"),
    ("Hard", "Hard"),
    ("VeryHard", "Very Hard"),
    ("Super", "Super"),
];

/// Star systems in the order the game opened them; others sort after, by name.
const SYSTEMS: [&str; 3] = ["Stanton", "Pyro", "Nyx"];

/// Where a variant label comes from, in order of preference (one kind labels a whole group).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LabelKind {
    /// A difficulty word in the debug name (`_Hard`, `_E_`, `_VE_`). Not the difficulty
    /// profile: on LIVE its names are activities (`general`, `logistics`), not difficulties.
    Difficulty,
    /// `RankN` in the debug name.
    Rank,
    /// The required standing's stock name.
    Standing,
    /// The one system a contract is offered in.
    System,
}

const KINDS: [LabelKind; 4] = [
    LabelKind::Difficulty,
    LabelKind::Rank,
    LabelKind::Standing,
    LabelKind::System,
];

/// A variant label with its sort position.
type Label = (i64, String);

/// The difficulty a whole debug-name token names: a word (`VeryEasy`, any case) or a
/// letter code (`VE`, `E`, `M`, `H`, `VH`, `S`).
fn difficulty_word(w: &str) -> Option<Label> {
    let short = match w {
        "VE" => Some("Very Easy"),
        "E" => Some("Easy"),
        "M" => Some("Medium"),
        "H" => Some("Hard"),
        "VH" => Some("Very Hard"),
        "S" => Some("Super"),
        _ => None,
    };
    DIFFICULTY
        .iter()
        .enumerate()
        .find(|(_, (word, text))| short == Some(*text) || w.eq_ignore_ascii_case(word))
        .map(|(i, (_, text))| (i as i64, (*text).to_string()))
}

/// The one value `f` finds over the debug-name tokens; none when none or several differ.
fn one_token(c: &Contract, f: impl Fn(&str) -> Option<Label>) -> Option<Label> {
    let found: BTreeSet<Label> = c.debug_name.split('_').filter_map(f).collect();
    if found.len() == 1 {
        found.into_iter().next()
    } else {
        None
    }
}

/// The systems a debug-name token names: `Stanton1`, `PyroRegionA`, `Nyx`, and combined
/// ones (`PyroNyx`, `Nyx/Stanton`). After a system only another system, digits or a
/// `Region…`/`Star…`/`DC…` suffix may follow; anything else (`Pyroclast`, `StantonBound`)
/// is not a system token.
fn system_tokens(w: &str) -> Option<Vec<Label>> {
    let mut out = Vec::new();
    for part in w.split('/') {
        let mut rest = part;
        let mut found = false;
        while let Some((i, s)) = SYSTEMS
            .iter()
            .enumerate()
            .find(|(_, s)| rest.starts_with(**s))
        {
            out.push((i as i64, (*s).to_string()));
            rest = &rest[s.len()..];
            found = true;
        }
        let tail_ok = rest.is_empty()
            || rest.chars().all(|c| c.is_ascii_digit())
            || ["Region", "Star", "DC"].iter().any(|p| rest.starts_with(p));
        if !found || !tail_ok {
            return None;
        }
    }
    out.sort();
    out.dedup();
    (!out.is_empty()).then_some(out)
}

fn system_order(s: &str) -> Label {
    let i = SYSTEMS
        .iter()
        .position(|x| *x == s)
        .unwrap_or(SYSTEMS.len());
    (i as i64, s.to_string())
}

/// The systems a contract is offered in: its locality prerequisites, else every system its
/// debug-name tokens name.
fn contract_systems(c: &Contract) -> Vec<Label> {
    if !c.systems.is_empty() {
        return c.systems.iter().map(|s| system_order(s)).collect();
    }
    let found: BTreeSet<Label> = c
        .debug_name
        .split('_')
        .filter_map(system_tokens)
        .flatten()
        .collect();
    found.into_iter().collect()
}

fn label_of(kind: LabelKind, c: &Contract, lk: &Lookup) -> Option<Label> {
    match kind {
        LabelKind::Difficulty => one_token(c, difficulty_word),
        LabelKind::Rank => one_token(c, |w| {
            let n: i64 = w.strip_prefix("Rank")?.parse().ok()?;
            Some((n, format!("Rank {n}")))
        }),
        LabelKind::Standing => {
            let (min, key) = lk
                .standings
                .get(&c.min_standing.as_ref()?.to_ascii_lowercase())?;
            let name = stock_name(lk.stock, *key)?;
            Some((min.unwrap_or(0.0) as i64, name.to_string()))
        }
        LabelKind::System => match contract_systems(c).as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        },
    }
}

/// The stock name of the faction a contract's reputation gain goes to.
fn faction_of<'s>(c: &Contract, lk: &Lookup<'s>) -> Option<&'s str> {
    let r = c
        .reputation
        .iter()
        .find(|r| r.on_success && r.amount > 0.0)?;
    stock_name(lk.stock, r.faction_name_key.as_deref())
}

/// Every contract's label of `kind`, or none when one has none. Two different standings
/// that read the same (`Junior` of two factions) get their faction added.
fn labels(kind: LabelKind, group: &[&Contract], lk: &Lookup) -> Option<Vec<Label>> {
    let mut out: Vec<Label> = group
        .iter()
        .map(|c| label_of(kind, c, lk))
        .collect::<Option<_>>()?;
    if kind == LabelKind::Standing {
        let record = |c: &Contract| c.min_standing.as_deref().map(str::to_ascii_lowercase);
        let clashes: Vec<bool> = group
            .iter()
            .zip(&out)
            .map(|(c, l)| {
                group
                    .iter()
                    .zip(&out)
                    .any(|(o, ol)| ol.1 == l.1 && record(o) != record(c))
            })
            .collect();
        for ((c, l), clash) in group.iter().zip(out.iter_mut()).zip(clashes) {
            if clash && let Some(f) = faction_of(c, lk) {
                l.1 = format!("{} ({f})", l.1);
            }
        }
    }
    Some(out)
}

/// Per-variant values of one detail line, and the label kind that separated them.
struct Variants {
    kind: LabelKind,
    values: Vec<(String, Vec<String>)>,
}

/// Per-variant values when the group's contracts differ in `value` and one label kind (not
/// `skip`) separates them: each distinct value with the labels it holds for, in label order,
/// equal values merged. `None` when every contract agrees or no label separates them (the
/// caller keeps its range: a label is never made up).
fn per_variant(
    group: &[&Contract],
    lk: &Lookup,
    skip: Option<LabelKind>,
    value: impl Fn(&Contract) -> Option<String>,
) -> Option<Variants> {
    let values: Vec<Option<String>> = group.iter().map(|c| value(c)).collect();
    if values.iter().all(|v| *v == values[0]) {
        return None;
    }
    for kind in KINDS.into_iter().filter(|k| Some(*k) != skip) {
        let Some(ls) = labels(kind, group, lk) else {
            continue;
        };
        let mut by_label: BTreeMap<&Label, BTreeSet<&Option<String>>> = BTreeMap::new();
        for (l, v) in ls.iter().zip(&values) {
            by_label.entry(l).or_default().insert(v);
        }
        if by_label.values().any(|vs| vs.len() > 1) {
            continue;
        }
        // Labels in order; each value takes the place of its first label.
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for (l, vs) in by_label {
            let v = vs
                .into_iter()
                .next()
                .and_then(Clone::clone)
                .unwrap_or_else(|| "none".into());
            match out.iter_mut().find(|(x, _)| *x == v) {
                Some((_, names)) => names.push(l.1.clone()),
                None => out.push((v, vec![l.1.clone()])),
            }
        }
        return Some(Variants { kind, values: out });
    }
    None
}

/// `100 (Very Easy, Easy) · 150 (Medium)`, each label list cut at `max`.
fn inline(variants: &[(String, Vec<String>)], max: usize) -> String {
    variants
        .iter()
        .map(|(v, ls)| format!("{v} ({})", list_cut(ls, max)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// `A, B, C` cut at `max` with `and N more`.
fn list_cut(items: &[String], max: usize) -> String {
    let mut s = items
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > max {
        s.push_str(&format!(" and {} more", items.len() - max));
    }
    s
}

/// The stock name of the faction every contract's reputation gain goes to, when all agree.
fn gain_faction<'s>(group: &[&Contract], lk: &Lookup<'s>) -> Option<&'s str> {
    let names: BTreeSet<Option<&str>> = group
        .iter()
        .filter(|c| c.reputation_gain().is_some())
        .map(|c| faction_of(c, lk))
        .collect();
    match names.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(n)] => Some(n),
        _ => None,
    }
}

/// The blueprint odds of one contract, as MISSION DETAILS words.
fn odds_text(c: &Contract) -> String {
    match chance(c) {
        None => "none".into(),
        Some(x) if x >= 1.0 => "Guaranteed".into(),
        Some(x) => pct(x * 100.0),
    }
}

/// Title tags over a title's contract group (spec 6.3). `all_owned`: the active character
/// owns every blueprint the group can award (`[BP Owned]` when `title_owned` is on).
pub fn title_tags(group: &[&Contract], m: &Mission, all_owned: bool) -> Option<String> {
    let wrap = |n: u8, t: String| {
        let n = if m.title_em { n } else { 0 };
        format!(" {}", em(n, &format!("[{t}]")))
    };
    let mut out = String::new();
    if m.title_blueprint && group.iter().any(|c| !c.blueprint_rewards.is_empty()) {
        let sure = group.iter().all(|c| chance(c).is_some_and(|x| x >= 1.0));
        let t = if m.title_owned && all_owned {
            "BP Owned"
        } else if sure {
            "BP"
        } else {
            "BP?"
        };
        out.push_str(&wrap(m.title_bp_em, t.into()));
    }
    if m.title_reputation
        && let Some(r) = range(group.iter().filter_map(|c| c.reputation_gain()))
    {
        out.push_str(&wrap(m.title_rep_em, format!("{r} Rep")));
    }
    (!out.is_empty()).then_some(out)
}

/// One blueprint bullet, structured until render so markers can be added (spec 6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bullet {
    pub name: String,
    pub tag: Option<String>,
    /// Whether the active character owns it (`No` when ownership is not looked up). A
    /// bullet that stands for several same-named blueprints is `Unsure` unless they agree.
    pub owned: Owned,
}

impl Bullet {
    /// `- [tag] Name`, then `[Owned]` (owned, `owned_marker`) or `[Missing]` (not owned,
    /// `missing_marker`, an owned set known), each in its colour setting. Every tag added
    /// here is closed on the same line and nothing nests.
    fn render(&self, m: &Mission, known: bool) -> String {
        let mut base = String::new();
        if let Some(t) = &self.tag {
            base.push_str(t);
            base.push(' ');
        }
        base.push_str(&self.name);
        if self.owned == Owned::Yes && m.owned_marker {
            if m.owned_line && m.owned_em != 0 {
                format!("- {}", em(m.owned_em, &format!("{base} [Owned]")))
            } else {
                format!("- {base} {}", em(m.owned_em, "[Owned]"))
            }
        } else if self.owned == Owned::No && known && m.missing_marker {
            format!("- {base} {}", em(m.missing_em, "[Missing]"))
        } else {
            format!("- {base}")
        }
    }
}

/// Inputs the description body needs besides the contracts.
pub struct Lookup<'a> {
    pub stock: &'a crate::ini::StockIni,
    /// The name chain (curated, game, generated) for item and blueprint names.
    pub names: &'a super::names::Resolver<'a>,
    pub pools: HashMap<String, Vec<&'a Blueprint>>,
    pub standings: HashMap<String, (Option<f64>, Option<&'a str>)>,
    /// Produced item short name (lower case) -> name tag text.
    pub item_tags: HashMap<String, String>,
    pub owned: &'a dyn OwnedBlueprints,
    /// Normalised produced-item names that more than one blueprint shares.
    pub shared_names: std::collections::HashSet<String>,
}

impl<'a> Lookup<'a> {
    pub fn new(
        gd: &'a GameData,
        stock: &'a crate::ini::StockIni,
        profile: &super::profile::Profile,
        owned: &'a dyn OwnedBlueprints,
        names: &'a super::names::Resolver<'a>,
    ) -> Self {
        let bps: HashMap<String, &Blueprint> = gd
            .blueprints
            .iter()
            .map(|b| (b.short_name().to_ascii_lowercase(), b))
            .collect();
        let mut by_name: HashMap<String, BTreeSet<&str>> = HashMap::new();
        for b in &gd.blueprints {
            if let Some(n) = names.name(b.produces_name_key.as_deref()) {
                by_name
                    .entry(super::owned::normalize_name(n))
                    .or_default()
                    .insert(b.id.as_str());
            }
        }
        let shared_names = by_name
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(n, _)| n)
            .collect();
        let pools = gd
            .blueprint_pools
            .iter()
            .map(|p| {
                let list = p
                    .blueprints
                    .iter()
                    .filter_map(|e| bps.get(&e.blueprint.to_ascii_lowercase()).copied())
                    .collect();
                (p.name.to_ascii_lowercase(), list)
            })
            .collect();
        let standings = gd
            .reputation_standings
            .iter()
            .map(|s| {
                (
                    s.name.to_ascii_lowercase(),
                    (s.min_reputation, s.display_name_key.as_deref()),
                )
            })
            .collect();
        let mut item_tags = HashMap::new();
        if profile.mission.bullet_tags {
            let t = &profile.tags;
            for c in &gd.components {
                let class = stock_name(stock, c.item.loc.desc.as_deref())
                    .and_then(tags::class_from_description);
                if let Some(tag) =
                    items::component_tag(c, class).and_then(|e| tags::build(&t.components, &e))
                {
                    item_tags.insert(c.item.short_name().to_ascii_lowercase(), tag);
                }
            }
            for w in &gd.weapons {
                if let Some(tag) =
                    items::weapon_tag(w, profile).and_then(|e| tags::build(&t.ship_weapons, &e))
                {
                    item_tags.insert(w.item.short_name().to_ascii_lowercase(), tag);
                }
            }
            for m in &gd.missiles {
                if let Some(tag) = tags::build(&t.missiles, &items::missile_tag(m)) {
                    item_tags.insert(m.item.short_name().to_ascii_lowercase(), tag);
                }
            }
        }
        Self {
            stock,
            names,
            pools,
            standings,
            item_tags,
            owned,
            shared_names,
        }
    }

    /// Every blueprint of a pool that has a display name, before de-duplication: name,
    /// bullet tag and ownership (`No` unless `track`).
    fn entries(&self, pool: &str, track: bool) -> Vec<(&'a str, Option<String>, Owned)> {
        let mut out = Vec::new();
        for b in self
            .pools
            .get(&pool.to_ascii_lowercase())
            .into_iter()
            .flatten()
        {
            let Some(name) = self.names.name(b.produces_name_key.as_deref()) else {
                continue;
            };
            let tag = b
                .produces
                .as_ref()
                .and_then(|p| self.item_tags.get(&p.to_ascii_lowercase()))
                .cloned();
            let owned = if track {
                self.owned.state(&BlueprintRef {
                    id: &b.id,
                    record: b.short_name(),
                    name,
                    name_unique: !self
                        .shared_names
                        .contains(&super::owned::normalize_name(name)),
                })
            } else {
                Owned::No
            };
            out.push((name, tag, owned));
        }
        out
    }

    /// The bullets of a pool. `track`: look up ownership (only when a setting shows it, so
    /// pools that differ only in what is owned still merge when nothing shows it).
    fn bullets(&self, pool: &str, track: bool) -> Vec<Bullet> {
        let mut seen: HashMap<(String, Option<String>), usize> = HashMap::new();
        let mut out: Vec<Bullet> = Vec::new();
        for (name, tag, owned) in self.entries(pool, track) {
            match seen.get(&(name.to_lowercase(), tag.clone())) {
                // Same name and tag as an earlier blueprint: one bullet, and its marker only
                // when both agree.
                Some(&i) => {
                    if out[i].owned != owned {
                        out[i].owned = Owned::Unsure;
                    }
                }
                None => {
                    seen.insert((name.to_lowercase(), tag.clone()), out.len());
                    out.push(Bullet {
                        name: name.to_string(),
                        tag,
                        owned,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.tag.cmp(&b.tag))
        });
        out
    }
}

fn title_case(s: &str) -> String {
    s.split(['_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How sure a title's blueprint reward is, over its contract group: what MISSION DETAILS says
/// and what the note under BLUEPRINTS warns about (`[BP?]` titles).
#[derive(Debug, Clone, PartialEq)]
pub enum Odds {
    /// Every variant awards a pool at 100 %.
    Guaranteed,
    /// Only some contracts with this title award a pool.
    SomeVariants,
    /// Every variant awards a pool, by chance (lowest and highest, 0-1).
    Chance(f64, f64),
}

impl Odds {
    /// The MISSION DETAILS `Blueprint reward:` value: `Guaranteed`, `Some variants only`,
    /// `25%`, `10–25%`.
    pub fn text(&self) -> String {
        match self {
            Odds::Guaranteed => "Guaranteed".into(),
            Odds::SomeVariants => "Some variants only".into(),
            Odds::Chance(lo, hi) => {
                let (lo, hi) = (lo * 100.0, hi * 100.0);
                if int(lo) == int(hi) {
                    pct(hi)
                } else {
                    format!("{}–{}", int(lo), pct(hi))
                }
            }
        }
    }

    /// The line under BLUEPRINTS for a `[BP?]` title; none when the reward is guaranteed.
    fn note(&self) -> Option<String> {
        match self {
            Odds::Guaranteed => None,
            Odds::SomeVariants => Some("Not every offer with this title awards one".into()),
            // Ends with the `%`: the game reads `%` followed by text as a format code.
            Odds::Chance(..) => Some(format!("Awarded by chance: {}", self.text())),
        }
    }
}

fn reward_odds(group: &[&Contract]) -> Option<Odds> {
    odds_by(group, chance)
}

/// [`reward_odds`] for one reward: `chance_of` is the chance a contract awards it (`None`:
/// that contract does not). The blueprint finder asks it per blueprint.
pub fn odds_by(group: &[&Contract], chance_of: impl Fn(&Contract) -> Option<f64>) -> Option<Odds> {
    let ch: Vec<f64> = group.iter().filter_map(|c| chance_of(c)).collect();
    if ch.is_empty() {
        return None;
    }
    if ch.len() < group.len() {
        return Some(Odds::SomeVariants);
    }
    if ch.iter().all(|x| *x >= 1.0) {
        return Some(Odds::Guaranteed);
    }
    let lo = ch.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ch.iter().copied().fold(0.0, f64::max);
    Some(Odds::Chance(lo, hi))
}

/// The description body for a contract group (spec 6.4), without the stock prose.
pub fn description_body(
    group: &[&Contract],
    lk: &Lookup,
    p: &super::profile::Profile,
) -> Option<String> {
    let m = &p.mission;
    let h = |t: &str| em(m.header_em, t);
    let label = |t: &str| em(m.label_em, &format!("{t}:"));
    let known = lk.owned.known();
    let mut sections: Vec<String> = Vec::new();

    if m.blueprints {
        // Distinct pools by content, in first-seen order, with the contracts that award them.
        let mut pools: Vec<(Vec<Bullet>, Vec<&Contract>)> = Vec::new();
        for c in group {
            for r in &c.blueprint_rewards {
                let b = lk.bullets(&r.pool, tracks_owned(m));
                if b.is_empty() {
                    continue;
                }
                match pools.iter_mut().find(|(x, _)| *x == b) {
                    Some((_, cs)) => {
                        if !cs.iter().any(|x| std::ptr::eq(*x, *c)) {
                            cs.push(c)
                        }
                    }
                    None => pools.push((b, vec![c])),
                }
            }
        }
        let cut = |list: &[Bullet]| -> Vec<String> {
            let mut v: Vec<String> = list
                .iter()
                .take(m.max_list)
                .map(|b| b.render(m, known))
                .collect();
            if list.len() > m.max_list {
                v.push(format!("- and {} more", list.len() - m.max_list));
            }
            v
        };
        // `[BP?]` titles: say why under the header, so the list is not read as a sure reward.
        let note = reward_odds(group).and_then(|o| o.note());
        let head = match &note {
            Some(n) => format!("{}\\n{n}", h("BLUEPRINTS")),
            None => h("BLUEPRINTS"),
        };
        if pools.len() == 1 {
            sections.push(format!("{head}\\n{}", cut(&pools[0].0).join("\\n")));
        } else if pools.len() > 1 {
            // Variant label: the shared difficulty of a pool's contracts, when every pool
            // has its own; otherwise `Variant n`.
            let labels: Vec<Option<String>> = pools
                .iter()
                .map(|(_, cs)| {
                    let d: BTreeSet<&str> =
                        cs.iter().filter_map(|c| c.difficulty.as_deref()).collect();
                    (d.len() == 1 && cs.iter().all(|c| c.difficulty.is_some()))
                        .then(|| title_case(d.into_iter().next().unwrap_or_default()))
                })
                .collect();
            let distinct: BTreeSet<&Option<String>> = labels.iter().collect();
            let use_data = labels.iter().all(Option::is_some) && distinct.len() == labels.len();
            let mut body = vec![head];
            for (i, (list, _)) in pools.iter().enumerate() {
                let l = if use_data {
                    labels[i].clone().unwrap_or_default()
                } else {
                    format!("Variant {}", i + 1)
                };
                body.push(em(m.variant_em, &format!("[{l}]")));
                body.extend(cut(list));
            }
            sections.push(body.join("\\n"));
        }
    }

    if m.item_rewards {
        let mut items: BTreeMap<String, (String, Vec<f64>)> = BTreeMap::new();
        for c in group {
            for r in &c.item_rewards {
                let Some(name) = lk.names.name(r.name_key.as_deref()) else {
                    continue;
                };
                let e = items
                    .entry(name.to_lowercase())
                    .or_insert_with(|| (name.to_string(), Vec::new()));
                e.1.push(r.amount);
            }
        }
        if !items.is_empty() {
            let mut lines = vec![h("ITEM REWARDS")];
            let mut list: Vec<String> = items
                .into_values()
                .map(|(name, amounts)| {
                    let hi = amounts.iter().copied().fold(0.0, f64::max);
                    let name = em(m.reward_em, &name);
                    if hi > 1.0 {
                        format!("- {name} ×{}", range(amounts).unwrap_or_default())
                    } else {
                        format!("- {name}")
                    }
                })
                .collect();
            let more = list.len().saturating_sub(m.max_list);
            list.truncate(m.max_list);
            lines.extend(list);
            if more > 0 {
                lines.push(format!("- and {more} more"));
            }
            sections.push(lines.join("\\n"));
        }
    }

    if m.details {
        let mut d: Vec<String> = Vec::new();
        // The kind that split a line into per-variant values, and whether any line kept a
        // range over values that differ: what the Shared text line may name.
        let mut split: Option<LabelKind> = None;
        let mut mixed = false;
        let mut note = |vs: &Option<Variants>, values: Vec<Option<String>>| match vs {
            Some(v) => {
                split.get_or_insert(v.kind);
            }
            None => mixed |= values.iter().any(|x| *x != values[0]),
        };
        let values_of = |f: &dyn Fn(&Contract) -> Option<String>| -> Vec<Option<String>> {
            group.iter().map(|c| f(c)).collect()
        };
        if let Some(odds) = reward_odds(group) {
            let odds_of = |c: &Contract| Some(odds_text(c));
            let vs = per_variant(group, lk, None, odds_of);
            note(&vs, values_of(&odds_of));
            match vs {
                // One line per value: a `%` must end its line (the game reads `%` followed
                // by text as a format code).
                Some(vs) => d.extend(vs.values.iter().map(|(v, ls)| {
                    format!(
                        "{} {v}",
                        label(&format!("Blueprint reward ({})", list_cut(ls, m.max_list)))
                    )
                })),
                None => d.push(format!("{} {}", label("Blueprint reward"), odds.text())),
            }
        }
        let rep_label = match gain_faction(group, lk) {
            Some(f) => format!("Reputation ({f})"),
            None => "Reputation".to_string(),
        };
        let gains = |c: &Contract| c.reputation_gain().map(int);
        let vs = per_variant(group, lk, None, gains);
        note(&vs, values_of(&gains));
        if let Some(vs) = vs {
            d.push(format!(
                "{} {}",
                label(&rep_label),
                inline(&vs.values, m.max_list)
            ));
        } else if let Some(r) = range(group.iter().filter_map(|c| c.reputation_gain())) {
            d.push(format!("{} {r}", label(&rep_label)));
        }
        let fails = |c: &Contract| c.failure_penalty().map(int);
        let vs = per_variant(group, lk, None, fails);
        note(&vs, values_of(&fails));
        if let Some(vs) = vs {
            d.push(format!(
                "{} {}",
                label("Failure"),
                inline(&vs.values, m.max_list)
            ));
        } else if let Some(f) = range(group.iter().filter_map(|c| c.failure_penalty())) {
            d.push(format!("{} {f}", label("Failure")));
        }
        let standing = |c: &Contract| {
            let (min, key) = lk
                .standings
                .get(&c.min_standing.as_ref()?.to_ascii_lowercase())?;
            Some((min.unwrap_or(0.0), stock_name(lk.stock, *key)?))
        };
        // A standing the contract requires but whose name does not resolve is "not known",
        // never "none" (that reads as no requirement).
        let req_name = |c: &Contract| match standing(c) {
            Some((_, n)) => Some(n.to_string()),
            None => c.min_standing.as_ref().map(|_| "not known".to_string()),
        };
        if group.iter().any(|c| standing(c).is_some()) {
            let vs = per_variant(group, lk, Some(LabelKind::Standing), req_name);
            note(&vs, values_of(&req_name));
            if let Some(vs) = vs {
                d.push(format!(
                    "{} {}",
                    label("Requires"),
                    inline(&vs.values, m.max_list)
                ));
            } else if let Some((_, name)) = group
                .iter()
                .filter_map(|c| standing(c))
                .min_by(|a, b| a.0.total_cmp(&b.0))
            {
                d.push(format!("{} {name}", label("Requires")));
            }
        }
        let mut kinds = Vec::new();
        if group.iter().all(|c| c.chain) {
            kinds.push("Chain");
        }
        if group.iter().all(|c| c.once_only) {
            kinds.push("Once only");
        }
        if !kinds.is_empty() {
            d.push(format!("{} {}", label("Type"), kinds.join(", ")));
        }
        // Also for one contract offered in two systems (`Refueling_PyroNyx_...`).
        let systems: BTreeSet<Label> = group.iter().flat_map(|c| contract_systems(c)).collect();
        if systems.len() > 1 {
            let names: Vec<String> = systems.into_iter().map(|(_, s)| s).collect();
            d.push(format!("{} {}", label("Systems"), names.join(", ")));
        }
        // This text is on several offers: say so, and how they are told apart. Counted per
        // contract: two generators can hold contracts with the same debug name.
        if group.len() > 1 {
            let mut line = format!(
                "{} shown on {} contracts",
                label("Shared text"),
                group.len()
            );
            // The labels are the kind that split a line above; with no split, only when every
            // line agrees over the group (a label list next to a range would contradict it).
            let kind = split.or_else(|| {
                (!mixed)
                    .then(|| {
                        KINDS.into_iter().find(|k| {
                            labels(*k, group, lk).is_some_and(|ls| {
                                ls.into_iter().collect::<BTreeSet<Label>>().len() > 1
                            })
                        })
                    })
                    .flatten()
            });
            // Systems already have their own line.
            if let Some(k) = kind.filter(|k| *k != LabelKind::System)
                && let Some(ls) = labels(k, group, lk)
            {
                let names: Vec<String> = ls
                    .into_iter()
                    .collect::<BTreeSet<Label>>()
                    .into_iter()
                    .map(|(_, s)| s)
                    .collect();
                line.push_str(&format!(" ({})", list_cut(&names, m.max_list)));
            }
            d.push(line);
        }
        if !d.is_empty() {
            sections.push(format!("{}\\n{}", h("MISSION DETAILS"), d.join("\\n")));
        }
    }
    (!sections.is_empty()).then(|| sections.join("\\n\\n"))
}

/// A setting shows ownership, so bullets need it looked up.
fn tracks_owned(m: &Mission) -> bool {
    m.owned_marker || m.missing_marker || m.title_owned
}

impl Lookup<'_> {
    /// Every blueprint the group's pools can award is owned (and there is at least one).
    /// Checked per blueprint, before bullets merge same-named ones.
    fn all_owned(&self, group: &[&Contract]) -> bool {
        let mut any = false;
        for c in group {
            for r in &c.blueprint_rewards {
                for (_, _, owned) in self.entries(&r.pool, true) {
                    if owned != Owned::Yes {
                        return false;
                    }
                    any = true;
                }
            }
        }
        any
    }
}

pub(crate) fn usable(c: &Contract) -> bool {
    !c.not_for_release && !c.work_in_progress
}

/// Group contracts by folded key, keeping the first spelling seen.
pub(crate) fn groups<'a>(
    contracts: &[&'a Contract],
    key: impl Fn(&Contract) -> Option<&String>,
) -> BTreeMap<String, (String, Vec<&'a Contract>)> {
    let mut out: BTreeMap<String, (String, Vec<&'a Contract>)> = BTreeMap::new();
    for c in contracts {
        let Some(k) = key(c).and_then(|k| loc_key(k)) else {
            continue;
        };
        out.entry(fold_key(k))
            .or_insert_with(|| (k.to_string(), Vec::new()))
            .1
            .push(c);
    }
    out
}

pub(crate) fn record_label(group: &[&Contract]) -> String {
    match group {
        [one] => one.debug_name.clone(),
        many => format!("{} (+{} more)", many[0].debug_name, many.len() - 1),
    }
}

/// Title tags and description bodies for all contracts (spec 6.2-6.4).
pub fn generate(
    en: &mut Engine,
    gd: &GameData,
    owned: &dyn OwnedBlueprints,
    names: &super::names::Resolver,
) {
    let p = en.profile;
    let contracts: Vec<&Contract> = gd.contracts.iter().filter(|c| usable(c)).collect();
    let titles = groups(&contracts, |c| c.title_key.as_ref());
    let m = &p.mission;
    let lk = Lookup::new(gd, en.stock, p, owned, names);
    if m.title_blueprint || m.title_reputation {
        for (key, group) in titles.values() {
            let all_owned = m.title_blueprint && m.title_owned && lk.all_owned(group);
            if let Some(t) = title_tags(group, m, all_owned) {
                en.offer(
                    Category::MissionTitles,
                    key,
                    Some(record_label(group)),
                    &Payload::Append(t),
                );
            }
        }
    }
    for (folded, (key, group)) in groups(&contracts, |c| c.description_key.as_ref()) {
        // A key used as a title never receives a description body (spec 6.2).
        if titles.contains_key(&folded) {
            continue;
        }
        if let Some(body) = description_body(&group, &lk, p) {
            en.offer(
                Category::MissionDescriptions,
                &key,
                Some(record_label(&group)),
                &Payload::Append(format!("\\n\\n{body}")),
            );
        }
    }
}
