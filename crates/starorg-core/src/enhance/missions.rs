//! Mission titles and descriptions (spec 6): blueprint pools, item rewards, reputation,
//! required standing and contract type, computed over every contract that shares a key.

use super::format::{int, pct};
use super::owned::{BlueprintRef, OwnedBlueprints};
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

/// Title tags over a title's contract group (spec 6.3).
pub fn title_tags(group: &[&Contract], em: bool, bp: bool, rep: bool) -> Option<String> {
    let wrap = |t: String| {
        if em {
            format!(" <EM4>[{t}]</EM4>")
        } else {
            format!(" [{t}]")
        }
    };
    let mut out = String::new();
    if bp && group.iter().any(|c| !c.blueprint_rewards.is_empty()) {
        let sure = group.iter().all(|c| chance(c).is_some_and(|x| x >= 1.0));
        out.push_str(&wrap(if sure { "BP".into() } else { "BP?".into() }));
    }
    if rep && let Some(r) = range(group.iter().filter_map(|c| c.reputation_gain())) {
        out.push_str(&wrap(format!("{r} Rep")));
    }
    (!out.is_empty()).then_some(out)
}

/// One blueprint bullet, structured until render so markers can be added (spec 6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bullet {
    pub name: String,
    pub tag: Option<String>,
    pub owned: bool,
}

impl Bullet {
    fn render(&self) -> String {
        let mut s = String::from("- ");
        if let Some(t) = &self.tag {
            s.push_str(t);
            s.push(' ');
        }
        s.push_str(&self.name);
        if self.owned {
            s.push_str(" [Owned]");
        }
        s
    }
}

/// Inputs the description body needs besides the contracts.
pub struct Lookup<'a> {
    pub stock: &'a crate::ini::StockIni,
    pub pools: HashMap<String, Vec<&'a Blueprint>>,
    pub standings: HashMap<String, (Option<f64>, Option<&'a str>)>,
    /// Produced item short name (lower case) -> name tag text.
    pub item_tags: HashMap<String, String>,
    pub owned: &'a dyn OwnedBlueprints,
}

