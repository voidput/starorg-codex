//! Where each row of the Loot screen's lists sits in its tree, and its name there.
//!
//! Areas: kind of place (`Caves`, from [`AREA_PLACES`]) -> a middle level from the first id
//! word several areas of that place share (`Occupied`, `Aberdeen`, `Cargo wing`, `Colonial`),
//! when one of them says more than that word -> the area, named by its remaining id words (`Acidic Pyro`, `High security`) or `General`
//! when none are left. Preset words (`lootgeneration`, `v3slotpreset`, ...) never show; a small
//! word map splits run-together and misspelt words (`highsecurtiy` -> `high security`). Names
//! are unique among siblings: two areas that read the same get the preset family that tells
//! them apart (`General (v3)`, `General (multi)`), then their id numbers, then the id words
//! the others lack.
//!
//! Items: category (`FPS weapons`) -> what kind of thing (`Rifles`, `Helmets`, `Magazines`, from
//! the item's type, sub type and weapon tags) -> the item.
//!
//! Neither tree adds a level that holds a single child: a place with one area is that area,
//! and a middle level is made only when it holds two or more rows and not all of its parent's.

use crate::hints::AREA_PLACES;
use starorg_gamedata::query::types::ItemTags;
use std::collections::HashMap;

/// Preset words: dropped from names; some say the preset family.
const NOISE: &[&str] = &[
    "lootgeneration",
    "slotpreset",
    "multislotpreset",
    "v3slotpreset",
    "multiconfig",
    "harvestable",
    "slotpresets",
    "loot",
];

/// Run-together and misspelt id words as readable words. One entry stays one tree word, so
/// `cargoshop_high` and `cargowing_high` do not share a `cargo` level.
const WORDS: &[(&str, &str)] = &[
    ("highsecurtiy", "high security"),
    ("highsecurity", "high security"),
    ("mediumsecurity", "medium security"),
    ("lowsecurity", "low security"),
    ("foodanddrink", "food and drink"),
    ("justboxes", "just boxes"),
    ("100percent", "100 percent"),
    ("sciencewing", "science wing"),
    ("cargowing", "cargo wing"),
    ("cargoshop", "cargo shop"),
    ("shippingwing", "shipping wing"),
    ("sideroad", "side road"),
    ("largeroom", "large room"),
    ("mediumroom", "medium room"),
    ("smallroom", "small room"),
    ("lowerdeck", "lower deck"),
    ("rewardroom", "reward room"),
    ("ghostarena", "ghost arena"),
    ("hightech", "high tech"),
    ("lowtech", "low tech"),
    ("holidaybox", "holiday box"),
    ("exec", "executive"),
    ("ext", "exterior"),
    ("int", "interior"),
];

/// Place words that also say which one; the rest of the word stays as the area's first word
/// (`techoutpost` -> `tech`, beside `colonial` from `colonialoutpost`).
const PLACE_QUALIFIERS: &[(&str, &str)] = &[
    ("colonialoutpost", "colonial"),
    ("techoutpost", "tech"),
    ("spacederelict", "space"),
    ("dishalignmentfacility", "dish alignment facility"),
    ("dcdelving", "delving"),
];

/// Id words that only repeat the place (`orb` of `orb_orbitalplatform_ext`), by place.
const PLACE_NOISE: &[(&str, &str)] = &[
    ("Orbital platforms", "orb"),
    ("Orbageddon", "orb"),
    ("Rock Cracker, Nyx", "rockcracker"),
];

/// Words written in capitals.
const ACRONYMS: &[&str] = &["tsg", "soo", "asd", "ugf", "lmg", "smg"];

/// Proper names that keep their capital inside a name (`Acidic Pyro`).
const PROPER: &[&str] = &[
    "stanton", "pyro", "nyx", "aberdeen", "daymar", "hurston", "kareah", "rund", "indy",
];

/// The kind of place of areas no [`AREA_PLACES`] word names.
pub const OTHER: &str = "Other";

/// How a slot preset was written: the only thing telling some areas apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    V3,
    Multi,
    Classic,
    Plain,
}

impl Family {
    fn of(lower: &[String]) -> Self {
        let has = |w: &str| lower.iter().any(|x| x == w);
        if has("v3slotpreset") {
            Self::V3
        } else if has("multislotpreset") || has("multiconfig") {
            Self::Multi
        } else if has("slotpreset") || has("lootgeneration") {
            Self::Classic
        } else {
            Self::Plain
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::V3 => "v3",
            Self::Multi => "multi",
            Self::Classic => "classic",
            Self::Plain => "base",
        }
    }
}

