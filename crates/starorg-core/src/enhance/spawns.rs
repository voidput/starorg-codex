//! The SPAWNS section of mission descriptions: the ships a contract spawns, from its spawn
//! groups (`Contract::spawns`), per variant when the variants differ.
//!
//! One line per group kind: `Salvage target: Gladius (smallest, light damage)`,
//! `Hostiles (Wave 1): 3 ships · Arrow, Cutlass Black and 12 more (pilot skill 50)`. Names
//! come from the vehicles the option's tag query matched; an option that matched none says
//! what the game picks from (`picked by the game from Salvage › Available to salvage ›
//! Smallest`).

use super::format::int;
use super::missions::{Lookup, list_cut, per_variant};
use super::profile::Mission;
use crate::model::{Contract, SpawnGroup, SpawnOption, SpawnRole};

/// Ship names per line at most (and `max_list` when lower): a combat query can match fifty
/// hulls, and a description holds several lines.
const SHIP_NAMES: usize = 6;

/// A selector that matches every ship.
const ANY_SHIP: &str = "Missions/VehicleType/Ship";

/// What `per_variant` prints for a contract without the value; such variants are left off.
const ABSENT: &str = "none";

/// Characters the game reads as markup or format codes in a description.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '%' | '<' | '>' | '=' | '\\' | '~'))
        .collect()
}