impl<'a> Lookup<'a> {
    pub fn new(
        gd: &'a GameData,
        stock: &'a crate::ini::StockIni,
        profile: &super::profile::Profile,
        owned: &'a dyn OwnedBlueprints,
    ) -> Self {
        let bps: HashMap<String, &Blueprint> = gd
            .blueprints
            .iter()
            .map(|b| (b.short_name().to_ascii_lowercase(), b))
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
            pools,
            standings,
            item_tags,
            owned,
        }
    }

    fn bullets(&self, pool: &str, marker: bool) -> Vec<Bullet> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for b in self
            .pools
            .get(&pool.to_ascii_lowercase())
            .into_iter()
            .flatten()
        {
            let Some(name) = stock_name(self.stock, b.produces_name_key.as_deref()) else {
                continue;
            };
            let tag = b
                .produces
                .as_ref()
                .and_then(|p| self.item_tags.get(&p.to_ascii_lowercase()))
                .cloned();
            let owned = marker
                && self.owned.owns(&BlueprintRef {
                    id: &b.id,
                    record: b.short_name(),
                    name,
                });
            if seen.insert((name.to_lowercase(), tag.clone())) {
                out.push(Bullet {
                    name: name.to_string(),
                    tag,
                    owned,
                });
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

/// The description body for a contract group (spec 6.4), without the stock prose.
pub fn description_body(
    group: &[&Contract],
    lk: &Lookup,
    p: &super::profile::Profile,
) -> Option<String> {
    let m = &p.mission;
    let h = |t: &str| format!("<EM{n}>{t}</EM{n}>", n = m.header_em);
    let label = |t: &str| format!("<EM{n}>{t}:</EM{n}>", n = m.label_em);
    let mut sections: Vec<String> = Vec::new();

    if m.blueprints {
        // Distinct pools by content, in first-seen order, with the contracts that award them.
        let mut pools: Vec<(Vec<Bullet>, Vec<&Contract>)> = Vec::new();
        for c in group {
            for r in &c.blueprint_rewards {
                let b = lk.bullets(&r.pool, m.owned_marker);
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
            let mut v: Vec<String> = list.iter().take(m.max_list).map(Bullet::render).collect();
            if list.len() > m.max_list {
                v.push(format!("- and {} more", list.len() - m.max_list));
            }
            v
        };
        if pools.len() == 1 {
            sections.push(format!(
                "{}\\n{}",
                h("BLUEPRINTS"),
                cut(&pools[0].0).join("\\n")
            ));
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
            let mut body = vec![h("BLUEPRINTS")];
            for (i, (list, _)) in pools.iter().enumerate() {
                let l = if use_data {
                    labels[i].clone().unwrap_or_default()
                } else {
                    format!("Variant {}", i + 1)
                };
                body.push(format!("<EM4>[{l}]</EM4>"));
                body.extend(cut(list));
            }
            sections.push(body.join("\\n"));
        }
    }

    if m.item_rewards {
        let mut items: BTreeMap<String, (String, Vec<f64>)> = BTreeMap::new();
        for c in group {
            for r in &c.item_rewards {
                let Some(name) = stock_name(lk.stock, r.name_key.as_deref()) else {
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
        let with_pool: Vec<&&Contract> = group
            .iter()
            .filter(|c| !c.blueprint_rewards.is_empty())
            .collect();
        if !with_pool.is_empty() {
            let text = if with_pool.len() < group.len() {
                "Some variants only".to_string()
            } else {
                let ch: Vec<f64> = with_pool.iter().filter_map(|c| chance(c)).collect();
                if ch.iter().all(|x| *x >= 1.0) {
                    "Guaranteed".into()
                } else {
                    let lo = ch.iter().copied().fold(f64::INFINITY, f64::min) * 100.0;
                    let hi = ch.iter().copied().fold(0.0, f64::max) * 100.0;
                    if int(lo) == int(hi) {
                        pct(hi)
                    } else {
                        format!("{}–{}", int(lo), pct(hi))
                    }
                }
            };
            d.push(format!("{} {text}", label("Blueprint reward")));
        }
        if let Some(r) = range(group.iter().filter_map(|c| c.reputation_gain())) {
            d.push(format!("{} {r}", label("Reputation")));
        }
        if let Some(f) = range(group.iter().filter_map(|c| c.failure_penalty())) {
            d.push(format!("{} {f}", label("Failure")));
        }
        let req = group
            .iter()
            .filter_map(|c| {
                lk.standings
                    .get(&c.min_standing.as_ref()?.to_ascii_lowercase())
            })
            .filter_map(|(min, key)| Some((min.unwrap_or(0.0), stock_name(lk.stock, *key)?)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, name)) = req {
            d.push(format!("{} {name}", label("Requires")));
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
        if !d.is_empty() {
            sections.push(format!("{}\\n{}", h("MISSION DETAILS"), d.join("\\n")));
        }
    }
    (!sections.is_empty()).then(|| sections.join("\\n\\n"))
}

fn usable(c: &Contract) -> bool {
    !c.not_for_release && !c.work_in_progress
}

/// Group contracts by folded key, keeping the first spelling seen.
fn groups<'a>(
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

fn record_label(group: &[&Contract]) -> String {
    match group {
        [one] => one.debug_name.clone(),
        many => format!("{} (+{} more)", many[0].debug_name, many.len() - 1),
    }
}

/// Title tags and description bodies for all contracts (spec 6.2-6.4).
pub fn generate(en: &mut Engine, gd: &GameData, owned: &dyn OwnedBlueprints) {
    let p = en.profile;
    let contracts: Vec<&Contract> = gd.contracts.iter().filter(|c| usable(c)).collect();
    let titles = groups(&contracts, |c| c.title_key.as_ref());
    let m = &p.mission;
    if m.title_blueprint || m.title_reputation {
        for (key, group) in titles.values() {
            if let Some(t) = title_tags(group, m.title_em, m.title_blueprint, m.title_reputation) {
                en.offer(
                    Category::MissionTitles,
                    key,
                    Some(record_label(group)),
                    &Payload::Append(t),
                );
            }
        }
    }
    let lk = Lookup::new(gd, en.stock, p, owned);
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
