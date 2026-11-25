//! The Loot screen's trees on a real cached build. `STARORG_LOOT_DIR`: a
//! `~/.cache/starorg-codex/mcp/<channel>/<build>` folder holding `game-item-tags.json`,
//! `game-loot.json` and `game-item-sources.json` (what "Read game data" caches). Prints every
//! area with its tree path and the row count of every node, and checks the naming rules
//! hold on every real id: unique names among siblings, no preset words, at most one middle
//! level, no level holding a single row. Then every detail in groups: the headers count
//! every drop (past the row caps), and the default answer stays small; prints the groups of
//! the biggest area and of `ASD facilities > General (classic)`.
//!
//! `STARORG_LOOT_DIR=... cargo test -p starorg-mcp --test loot_real -- --ignored --nocapture`

use serde_json::Value;
use starorg_mcp::loot::input::{Datasets, ItemTags, LootArea, LootData, build_loot_index};
use starorg_mcp::loot::{
    DetailQuery, LootGroupBy, LootScreen, LootSummary, LootView, MAX_DROPS, MAX_GROUP_ROWS,
};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

fn json(dir: &Path, name: &str) -> Value {
    let t = std::fs::read_to_string(dir.join(format!("{name}.json"))).unwrap();
    serde_json::from_str(&t).unwrap()
}

/// Rows per node (every prefix of every row's path).
fn node_counts(rows: &[LootSummary]) -> BTreeMap<String, usize> {
    let mut n: BTreeMap<String, usize> = BTreeMap::new();
    for r in rows {
        for d in 1..=r.path.len() {
            *n.entry(r.path[..d].join(" > ")).or_default() += 1;
        }
    }
    n
}

fn check(rows: &[LootSummary], view: &str) {
    let mut seen = HashSet::new();
    for r in rows {
        let mut p = r.path.clone();
        p.push(r.name.clone());
        assert!(
            seen.insert(p.clone()) || view == "items",
            "two areas at {p:?}"
        );
        let lower = r.name.to_lowercase();
        assert!(
            view == "items" || (!r.name.contains('_') && !lower.contains("slotpreset")),
            "{}",
            r.name
        );
        // Worn gear: kind -> faction -> role; everything else at most one middle level.
        let deep = if starorg_mcp::loot::is_gear(&r.id) {
            3
        } else {
            2
        };
        assert!(r.path.len() <= deep, "{p:?}");
    }
    for (node, n) in node_counts(rows) {
        assert!(n >= 2, "{view}: {node} holds one row");
        println!("NODE\t{view}\t{node}\t{n}");
    }
}

#[test]
#[ignore = "needs a cached loot build in STARORG_LOOT_DIR"]
fn real_loot_tree() {
    let dir = std::env::var("STARORG_LOOT_DIR").expect("STARORG_LOOT_DIR");
    let dir = Path::new(&dir);
    let items: Vec<ItemTags> =
        serde_json::from_value(json(dir, "game-item-tags")["items"].clone()).unwrap();
    let loot: LootData = serde_json::from_value(json(dir, "game-loot")).unwrap();
    let areas: Vec<LootArea> =
        serde_json::from_value(json(dir, "game-item-sources")["areas"].clone()).unwrap();
    let d = Datasets {
        items,
        loot: Some(loot),
        areas: Some(areas),
        ..Default::default()
    };
    let index = build_loot_index(d.loot.as_ref().unwrap());
    let s = LootScreen::build(&d, Some(&index), Some("real".into()));
    let a = s.list(LootView::Areas).rows;
    for r in &a {
        let mut p = r.path.clone();
        p.push(r.name.clone());
        println!("AREA\t{}\t{}\t{}", p.join(" > "), r.count, r.id);
    }
    check(&a, "areas");
    let items = s.list(LootView::Items).rows;
    check(&items, "items");
    groups(&s, LootView::Areas, &a);
    groups(&s, LootView::Items, &items);
    let classic = a
        .iter()
        .find(|r| {
            r.path.first().map(String::as_str) == Some("ASD facilities")
                && r.name == "General (classic)"
        })
        .or_else(|| a.iter().max_by_key(|r| r.count))
        .unwrap();
    // NPC bodies: each slot with its best rows.
    for r in a.iter().filter(|r| starorg_mcp::loot::is_npc(&r.id)) {
        let d = s.detail(LootView::Areas, &r.id, &DetailQuery::default());
        for g in &d.groups {
            let top: Vec<String> = g
                .rows
                .iter()
                .flatten()
                .take(3)
                .map(|x| format!("{} {:.4}", x.name, x.odds))
                .collect();
            println!(
                "NPCSLOT\t{}\t{}\t{}\t{:.4}\t{}",
                r.name,
                g.label,
                g.count,
                g.best,
                top.join(" | ")
            );
        }
    }
    let biggest = a.iter().max_by_key(|r| r.count).unwrap();
    for r in [classic, biggest] {
        let t = std::time::Instant::now();
        let d = s.detail(LootView::Areas, &r.id, &DetailQuery::default());
        let took = t.elapsed();
        let mut p = r.path.clone();
        p.push(r.name.clone());
        println!(
            "GROUPS\t{}\t{} drops\t{} groups\t{} rows sent\t{took:?}",
            p.join(" > "),
            d.total,
            d.groups.len(),
            d.rows().count()
        );
        for g in &d.groups {
            println!(
                "GROUP\t{}\t{}\t{:.4}\t{}",
                g.label,
                g.count,
                g.best,
                if g.rows.is_some() { "open" } else { "" }
            );
        }
    }
}