/// One area id taken apart.
#[derive(Debug, Clone)]
struct Parsed {
    place: String,
    /// Readable words after the place (lower case; one entry may hold spaces).
    words: Vec<String>,
    /// Number words (`001`), kept apart: they name nothing unless needed to tell areas apart.
    nums: Vec<String>,
    family: Family,
    /// Every non-preset id word, for the last resort of telling areas apart.
    raw: Vec<String>,
}

/// The kind of place: the first [`AREA_PLACES`] entry a word starts with, else the mined
/// place ([`starorg_gamedata::query::mined::area_place`]). The key of the hit, if any.
fn place_of(area: &str, lower: &[String]) -> (Option<String>, Option<&'static str>) {
    let hit = AREA_PLACES
        .iter()
        .find(|(k, _)| lower.iter().any(|w| w.starts_with(k)));
    match hit {
        Some((k, label)) => (Some((*label).to_string()), Some(*k)),
        None => (
            starorg_gamedata::query::mined::area_place(area).map(str::to_string),
            None,
        ),
    }
}

/// A word in the context of its place: `high` in a distribution centre is its security,
/// `1a` in a contested zone is a zone.
fn in_place(place: &str, w: &str) -> String {
    if place == "Distribution centres" && matches!(w, "high" | "medium" | "low") {
        return format!("{w} security");
    }
    if place == "Contested zones"
        && w.len() >= 2
        && w[..w.len() - 1].bytes().all(|b| b.is_ascii_digit())
        && w.as_bytes()[w.len() - 1].is_ascii_alphabetic()
    {
        return format!("zone {w}");
    }
    w.to_string()
}

fn parse(area: &str) -> Parsed {
    let lower: Vec<String> = area
        .split(['/', '_', '-'])
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let family = Family::of(&lower);
    let (place, key) = place_of(area, &lower);
    let place = place.unwrap_or_else(|| OTHER.to_string());
    let noise = PLACE_NOISE
        .iter()
        .find(|(p, _)| *p == place)
        .map(|(_, n)| *n);
    let mut words: Vec<String> = Vec::new();
    let mut nums = Vec::new();
    let mut raw = Vec::new();
    for w in lower.iter().filter(|w| !NOISE.contains(&w.as_str())) {
        raw.push(w.clone());
        let w = w.as_str();
        if key.is_some_and(|k| w.starts_with(k)) {
            if let Some((_, q)) = PLACE_QUALIFIERS.iter().find(|(p, _)| *p == w) {
                words.push((*q).to_string());
            }
            continue;
        }
        if noise.is_some_and(|n| w.starts_with(n)) {
            continue;
        }
        if w.bytes().all(|b| b.is_ascii_digit()) {
            nums.push(w.to_string());
            continue;
        }
        let w = WORDS.iter().find(|(k, _)| *k == w).map_or(w, |(_, v)| *v);
        let w = in_place(&place, w);
        if words.last() != Some(&w) {
            words.push(w);
        }
    }
    Parsed {
        place,
        words,
        nums,
        family,
        raw,
    }
}

/// An area's kind of place (`Caves`), else [`OTHER`].
pub fn area_place(area: &str) -> String {
    parse(area).place
}

/// Readable words as a name: the first letter in capitals, as are acronyms, proper names
/// and zone letters (`zone 1a reward room` -> `Zone 1A reward room`).
fn sentence(words: &[String]) -> String {
    let mut out = Vec::new();
    for (i, phrase) in words.iter().enumerate() {
        for (j, w) in phrase.split(' ').enumerate() {
            let first = i == 0 && j == 0;
            let lw = w.to_ascii_lowercase();
            let s = if ACRONYMS.contains(&lw.as_str())
                || (w.len() >= 2 && w.as_bytes()[0].is_ascii_digit())
            {
                w.to_ascii_uppercase()
            } else if first || PROPER.contains(&lw.as_str()) {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            } else {
                w.to_string()
            };
            out.push(s);
        }
    }
    out.join(" ")
}

/// Leaf labels of one parent's children, unique: same labels get, in turn, their preset
/// family (when that differs), their id numbers, then the id words the others lack.
fn unique_labels(base: Vec<String>, parsed: &[&Parsed]) -> Vec<String> {
    let mut labels = base;
    for step in 0..4 {
        let mut same: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, l) in labels.iter().enumerate() {
            same.entry(l.as_str()).or_default().push(i);
        }
        let clashes: Vec<Vec<usize>> = same.into_values().filter(|v| v.len() > 1).collect();
        if clashes.is_empty() {
            break;
        }
        let mut next = labels.clone();
        for group in clashes {
            for &i in &group {
                let p = parsed[i];
                let extra = match step {
                    0 => {
                        let f = p.family;
                        group
                            .iter()
                            .any(|&o| parsed[o].family != f)
                            .then(|| f.label().to_string())
                    }
                    1 => (!p.nums.is_empty() && group.iter().any(|&o| parsed[o].nums != p.nums))
                        .then(|| p.nums.join(" ")),
                    2 => {
                        let own: Vec<&String> = p
                            .raw
                            .iter()
                            .filter(|w| group.iter().any(|&o| !parsed[o].raw.contains(w)))
                            .collect();
                        (!own.is_empty())
                            .then(|| own.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))
                    }
                    // The same id words in another case: count them.
                    _ => Some((group.iter().position(|&o| o == i).unwrap_or(0) + 1).to_string()),
                };
                if let Some(e) = extra {
                    next[i] = format!("{} ({e})", labels[i]);
                }
            }
        }
        labels = next;
    }
    labels
}

