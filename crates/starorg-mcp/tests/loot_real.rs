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
        assert!(r.path.len() <= 2, "{p:?}");
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
            assert!(d.groups.windows(2).all(|w| w[0].best >= w[1].best));
            assert!((d.groups[0].best - r.best).abs() < 1e-12);
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