/// The worn-gear rows on a real build: `game-npc-loadouts.json` in `STARORG_LOOT_DIR` too
/// (or `STARORG_GEAR_FILE`). The tree rules of [`real_loot_tree`] hold with them, every
/// detail adds up, and the Ace pilot reads as in the game files. Prints every gear row
/// and the Ace pilot's slots.
///
/// `STARORG_LOOT_DIR=... cargo test -p starorg-mcp --test loot_real real_worn_gear -- --ignored --nocapture`
#[test]
#[ignore = "needs a cached loot build and game-npc-loadouts.json in STARORG_LOOT_DIR"]
fn real_worn_gear() {
    use starorg_mcp::loot::input::NpcLoadouts;
    use starorg_mcp::loot::{GEAR_KIND, LootDropKind, is_gear};
    let dir = std::env::var("STARORG_LOOT_DIR").expect("STARORG_LOOT_DIR");
    let dir = Path::new(&dir);
    let gear_file = std::env::var("STARORG_GEAR_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dir.join("game-npc-loadouts.json"));
    let gear: NpcLoadouts = serde_json::from_slice(&std::fs::read(&gear_file).unwrap()).unwrap();
    // No condition contradicts itself (`X and not X`), and the single-port armour slots
    // never hold more than one item's worth unconditionally (one pick per port).
    let mut gated = 0;
    for a in &gear.actors {
        for i in &a.items {
            for r in &i.requires {
                let parts: Vec<&str> = r.split(" and ").collect();
                assert!(
                    !parts
                        .iter()
                        .any(|p| parts.contains(&format!("not {p}").as_str())),
                    "{}: {} {r}",
                    a.class,
                    i.item
                );
            }
            gated += usize::from(!i.requires.is_empty());
        }
        for slot in ["Undersuit", "Helmet", "Torso", "Arms", "Legs", "Backpack"] {
            let sum: f64 = a
                .items
                .iter()
                .filter(|i| i.slot == slot && i.requires.is_empty())
                .map(|i| i.chance)
                .sum();
            assert!(sum <= 1.0 + 1e-9, "{}: {slot} sums to {sum}", a.class);
        }
    }
    println!("GATED\t{gated} items with a condition");
    let items: Vec<ItemTags> =
        serde_json::from_value(json(dir, "game-item-tags")["items"].clone()).unwrap();
    let loot: LootData = serde_json::from_value(json(dir, "game-loot")).unwrap();
    let areas: Vec<LootArea> =
        serde_json::from_value(json(dir, "game-item-sources")["areas"].clone()).unwrap();
    let d = Datasets {
        items,
        loot: Some(loot),
        areas: Some(areas),
        ..Default::default()
    };
    let index = build_loot_index(d.loot.as_ref().unwrap());
    let t = std::time::Instant::now();
    let s = LootScreen::build_with_gear(&d, Some(&index), Some(&gear), Some("real".into()));
    println!("BUILT in {:?}", t.elapsed());
    let a = s.list(LootView::Areas).rows;
    let worn: Vec<&LootSummary> = a.iter().filter(|r| is_gear(&r.id)).collect();
    for r in &worn {
        let mut p = r.path.clone();
        p.push(r.name.clone());
        println!("GEAR\t{}\t{}\t{}", p.join(" > "), r.count, r.id);
    }
    check(&a, "areas");
    let items = s.list(LootView::Items).rows;
    check(&items, "items");
    let gear_rows: Vec<LootSummary> = worn.iter().map(|r| (*r).clone()).collect();
    groups(&s, LootView::Areas, &gear_rows);
    let worn_items: Vec<LootSummary> = items
        .iter()
        .filter(|r| {
            s.detail(LootView::Items, &r.id, &DetailQuery::default())
                .group(GEAR_KIND)
                .is_some()
        })
        .cloned()
        .collect();
    groups(&s, LootView::Items, &worn_items);
    println!(
        "COUNTS\tactors {}\tgear rows {}\titems worn {}\titems listed {}",
        gear.actors.len(),
        worn.len(),
        worn_items.len(),
        items.len()
    );
    // The Ace pilot: its suit, helmet, core and SMG, each always there.
    let ace = worn
        .iter()
        .find(|r| r.id == "gear:PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_AcePilot")
        .expect("ace pilot row");
    let mut p = ace.path.clone();
    p.push(ace.name.clone());
    println!("ACE\t{}\t{}", p.join(" > "), ace.id);
    let d = s.detail(LootView::Areas, &ace.id, &DetailQuery::default());
    let npc = d.npc.as_ref().unwrap();
    println!(
        "ACE\tclasses {:?}\tfaction {:?}\twear {:?}\tbody {:?}",
        npc.classes, npc.faction, npc.wear, npc.body_loot
    );
    for g in &d.groups {
        for r in g.rows.iter().flatten() {
            println!(
                "ACESLOT\t{}\t{}\t{}\t{:.4}\t{}",
                g.label,
                r.name,
                r.id,
                r.odds,
                r.note.as_deref().unwrap_or("")
            );
        }
    }
    let suit = d
        .rows()
        .find(|r| r.id == "basl_combat_light_flightsuit_02_01_01")
        .unwrap();
    assert_eq!(suit.name, "Ace Interceptor Racing Flight Suit");
    assert_eq!(suit.kind, LootDropKind::Worn);
    for c in [
        "basl_combat_light_helmet_02_01_01",
        "rsi_explorer_armor_light_core_01_01_20",
        "volt_smg_energy_01",
    ] {
        assert!(d.rows().any(|r| r.id == c), "{c}");
    }
    assert!(
        npc.body_loot
            .as_ref()
            .is_some_and(|l| l.id.eq_ignore_ascii_case("npc:LootTable_Actor_AcePilot"))
    );
    // By item: the suit's places hold the ace pilot under the worn-gear kind.
    let it = s.detail(
        LootView::Items,
        "basl_combat_light_flightsuit_02_01_01",
        &DetailQuery::default(),
    );
    let g = it.group(GEAR_KIND).expect("worn-gear group");
    assert!(g.rows.iter().flatten().any(|r| r.id == ace.id));
}

/// Every detail of a list in groups: headers cover every drop, rows stay under the caps,
/// groups are best first.
fn groups(s: &LootScreen, view: LootView, rows: &[LootSummary]) {
    let modes: &[LootGroupBy] = match view {
        LootView::Areas => &[LootGroupBy::Container, LootGroupBy::None],
        LootView::Items => &[
            LootGroupBy::Place,
            LootGroupBy::Container,
            LootGroupBy::None,
        ],
    };
    for r in rows {
        for &m in modes {
            let d = s.detail(
                view,
                &r.id,
                &DetailQuery {
                    group_by: Some(m),
                    ..DetailQuery::default()
                },
            );
            assert_eq!(d.total, r.count, "{}", r.id);
            assert_eq!(d.groups.iter().map(|g| g.count).sum::<usize>(), r.count);
            // A gear row's slots come in body order; every other list best first.
            let slots = m == LootGroupBy::Container && starorg_mcp::loot::is_gear(&r.id);
            if !slots {
                assert!(d.groups.windows(2).all(|w| w[0].best >= w[1].best));
                assert!((d.groups[0].best - r.best).abs() < 1e-12);
            }
            assert!(d.groups[0].rows.is_some());
            let cap = if m == LootGroupBy::None {
                MAX_DROPS
            } else {
                MAX_GROUP_ROWS
            };
            for g in &d.groups {
                if let Some(rows) = &g.rows {
                    assert_eq!(rows.len(), g.count.min(cap), "{} {}", r.id, g.label);
                }
            }
        }
    }
}