/// Each area's tree path (its ancestors, top first; empty for a place with one area) and its
/// name there, in the order of `areas`.
pub fn area_tree(areas: &[&str]) -> Vec<(Vec<String>, String)> {
    let parsed: Vec<Parsed> = areas.iter().map(|a| parse(a)).collect();
    let mut out: Vec<(Vec<String>, String)> = vec![(Vec::new(), String::new()); areas.len()];
    let mut by_place: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, p) in parsed.iter().enumerate() {
        by_place.entry(p.place.as_str()).or_default().push(i);
    }
    for (place, members) in by_place {
        if let [only] = members.as_slice() {
            let p = &parsed[*only];
            let name = if p.words.is_empty() {
                place.to_string()
            } else {
                format!("{place}: {}", sentence(&p.words))
            };
            out[*only] = (Vec::new(), name);
            continue;
        }
        // Per first word: how many areas start with it, and whether any says more.
        let mut firsts: HashMap<&str, (usize, bool)> = HashMap::new();
        for &i in &members {
            let w = &parsed[i].words;
            if let Some(f) = w.first() {
                let e = firsts.entry(f.as_str()).or_default();
                e.0 += 1;
                e.1 |= w.len() > 1;
            }
        }
        // (middle, leaf words) per member
        let mut parents: HashMap<Option<String>, Vec<(usize, Vec<String>)>> = HashMap::new();
        for &i in &members {
            let w = &parsed[i].words;
            let mid = w
                .first()
                .filter(|f| {
                    let (n, more) = firsts[f.as_str()];
                    more && (2..members.len()).contains(&n)
                })
                .cloned();
            let leaf = if mid.is_some() {
                w[1..].to_vec()
            } else {
                w.clone()
            };
            parents.entry(mid).or_default().push((i, leaf));
        }
        for (mid, kids) in parents {
            let base: Vec<String> = kids
                .iter()
                .map(|(_, w)| {
                    if w.is_empty() {
                        "General".to_string()
                    } else {
                        sentence(w)
                    }
                })
                .collect();
            let ps: Vec<&Parsed> = kids.iter().map(|(i, _)| &parsed[*i]).collect();
            let labels = unique_labels(base, &ps);
            let mut path = vec![place.to_string()];
            if let Some(m) = &mid {
                path.push(sentence(std::slice::from_ref(m)));
            }
            for ((i, _), l) in kids.iter().zip(labels) {
                out[*i] = (path.clone(), l);
            }
        }
    }
    out
}

/// An FPS weapon's class from its `Weapon/FPS/...` tags, else its sub type.
fn weapon_class(item: &ItemTags) -> &'static str {
    let parts = |t: &String| -> Vec<String> {
        t.strip_prefix("Weapon/FPS/")
            .map(|r| r.split('/').map(str::to_string).collect())
            .unwrap_or_default()
    };
    // The most specific tag first: `Pistol/Gadget` before `Pistol`.
    let mut best: Option<(usize, &'static str)> = None;
    for t in &item.tags {
        let p = parts(t);
        let p: Vec<&str> = p.iter().map(String::as_str).collect();
        let hit = match p.as_slice() {
            ["Stocked", "Rifle", ..] => Some("Rifles"),
            ["Stocked", "SMG", ..] => Some("SMGs"),
            ["Stocked", "Shotgun", ..] => Some("Shotguns"),
            ["Stocked", "SniperRifle", ..] => Some("Sniper rifles"),
            ["Stocked", "LMG", ..] => Some("LMGs"),
            ["Stocked", "GrenadeLauncher", ..] => Some("Grenade launchers"),
            ["Stocked", "Gadget", ..] | ["Pistol", "Gadget", ..] => Some("Gadgets"),
            ["Stocked", "Special", ..] => Some("Special weapons"),
            ["Pistol", ..] => Some("Pistols"),
            ["Melee", ..] => Some("Knives"),
            ["Shouldered", ..] | ["HeavyHip", ..] => Some("Heavy weapons"),
            ["Grenade", ..] => Some("Grenades"),
            _ => None,
        };
        if let Some(h) = hit
            && best.is_none_or(|(n, _)| p.len() > n)
        {
            best = Some((p.len(), h));
        }
    }
    if let Some((_, h)) = best {
        return h;
    }
    match item.sub_type.as_deref().unwrap_or("") {
        "Gadget" => "Gadgets",
        "Grenade" => "Grenades",
        "Knife" => "Knives",
        "Small" => "Pistols",
        "Large" => "Heavy weapons",
        _ => "Other weapons",
    }
}