/// Trimmed, one space between words (no-break spaces too).
fn tidy(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `AvailableToSalvage` -> `Available to salvage`, `Wave1` -> `Wave 1`, `Hornet_F7C` ->
/// `Hornet F7C`: words split at case changes, digits and `_`; capitalised words after the
/// first in lower case, all-caps words and words with digits kept. Markup characters go.
fn humanize(s: &str) -> String {
    let s = sanitize(s);
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev: Option<char> = None;
    for ch in s.chars() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev = None;
            continue;
        }
        let split =
            prev.is_some_and(|p| p.is_lowercase() && (ch.is_uppercase() || ch.is_ascii_digit()));
        if split && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
        prev = Some(ch);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let plain = w.chars().skip(1).all(|c| c.is_lowercase());
            if i > 0 && plain {
                w.to_lowercase()
            } else {
                w.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The size word of a selector under `Missions/MissionType/<type>/<pool>/<size>`
/// (`smallest`, `large`, `very small` from `RefuelVerySmall`).
fn size_word(selector: &str) -> Option<String> {
    let rest = selector.strip_prefix("Missions/MissionType/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 3 {
        return None;
    }
    let leaf = parts[parts.len() - 1];
    let leaf = leaf.strip_prefix("Refuel").unwrap_or(leaf);
    Some(humanize(leaf).to_lowercase())
}

/// `salvagecontractor_lightdamage` -> `light damage`.
fn damage_word(record: &str) -> Option<String> {
    let r = record.to_ascii_lowercase();
    ["light", "medium", "heavy", "high"]
        .iter()
        .find(|w| r.contains(&format!("{w}damage")))
        .map(|w| format!("{w} damage"))
}

/// What the game picks from, for an option no vehicle in the data matched: the known tag
/// paths only (unknown ids are never shown).
fn selector_text(selector: &[String]) -> String {
    let mut words = Vec::new();
    for p in selector {
        push_new(
            &mut words,
            match p.strip_prefix("Missions/MissionType/") {
                Some(rest) => rest
                    .split('/')
                    .map(humanize)
                    .collect::<Vec<_>>()
                    .join(" › "),
                None => humanize(p.rsplit('/').next().unwrap_or(p)),
            },
        );
    }
    words.retain(|w| !w.is_empty());
    words.join(", ")
}

/// Cargo words of the applied `AI/CargoManifest/General/...` tags, with their order:
/// legality, value, fill, type.
fn cargo_word(applied: &str) -> Option<(u8, String)> {
    let rest = applied.strip_prefix("AI/CargoManifest/General/")?;
    let (family, leaf) = rest.split_once('/')?;
    Some(match (family, leaf) {
        ("Legality", "Ilegal" | "Illegal") => (0, "illegal".to_string()),
        ("Legality", "Mixed") => (0, "mixed legality".to_string()),
        ("Legality", _) => (0, humanize(leaf).to_lowercase()),
        ("Value", _) => (1, humanize(leaf).to_lowercase()),
        ("PopulationVariation", _) => (2, humanize(leaf).to_lowercase()),
        _ => (3, humanize(leaf).to_lowercase()),
    })
}

/// Pilot skill of an applied `AI/SkillDefinitions/Description/HumanPilotNN` tag.
fn skill(applied: &str) -> Option<f64> {
    applied
        .strip_prefix("AI/SkillDefinitions/Description/HumanPilot")?
        .parse()
        .ok()
}

fn is_salvage(g: &SpawnGroup) -> bool {
    g.variable.to_ascii_lowercase().contains("salvage")
        || g.name.to_ascii_lowercase().contains("salvage")
        || g.slots.iter().flat_map(|s| &s.options).any(|o| {
            o.selector
                .iter()
                .any(|p| p.starts_with("Missions/MissionType/Salvage/"))
        })
}

fn role_label(g: &SpawnGroup) -> &'static str {
    match g.role {
        SpawnRole::Target if is_salvage(g) => "Salvage target",
        SpawnRole::Target => "Target",
        SpawnRole::Hostile => "Hostiles",
        SpawnRole::Escort => "Friendly",
        SpawnRole::Cargo => "Cargo ship",
        SpawnRole::Other => "Ships",
    }
}

/// Line order: what the contract is about first, then who helps and who attacks.
fn role_order(label: &str) -> u8 {
    match label {
        "Salvage target" | "Target" => 0,
        "Cargo ship" => 1,
        "Friendly" => 2,
        "Hostiles" => 3,
        _ => 4,
    }
}

/// The group's own name as a label (`Wave 1`); none when it has none. Mission variable
/// names are never shown.
fn group_label(g: &SpawnGroup) -> Option<String> {
    Some(humanize(&g.name)).filter(|s| !s.is_empty())
}

fn push_new(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

/// Words after a base name that mark a special edition (`... Best In Show Edition`).
fn special_edition(rest: &str) -> bool {
    let r = rest.to_lowercase();
    ["best in show", "teach's special", "wikelo"]
        .iter()
        .any(|w| r.contains(w))
        || r.split(' ')
            .any(|w| matches!(w, "exec" | "executive" | "edition" | "special"))
}

/// Names without the special editions of a base name also in the list.
fn without_editions(names: Vec<String>) -> Vec<String> {
    let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    names
        .iter()
        .zip(&lower)
        .filter(|(_, l)| {
            !lower.iter().any(|b| {
                l.strip_prefix(b.as_str())
                    .and_then(|r| r.strip_prefix(' '))
                    .is_some_and(special_edition)
            })
        })
        .map(|(n, _)| n.clone())
        .collect()
}

/// The names an option stands for: `any ship` for the catch-all selector, else its
/// matched vehicles' display names (placeholders dropped).
fn option_names(o: &SpawnOption, lk: &Lookup) -> Vec<String> {
    if o.selector.iter().all(|p| p == ANY_SHIP) && !o.selector.is_empty() {
        return vec!["any ship".to_string()];
    }
    let mut out = Vec::new();
    for class in &o.ships {
        if let Some(n) = lk.ship_name(class) {
            push_new(&mut out, tidy(n));
        }
    }
    out
}

/// One line's value over the groups of one or more contracts (`per`: each contract's
/// groups under this line). Counts add up within a contract and range over contracts.
fn render(per: &[Vec<&SpawnGroup>], lk: &Lookup, m: &Mission) -> String {
    let (mut lo, mut hi) = (f64::INFINITY, 0.0_f64);
    // (weight rank, name): names of heavier options first.
    let mut named: Vec<(f64, String)> = Vec::new();
    let mut mostly: Vec<String> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let (mut sizes, mut damage) = (Vec::new(), Vec::new());
    let mut cargo: Vec<(u8, String)> = Vec::new();
    let mut skills: Vec<f64> = Vec::new();
    for groups in per {
        let (mut c_lo, mut c_hi) = (0.0, 0.0);
        for g in groups {
            for slot in &g.slots {
                let amounts = slot.options.iter().map(|o| o.amount);
                c_lo += amounts.clone().fold(f64::INFINITY, f64::min);
                c_hi += amounts.fold(0.0, f64::max);
                let total: f64 = slot.options.iter().map(|o| o.weight.max(0.0)).sum();
                let uneven = slot
                    .options
                    .iter()
                    .any(|o| o.weight != slot.options[0].weight);
                for o in &slot.options {
                    let names = option_names(o, lk);
                    // Unknown tag ids alone: "picked by the game", never the ids.
                    if names.is_empty() && !o.selector.is_empty() {
                        push_new(&mut unresolved, selector_text(&o.selector));
                    }
                    let dominant = uneven && total > 0.0 && o.weight / total >= 0.5;
                    for n in names {
                        if dominant {
                            push_new(&mut mostly, n.clone());
                        }
                        match named.iter_mut().find(|(_, x)| x.eq_ignore_ascii_case(&n)) {
                            Some(e) => e.0 = e.0.max(o.weight),
                            None => named.push((o.weight, n)),
                        }
                    }
                    for s in o.selector.iter().filter_map(|p| size_word(p)) {
                        push_new(&mut sizes, s);
                    }
                    if let Some(d) = o.damage.as_deref().and_then(damage_word) {
                        push_new(&mut damage, d);
                    }
                    for a in &o.applied {
                        // A hostile's hold is loot at best: left out to keep its line short.
                        if g.role != SpawnRole::Hostile
                            && let Some(c) = cargo_word(a)
                            && !cargo.contains(&c)
                        {
                            cargo.push(c);
                        }
                        skills.extend(skill(a));
                    }
                }
            }
        }
        lo = lo.min(c_lo);
        hi = hi.max(c_hi);
    }
    named.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    let names = without_editions(named.into_iter().map(|(_, n)| n).collect());
    let mostly: Vec<String> = mostly.into_iter().filter(|n| names.contains(n)).collect();
    let mut out = String::new();
    if hi > 1.0 {
        let n = if int(lo) == int(hi) {
            int(hi)
        } else {
            format!("{}–{}", int(lo), int(hi))
        };
        out.push_str(&format!("{n} ships · "));
    }
    let cap = m.max_list.min(SHIP_NAMES);
    if names.is_empty() {
        let from: Vec<String> = unresolved.into_iter().filter(|s| !s.is_empty()).collect();
        if from.is_empty() {
            out.push_str("picked by the game");
        } else {
            out.push_str(&format!("picked by the game from {}", list_cut(&from, 3)));
        }
    } else if !mostly.is_empty() && mostly.len() < names.len() {
        let rest: Vec<String> = names
            .iter()
            .filter(|n| !mostly.contains(n))
            .cloned()
            .collect();
        let first = list_cut(&mostly, cap);
        let left = cap.saturating_sub(mostly.len()).max(1);
        out.push_str(&format!("mostly {first}; or {}", list_cut(&rest, left)));
    } else {
        out.push_str(&list_cut(&names, cap));
    }
    let mut extras: Vec<String> = Vec::new();
    if !sizes.is_empty() {
        extras.push(sizes.join("/"));
    }
    extras.extend(damage);
    if !cargo.is_empty() {
        cargo.sort_by_key(|(order, _)| *order);
        let words: Vec<String> = cargo.into_iter().map(|(_, w)| w).collect();
        extras.push(format!("cargo: {}", words.join(", ")));
    }
    if let (Some(a), Some(b)) = (
        skills.iter().copied().reduce(f64::min),
        skills.iter().copied().reduce(f64::max),
    ) {
        extras.push(if int(a) == int(b) {
            format!("pilot skill {}", int(a))
        } else {
            format!("pilot skill {}–{}", int(a), int(b))
        });
    }
    if !extras.is_empty() {
        out.push_str(&format!(" ({})", extras.join(", ")));
    }
    out
}

/// A line's label: role, then the group name when the role has several over the whole
/// description group.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    role: &'static str,
    group: Option<String>,
}

impl Key {
    /// `Hostiles`, `Hostiles (Wave 1)`, `Hostiles (Wave 1; Easy, Hard)`: one bracket.
    fn text(&self, variants: Option<&str>) -> String {
        let inner: Vec<&str> = self.group.as_deref().into_iter().chain(variants).collect();
        if inner.is_empty() {
            self.role.to_string()
        } else {
            format!("{} ({})", self.role, inner.join("; "))
        }
    }
}

/// The SPAWNS section body lines for a description's contract group; empty when none
/// spawns ships.
pub(super) fn lines(group: &[&Contract], lk: &Lookup, m: &Mission) -> Vec<String> {
    let label = |t: &str| super::profile::em(m.label_em, &format!("{t}:"));
    // Group names per role over the whole description group: a name is shown only when a
    // role has more than one.
    let mut names_of: Vec<(&'static str, Vec<Option<String>>)> = Vec::new();
    for g in group.iter().flat_map(|c| &c.spawns) {
        let role = role_label(g);
        let name = group_label(g);
        match names_of.iter_mut().find(|(r, _)| *r == role) {
            Some((_, ns)) => {
                if !ns.contains(&name) {
                    ns.push(name);
                }
            }
            None => names_of.push((role, vec![name])),
        }
    }
    let key_of = |g: &SpawnGroup| {
        let role = role_label(g);
        let several = names_of
            .iter()
            .find(|(r, _)| *r == role)
            .is_some_and(|(_, ns)| ns.len() > 1);
        Key {
            role,
            group: if several { group_label(g) } else { None },
        }
    };
    let mut keys: Vec<Key> = Vec::new();
    for g in group.iter().flat_map(|c| &c.spawns) {
        let k = key_of(g);
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    // Stable: first-seen order within a role.
    keys.sort_by_key(|k| role_order(k.role));
    let mut out = Vec::new();
    for key in &keys {
        let per: Vec<Vec<&SpawnGroup>> = group
            .iter()
            .map(|c| c.spawns.iter().filter(|g| key_of(g) == *key).collect())
            .collect();
        let values: Vec<Option<String>> = per
            .iter()
            .map(|gs| (!gs.is_empty()).then(|| render(std::slice::from_ref(gs), lk, m)))
            .collect();
        let present: Vec<Option<&String>> = values.iter().map(Option::as_ref).collect();
        let first = present.iter().flatten().next().copied();
        let all_same = present.iter().flatten().all(|v| Some(*v) == first);
        if all_same && present.iter().all(Option::is_some) {
            out.push(format!(
                "{} {}",
                label(&key.text(None)),
                first.cloned().unwrap_or_default()
            ));
            continue;
        }
        let index = |c: &Contract| group.iter().position(|x| std::ptr::eq(*x, c));
        let value = |c: &Contract| index(c).and_then(|i| values[i].clone());
        match per_variant(group, lk, None, value) {
            // Variants without this group are left off.
            Some(vs) => out.extend(vs.values.iter().filter(|(v, _)| v.as_str() != ABSENT).map(
                |(v, ls)| {
                    let vl = list_cut(ls, m.max_list);
                    format!("{} {v}", label(&key.text(Some(&vl))))
                },
            )),
            // Nothing tells the variants apart: one line over those that have it.
            None => {
                let with: Vec<Vec<&SpawnGroup>> =
                    per.into_iter().filter(|gs| !gs.is_empty()).collect();
                out.push(format!(
                    "{} {}",
                    label(&key.text(None)),
                    render(&with, lk, m)
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(humanize("AvailableToSalvage"), "Available to salvage");
        assert_eq!(humanize("Wave1"), "Wave 1");
        assert_eq!(
            humanize("EscortReinforcementsWave01"),
            "Escort reinforcements wave 01"
        );
        assert_eq!(humanize("Hornet_F7C"), "Hornet F7C");
        assert_eq!(humanize("LargeCombatShip"), "Large combat ship");
        assert_eq!(humanize("Wave <1>~%"), "Wave 1");
        assert_eq!(tidy(" RSI Aurora Mk I  LX\u{a0}"), "RSI Aurora Mk I LX");
        assert_eq!(
            size_word("Missions/MissionType/Salvage/AvailableToSalvage/Smallest").as_deref(),
            Some("smallest")
        );
        assert_eq!(
            size_word("Missions/MissionType/Refueling/AvalibleToRefuel/RefuelVerySmall").as_deref(),
            Some("very small")
        );
        assert_eq!(
            size_word("Missions/MissionType/Salvage/AvailableToSalvage"),
            None
        );
        assert_eq!(
            damage_word("salvagecontractor_lightdamagecoverup").as_deref(),
            Some("light damage")
        );
        assert_eq!(
            damage_word("capitalship_highdamage").as_deref(),
            Some("high damage")
        );
        assert_eq!(
            selector_text(&["Missions/MissionType/Salvage/AvailableToSalvage/Smallest".into()]),
            "Salvage › Available to salvage › Smallest"
        );
        assert_eq!(
            selector_text(&[
                "AI/Faction/Criminal".into(),
                "AI/Ship/CombatClass/Easy".into(),
                "Ship/Model/Freelancer".into(),
                "Ship/Series/Freelancer".into()
            ]),
            "Criminal, Easy, Freelancer"
        );
        assert_eq!(
            cargo_word("AI/CargoManifest/General/Legality/Ilegal"),
            Some((0, "illegal".into()))
        );
        assert_eq!(
            cargo_word("AI/CargoManifest/General/PopulationVariation/Full Cargo"),
            Some((2, "full cargo".into()))
        );
        assert_eq!(
            skill("AI/SkillDefinitions/Description/HumanPilot50"),
            Some(50.0)
        );
    }

    #[test]
    fn special_editions_give_way_to_the_base_name() {
        let got = without_editions(vec![
            "Anvil Carrack".into(),
            "Anvil Carrack Best In Show Edition".into(),
            "Drake Cutlass Black".into(),
            "Drake Cutlass Black Teach's Special".into(),
            "Origin 600i Exec".into(),
            "Origin 600i Explorer".into(),
            "MISC Prospector Wikelo Work Special".into(),
        ]);
        assert_eq!(
            got,
            [
                "Anvil Carrack",
                "Drake Cutlass Black",
                "Origin 600i Exec",
                "Origin 600i Explorer",
                "MISC Prospector Wikelo Work Special"
            ]
        );
    }
}
