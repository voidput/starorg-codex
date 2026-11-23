//! The Loot screen's trees on a real cached build. `STARORG_LOOT_DIR`: a
//! `~/.cache/starorg-codex/mcp/<channel>/<build>` folder holding `game-item-tags.json`,
//! `game-loot.json` and `game-item-sources.json` (what "Read game data" caches). Prints every
//! area with its tree path and the row count of every node, and checks the naming rules
//! hold on every real id: unique names among siblings, no preset words, at most one middle
//! level, no level holding a single row.
//!
//! `STARORG_LOOT_DIR=... cargo test -p starorg-mcp --test loot_real -- --ignored --nocapture`

use serde_json::Value;
use starorg_mcp::loot::input::{Datasets, ItemTags, LootArea, LootData, build_loot_index};
use starorg_mcp::loot::{LootScreen, LootSummary, LootView};
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
    check(&s.list(LootView::Items).rows, "items");
}