/// What kind of thing an item is inside its category (`Rifles`, `Helmets`, `Magazines`,
/// `Power plants`), from its type, sub type and weapon tags; `None` when the data does not
/// say (`UNDEFINED`).
pub fn item_kind(item: &ItemTags) -> Option<String> {
    let ty = item.ty.as_deref()?;
    let sub = item.sub_type.as_deref().unwrap_or("");
    let s = match ty {
        "WeaponPersonal" => weapon_class(item),
        "WeaponAttachment" => match sub {
            "Barrel" => "Barrel attachments",
            "BottomAttachment" => "Underbarrel attachments",
            "IronSight" => "Sights",
            "Magazine" => "Magazines",
            "Utility" => "Utility attachments",
            _ => "Other attachments",
        },
        "Char_Armor_Helmet" => "Helmets",
        "Char_Armor_Torso" => "Torso armour",
        "Char_Armor_Arms" => "Arm armour",
        "Char_Armor_Legs" => "Leg armour",
        "Char_Armor_Undersuit" => "Undersuits",
        "Char_Armor_Backpack" => "Backpacks",
        "Char_Clothing_Feet" => "Footwear",
        "Char_Clothing_Hands" => "Gloves",
        "Char_Clothing_Hat" => "Hats",
        "Char_Clothing_Legs" => "Legwear",
        "Char_Clothing_Torso_0" => "Shirts",
        "Char_Clothing_Torso_1" => "Jackets & coats",
        "Char_Accessory_Eyes" => "Eyewear",
        "Drink" => "Drinks",
        "Food" => "Food",
        "FPS_Consumable" => "Medical & utility",
        "Cooler" => "Coolers",
        "FlightController" => "Flight controllers",
        "PowerPlant" => "Power plants",
        "QuantumDrive" => "Quantum drives",
        "Radar" => "Radars",
        "Shield" => "Shields",
        "Battery" => "Batteries",
        "Missile" => match sub {
            "Torpedo" => "Torpedoes",
            "Rocket" => "Rockets",
            _ => "Missiles",
        },
        "Cargo" => "Cargo",
        "Container" | "InventoryContainer" => "Containers",
        "Currency" => "Currency",
        "Gadget" => "Gadgets",
        "RemovableChip" => "Chips",
        "Misc" => match sub {
            "Harvestable" => "Harvestables",
            "Trophy" => "Trophies",
            _ => "Miscellaneous",
        },
        "UNDEFINED" | "" => return None,
        other => return Some(spaced(other)),
    };
    Some(s.to_string())
}

