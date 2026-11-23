//! Missing descriptions against real game data (spec missing-names, "Descriptions").
//!
//! Ignored by default: it needs the extraction cache of an installed channel. Read only: it
//! reads `gamedata.json` and `base.ini` from the cache (`$STARORG_CACHE_DIR`, default
//! `~/.cache/starorg-codex`) and never touches the game. Run with
//!
//! ```sh
//! cargo test -p starorg-core --test descs_real -- --ignored --nocapture
//! ```

use starorg_core::enhance::descs::{self, RULE_SIBLING};
use starorg_core::enhance::names::{Curated, Resolver, Tier};
use starorg_core::enhance::{self, owned::NoneOwned, profile::Profile};
use starorg_core::ini::StockIni;
use starorg_core::model::GameData;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn cache_root() -> PathBuf {
    std::env::var_os("STARORG_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cache/starorg-codex")
        })
}

/// `<root>/<install>/<CHANNEL>/<key>/` folders holding a cached extraction.
fn cached(channel: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(installs) = std::fs::read_dir(cache_root()) else {
        return out;
    };
    for i in installs.flatten() {
        let Ok(keys) = std::fs::read_dir(i.path().join(channel)) else {
            continue;
        };
        for k in keys.flatten() {
            if k.path().join("gamedata.json").is_file() && k.path().join("base.ini").is_file() {
                out.push(k.path());
            }
        }
    }
    out
}

#[test]
#[ignore = "needs a cached extraction of an installed channel"]
fn sibling_descriptions_on_real_data() {
    for ch in ["LIVE", "PTU", "HOTFIX"] {
        for dir in cached(ch) {
            let gd: GameData =
                serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
            let stock = StockIni::parse(std::fs::read(dir.join("base.ini")).unwrap());
            let curated = Curated::new();
            let names = Resolver::new(&gd, &stock, &curated, true);
            let p = descs::plan(&gd, &stock, &names);
            let entities = |k: &str| {
                gd.item_names
                    .iter()
                    .filter(|n| {
                        n.desc
                            .as_deref()
                            .and_then(starorg_core::model::loc_key)
                            .is_some_and(|d| starorg_core::ini::fold_key(d) == k)
                    })
                    .count()
            };
            let filled: usize = p.fills.keys().map(|k| entities(k)).sum();
            let mut skipped: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
            for (why, records) in p.skipped.values() {
                let e = skipped.entry(why.name()).or_default();
                e.0 += 1;
                e.1 += records.len();
            }
            println!(
                "{ch}: {} keys filled ({filled} entities, {} renamed placeholders), skipped {:?}",
                p.fills.len(),
                p.fills.values().filter(|f| f.in_stock).count(),
                skipped
            );
            // Entities the game names itself (stock or curated name, not generated).
            let named = |records: &[String]| {
                records
                    .iter()
                    .filter(|r| {
                        gd.item_names.iter().any(|n| {
                            &n.class == *r
                                && matches!(
                                    names.resolve(Some(&n.name)),
                                    Some((_, Tier::Game | Tier::Curated))
                                )
                        })
                    })
                    .count()
            };
            let named_filled: usize = p.fills.values().map(|f| named(&f.records)).sum();
            let mut named_skipped: BTreeMap<&str, usize> = BTreeMap::new();
            for (why, records) in p.skipped.values() {
                *named_skipped.entry(why.name()).or_default() += named(records);
            }
            println!("  named entities: {named_filled} filled, skipped {named_skipped:?}");
            for (why, records) in p.skipped.values() {
                println!(
                    "  skip {}{}: {}",
                    why.name(),
                    if named(records) > 0 { " (named)" } else { "" },
                    records.join(", ")
                );
            }
            for f in p.fills.values() {
                println!("  fill {} <- {}: {}", f.key, f.sibling, f.value);
            }
            // Every fill passes the added-value rules and renders.
            let mut prof = Profile::off();
            prof.missing_names = true;
            let g = enhance::generate(&gd, &stock, &prof, &NoneOwned);
            let n = g.added.values().filter(|a| a.rule == RULE_SIBLING).count()
                + g.edits
                    .values()
                    .filter(|e| e.rule == Some(RULE_SIBLING))
                    .count();
            assert_eq!(n, p.fills.len(), "{ch}: every planned fill is written");
            enhance::render(&stock, &g).expect("whole-file rules pass");
        }
    }
}