/// `MiningModifier` -> `Mining modifier`.
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
            out.push(if out.is_empty() {
                c
            } else {
                c.to_ascii_lowercase()
            });
        }
        prev = Some(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Item tree paths from (category, kind) per item: the kind level only where it holds two
/// or more items and not all of the category's; a category with one item is that item.
pub fn item_paths(rows: &[(String, Option<String>)]) -> Vec<Vec<String>> {
    let mut per_cat: HashMap<&str, usize> = HashMap::new();
    let mut per_kind: HashMap<(&str, &str), usize> = HashMap::new();
    for (c, k) in rows {
        *per_cat.entry(c.as_str()).or_default() += 1;
        if let Some(k) = k {
            *per_kind.entry((c.as_str(), k.as_str())).or_default() += 1;
        }
    }
    rows.iter()
        .map(|(c, k)| {
            let n = per_cat[c.as_str()];
            if n == 1 {
                return Vec::new();
            }
            match k {
                Some(k) if (2..n).contains(&per_kind[&(c.as_str(), k.as_str())]) => {
                    vec![c.clone(), k.clone()]
                }
                _ => vec![c.clone()],
            }
        })
        .collect()
}

/// The kind of place NPC rows sit under in the areas list.
pub const NPC_KIND: &str = "NPCs (body loot)";

/// Id of an NPC row: this prefix and the actor loot table's name (never a slot preset path).
pub const NPC_PREFIX: &str = "npc:";

/// Whether an areas-list id is an NPC's.
pub fn is_npc(id: &str) -> bool {
    id.starts_with(NPC_PREFIX)
}

/// The words of an actor loot table's name after `Actor_` (`V3LootTable_Actor_SOO_MendoRen`
/// -> `SOO_MendoRen`); `None` for other tables.
fn actor_rest(table: &str) -> Option<&str> {
    let lower = table.to_ascii_lowercase();
    let at = lower.find("actor_")?;
    let head = &lower[..at];
    // `LootTable_Actor_`, `V3LootTable_Actor_`: the word before is the table family.
    (head.is_empty() || head.ends_with("loottable_")).then(|| &table[at + "actor_".len()..])
}

/// An NPC body's loot table (`*LootTable_Actor_*`) that is a real NPC's: not a `_TEMPLATE`
/// (designer templates no record points to; their unique-item slot is empty).
pub fn is_actor_table(table: &str) -> bool {
    actor_rest(table).is_some() && !table.to_ascii_lowercase().contains("template")
}

/// The NPC groups, in list order.
pub const NPC_PILOTS: &str = "Pilots";
pub const NPC_GROUND: &str = "Ground combat";
pub const NPC_PRISONERS: &str = "Prisoners";
pub const NPC_BOSSES: &str = "Bosses";

/// Known NPCs by the lower-case words after `Actor_`: (group, name).
const NPCS: &[(&str, &str, &str)] = &[
    ("acepilot", NPC_PILOTS, "Ace pilot"),
    ("grunt", NPC_GROUND, "Grunt"),
    ("soldier", NPC_GROUND, "Soldier"),
    ("sniper", NPC_GROUND, "Sniper"),
    ("cqc", NPC_GROUND, "CQC fighter"),
    ("grenadier", NPC_GROUND, "Grenadier"),
    ("juggernaut", NPC_GROUND, "Juggernaut"),
    ("techie", NPC_GROUND, "Techie"),
    ("largeammo", NPC_GROUND, "Large-ammo carrier"),
    ("prisoner", NPC_PRISONERS, "Prisoner"),
    ("soo_artkelvin", NPC_BOSSES, "Art Kelvin"),
    ("soo_mendoren", NPC_BOSSES, "Mendo Ren"),
    ("soo_remykettle", NPC_BOSSES, "Remy Kettle"),
    (
        "soo_juggernaut_elite",
        NPC_BOSSES,
        "Juggernaut (elite, SOO)",
    ),
    (
        "largeammo_rockcracker_boss",
        NPC_BOSSES,
        "Rock Cracker boss",
    ),
];

/// The variant suffix that adds a council scrip slot to an NPC's table.
const COUNCIL_SCRIP: &str = "_councilscrip";

/// An actor table's NPC group and readable name: a known NPC ([`NPCS`]), else the table's
/// words (`SOO_NewBoss` -> `Bosses`, `New boss (SOO)`). A `_CouncilScrip` table is its own
/// NPC row (the data does not say which NPCs roll which of the two), named after the base
/// NPC: `Soldier (with council scrip)`.
pub fn npc_name(table: &str) -> (String, String) {
    let rest = actor_rest(table).unwrap_or(table);
    let lower = rest.to_ascii_lowercase();
    let (base, scrip) = match lower.strip_suffix(COUNCIL_SCRIP) {
        Some(b) => (b.to_string(), true),
        None => (lower.clone(), false),
    };
    let (group, name) = match NPCS.iter().find(|(k, _, _)| *k == base) {
        Some((_, g, n)) => ((*g).to_string(), (*n).to_string()),
        None => {
            let raw = &rest[..base.len().min(rest.len())];
            let soo = base.split('_').any(|w| w == "soo");
            let words: Vec<String> = raw
                .split('_')
                .filter(|w| !w.eq_ignore_ascii_case("soo"))
                .map(spaced)
                .filter(|w| !w.is_empty())
                .collect();
            let mut name = sentence(&words.iter().map(|w| w.to_lowercase()).collect::<Vec<_>>());
            if name.is_empty() {
                name = rest.to_string();
            }
            if soo {
                name.push_str(" (SOO)");
            }
            let group = if soo || base.contains("boss") {
                NPC_BOSSES
            } else if base.contains("pilot") {
                NPC_PILOTS
            } else if base.contains("prisoner") {
                NPC_PRISONERS
            } else {
                NPC_GROUND
            };
            (group.to_string(), name)
        }
    };
    let name = if scrip {
        format!("{name} (with council scrip)")
    } else {
        name
    };
    (group, name)
}

/// A readable name for a slot of an NPC's table: V3 slots name themselves (`Ammo`); a v1
/// slot is its archetype, named without the archetype and NPC words
/// (`LootArchetype_Actor_AcePilot_Medals` of `LootTable_Actor_AcePilot` -> `Medals`).
pub fn npc_slot_label(table: &str, label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        return "Body loot".to_string();
    }
    if !label.contains('_') {
        return label.to_string();
    }
    let own: Vec<String> = actor_rest(table)
        .unwrap_or("")
        .split('_')
        .map(str::to_ascii_lowercase)
        .collect();
    let words: Vec<String> = label
        .split('_')
        .map(str::to_ascii_lowercase)
        .filter(|w| {
            !w.is_empty()
                && !matches!(w.as_str(), "lootarchetype" | "v3lootarchetype" | "actor")
                && !own.contains(w)
        })
        .collect();
    if words.is_empty() {
        label.to_string()
    } else {
        sentence(&words)
    }
}

/// Each NPC row's tree path and name, in the order of `tables`: [`NPC_KIND`] -> its group
/// -> the NPC, without a group level that holds one NPC (or all of them); a list with one
/// NPC is that NPC (`NPCs (body loot): Ace pilot`).
pub fn npc_tree(tables: &[&str]) -> Vec<(Vec<String>, String)> {
    let named: Vec<(String, String)> = tables.iter().map(|t| npc_name(t)).collect();
    if let [(_, name)] = named.as_slice() {
        return vec![(Vec::new(), format!("{NPC_KIND}: {name}"))];
    }
    let mut per_group: HashMap<&str, usize> = HashMap::new();
    for (g, _) in &named {
        *per_group.entry(g.as_str()).or_default() += 1;
    }
    named
        .iter()
        .map(|(g, name)| {
            let n = per_group[g.as_str()];
            let path = if (2..named.len()).contains(&n) {
                vec![NPC_KIND.to_string(), g.clone()]
            } else {
                vec![NPC_KIND.to_string()]
            };
            (path, name.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npc_tables_and_names() {
        assert!(is_actor_table("LootTable_Actor_AcePilot"));
        assert!(is_actor_table("V3LootTable_Actor_CQC_CouncilScrip"));
        assert!(!is_actor_table("V3LootTable_Actor_TEMPLATE"));
        assert!(!is_actor_table(
            "V3LootTable_Actor_Soldier_UniqueItem_TEMPLATE"
        ));
        assert!(!is_actor_table("V3LootTable_Medical_Common"));
        assert!(!is_actor_table("Crate_Factor_Weapons"));
        let n = |t: &str| npc_name(t);
        assert_eq!(
            n("LootTable_Actor_AcePilot"),
            ("Pilots".into(), "Ace pilot".into())
        );
        assert_eq!(
            n("V3LootTable_Actor_Soldier_CouncilScrip"),
            (
                "Ground combat".into(),
                "Soldier (with council scrip)".into()
            )
        );
        assert_eq!(n("V3LootTable_Actor_SOO_MendoRen").1, "Mendo Ren");
        assert_eq!(
            n("V3LootTable_Actor_SOO_Juggernaut_Elite").1,
            "Juggernaut (elite, SOO)"
        );
        assert_eq!(
            n("V3LootTable_Actor_LargeAmmo_RockCracker_Boss").0,
            "Bosses"
        );
        // Unknown tables read from their words.
        assert_eq!(
            n("V3LootTable_Actor_SOO_NewBoss"),
            ("Bosses".into(), "New boss (SOO)".into())
        );
        assert_eq!(
            n("V3LootTable_Actor_RiotGuard"),
            ("Ground combat".into(), "Riot guard".into())
        );
    }

    #[test]
    fn npc_tree_drops_single_row_groups() {
        let t = npc_tree(&[
            "LootTable_Actor_AcePilot",
            "V3LootTable_Actor_Grunt",
            "V3LootTable_Actor_Soldier",
            "V3LootTable_Actor_SOO_MendoRen",
        ]);
        assert_eq!(t[0], (vec![NPC_KIND.to_string()], "Ace pilot".to_string()));
        assert_eq!(t[1].0, [NPC_KIND, "Ground combat"]);
        assert_eq!(t[3].0, [NPC_KIND]);
        // All in one group: no group level.
        let t = npc_tree(&["V3LootTable_Actor_Grunt", "V3LootTable_Actor_Soldier"]);
        assert_eq!(t[0].0, [NPC_KIND]);
        let l = |a: &str| npc_slot_label("LootTable_Actor_AcePilot", a);
        assert_eq!(l("LootArchetype_Actor_AcePilot_Medals"), "Medals");
        assert_eq!(l("Unique item"), "Unique item");
        assert_eq!(l(" "), "Body loot");
        let t = npc_tree(&["LootTable_Actor_AcePilot"]);
        assert_eq!(
            t[0],
            (Vec::new(), "NPCs (body loot): Ace pilot".to_string())
        );
    }

    fn tree(ids: &[&str]) -> Vec<String> {
        area_tree(ids)
            .into_iter()
            .map(|(p, n)| {
                let mut p = p;
                p.push(n);
                p.join(" > ")
            })
            .collect()
    }

    #[test]
    fn typo_and_run_together_words_read() {
        let t = tree(&[
            "v3slotpreset_distributioncentres_highsecurtiy",
            "v3slotpreset_distributioncentres_lowsecurity",
            "lootgeneration_slotpreset_foodanddrink",
            "lootgeneration_slotpreset_shippingcontainer_justboxes",
            "lootgeneration_slotpreset_shippingcontainer_ammo",
            "distributioncentre_multiconfig_100percent",
        ]);
        assert_eq!(
            t,
            [
                "Distribution centres > High security",
                "Distribution centres > Low security",
                "Other: Food and drink",
                "Shipping containers > Just boxes",
                "Shipping containers > Ammo",
                "Distribution centres > 100 percent",
            ]
        );
    }

    #[test]
    fn noise_only_ids_never_show_the_raw_id() {
        let t = tree(&[
            "lootgeneration_slotpreset",
            "v3slotpreset_loot_generic",
            "lootgeneration_slotpreset_generic",
        ]);
        assert_eq!(
            t,
            [
                "Other > General",
                "Other > Generic (v3)",
                "Other > Generic (classic)"
            ]
        );
        // Alone in its place: the place's name.
        assert_eq!(
            tree(&["lootgeneration_slotpreset_derelictoutpost"]),
            ["Derelict outposts"]
        );
        assert_eq!(tree(&["v3slotpreset_battaglia"]), ["Battaglia"]);
    }

    /// Representative real LIVE 4.10 ids (see `tests/loot_real.rs` for the whole list).
    const REAL: &[&str] = &[
        "asddelving_multiconfig",
        "lootgeneration_slotpreset_asddelving",
        "v3slotpreset_asddelving",
        "v3slotpreset_asddelving_sciencewing",
        "asteroidbases/asteroidbase_dungeon_001",
        "asteroidbases/asteroidbase_exec_hangar_001",
        "lootgeneration_slotpreset_asteroidbase_pyro_encounter",
        "cave_aberdeen_rich",
        "cave_aberdeen_poor",
        "cave_daymar_rich",
        "lootgeneration_slotpreset_caves",
        "caves/loot_caves_occupied",
        "caves/loot_caves_occupied_acidic_pyro",
        "caves/loot_caves_occupied_rock_stanton",
        "cave_prison_harvestables",
        "contestedzones/contestedzone_1a",
        "contestedzones/contestedzone_1a_rewardroom_001",
        "contestedzones/contestedzone_1b_rewardroom_002",
        "v3slotpreset_contestedzones",
        "contestedzones/ghostarena_01",
        "distributioncentre_multiconfig",
        "distributioncentre_multiconfig_cargowing",
        "distributioncentre_multiconfig_cargowing_high",
        "distributioncentre_multiconfig_cargoshop_low",
        "v3slotpreset_dcdelving",
        "lootgeneration_multislotpreset_derelictoutposts",
        "lootgeneration_slotpreset_derelictoutpost",
        "lootgeneration_slotpreset_colonialoutpost",
        "lootgeneration_slotpreset_colonialoutpost_indy_large",
        "lootgeneration_slotpreset_techoutpost",
        "orbageddon/orb_warehouse_multiconfig",
        "lootgeneration_slotpreset_orbageddon",
        "v3slotpreset_orbageddon",
        "orb_dishalignmentfacility",
        "orb_orbitalplatform_ext",
        "lootgeneration_multislotpreset_stations",
        "lootgeneration_slotpreset_station",
        "lootgeneration_slotpreset_station_rund",
        "lootgeneration_slotpreset_station_rund_lowerdeck",
        "v3slotpreset_rockcracker",
        "v3slotpreset_rockcracker_sandbox",
        "special_holidaybox",
        "special_scrips",
        "v3slotpreset_tsg",
    ];

    #[test]
    fn real_ids_get_tree_paths() {
        let t = tree(REAL);
        let want = [
            "ASD facilities > General (multi)",
            "ASD facilities > General (classic)",
            "ASD facilities > General (v3)",
            "ASD facilities > Science wing",
            "Asteroid bases > Dungeon",
            "Asteroid bases > Executive hangar",
            "Asteroid bases > Pyro encounter",
            "Caves > Aberdeen > Rich",
            "Caves > Aberdeen > Poor",
            "Caves > Daymar rich",
            "Caves > General",
            "Caves > Occupied > General",
            "Caves > Occupied > Acidic Pyro",
            "Caves > Occupied > Rock Stanton",
            "Caves > Prison harvestables",
            "Contested zones > Zone 1A > General",
            "Contested zones > Zone 1A > Reward room",
            "Contested zones > Zone 1B reward room",
            "Contested zones > General",
            "Contested zones > Ghost arena",
            "Distribution centres > General",
            "Distribution centres > Cargo wing > General",
            "Distribution centres > Cargo wing > High security",
            "Distribution centres > Cargo shop low security",
            "Distribution centres > Delving",
            "Derelict outposts > General (multi)",
            "Derelict outposts > General (classic)",
            "Outposts > Colonial > General",
            "Outposts > Colonial > Indy large",
            "Outposts > Tech",
            "Orbageddon > Warehouse",
            "Orbageddon > General (classic)",
            "Orbageddon > General (v3)",
            "Orbital platforms > Dish alignment facility",
            "Orbital platforms > Exterior",
            "Stations > General (multi)",
            "Stations > General (classic)",
            "Stations > Rund > General",
            "Stations > Rund > Lower deck",
            "Rock Cracker, Nyx > General",
            "Rock Cracker, Nyx > Sandbox",
            "Other > Special > Holiday box",
            "Other > Special > Scrips",
            "Other > TSG",
        ];
        assert_eq!(t, want);
    }

    #[test]
    fn sibling_names_are_unique() {
        let all = area_tree(REAL);
        let mut seen = std::collections::HashSet::new();
        for (p, n) in &all {
            assert!(seen.insert((p.clone(), n.clone())), "{p:?} {n}");
            assert!(!n.contains('_') && !n.contains("slotpreset"), "{n}");
        }
        // Same words, numbers apart; same family and numbers: the id words.
        let t = tree(&[
            "asteroidbases/asteroidbase_dungeon_001",
            "asteroidbases/asteroidbase_dungeon_002",
            "lootgeneration_slotpreset_ugfs_alpha",
            "lootgeneration_slotpreset_ugfs_beta_alpha",
            "lootgeneration_slotpreset_ugfs_beta",
            "lootgeneration_slotpreset_ugfs_beta_b",
        ]);
        assert_eq!(
            t,
            [
                "Asteroid bases > Dungeon (001)",
                "Asteroid bases > Dungeon (002)",
                "Underground facilities > Alpha",
                "Underground facilities > Beta > Alpha",
                "Underground facilities > Beta > General",
                "Underground facilities > Beta > B",
            ]
        );
    }

    fn item(ty: &str, sub: &str, tags: &[&str]) -> ItemTags {
        ItemTags {
            ty: Some(ty.into()),
            sub_type: (!sub.is_empty()).then(|| sub.into()),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn item_kinds_split_categories() {
        let kind = |ty, sub, tags: &[&str]| item_kind(&item(ty, sub, tags));
        assert_eq!(
            kind(
                "WeaponPersonal",
                "Medium",
                &["Weapon/FPS/Stocked", "Weapon/FPS/Stocked/SniperRifle"]
            )
            .as_deref(),
            Some("Sniper rifles")
        );
        assert_eq!(
            kind(
                "WeaponPersonal",
                "Small",
                &["Weapon/FPS/Pistol", "Weapon/FPS/Pistol/Gadget"]
            )
            .as_deref(),
            Some("Gadgets")
        );
        assert_eq!(
            kind("WeaponPersonal", "Knife", &[]).as_deref(),
            Some("Knives")
        );
        assert_eq!(
            kind("WeaponAttachment", "IronSight", &[]).as_deref(),
            Some("Sights")
        );
        assert_eq!(
            kind("Char_Clothing_Torso_1", "", &[]).as_deref(),
            Some("Jackets & coats")
        );
        assert_eq!(
            kind("MiningModifier", "Gun", &[]).as_deref(),
            Some("Mining modifier")
        );
        assert_eq!(kind("UNDEFINED", "", &[]), None);
        let rows = |v: &[(&str, Option<&str>)]| -> Vec<(String, Option<String>)> {
            v.iter()
                .map(|(c, k)| (c.to_string(), k.map(str::to_string)))
                .collect()
        };
        let p = item_paths(&rows(&[
            ("FPS weapons", Some("Rifles")),
            ("FPS weapons", Some("Rifles")),
            ("FPS weapons", Some("Pistols")),
            ("Mining & salvage", Some("Mining modifier")),
            ("Mining & salvage", Some("Mining modifier")),
            ("Other", None),
        ]));
        assert_eq!(
            p,
            [
                vec!["FPS weapons", "Rifles"],
                vec!["FPS weapons", "Rifles"],
                vec!["FPS weapons"],
                vec!["Mining & salvage"],
                vec!["Mining & salvage"],
                vec![],
            ]
        );
    }
}
