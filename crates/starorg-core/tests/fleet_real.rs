//! Fleet matching against real game data (addendum §6.9 "Local integration").
//!
//! Ignored by default: it needs the extraction cache of an installed channel. Read only: it
//! reads `gamedata.json` and `base.ini` from the cache (`$STARORG_CACHE_DIR`, default
//! `~/.cache/starorg-strings`) and never touches the game. Run with
//!
//! ```sh
//! cargo test -p starorg-core --test fleet_real -- --ignored --nocapture
//! ```
//!
//! The fleet is synthetic: one HangarXPLOR JSON row per name of HangarXPLOR's public ship
//! table (MIT, `fixtures/fleet/hangarxplor-table.json`, made-up pledge ids). It includes
//! concept ships and retired store names, so some rows stay unmatched by design.

use starorg_core::fleet::import::{self, Tier};
use starorg_core::fleet::{Catalog, store::FleetFile};
use starorg_core::ini::StockIni;
use starorg_core::model::GameData;
use std::path::{Path, PathBuf};

fn cache_root() -> PathBuf {
    std::env::var_os("STARORG_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cache/starorg-strings")
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

fn catalog(dir: &Path) -> Catalog {
    let gd: GameData =
        serde_json::from_slice(&std::fs::read(dir.join("gamedata.json")).unwrap()).unwrap();
    let stock = StockIni::parse(std::fs::read(dir.join("base.ini")).unwrap());
    Catalog::build(&gd, &stock)
}

#[test]
#[ignore = "needs a cached extraction of an installed channel"]
fn public_ship_table_matches_at_least_the_trial() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fleet/hangarxplor-table.json");
    let parsed = import::parse(&std::fs::read(fixture).unwrap(), None).unwrap();
    let mut ran = 0;
    for ch in ["LIVE", "PTU", "HOTFIX"] {
        for dir in cached(ch) {
            let cat = catalog(&dir);
            let o = import::run(&parsed, &cat, &FleetFile::default(), false);
            let r = &o.report;
            let matched: usize = r.tiers.values().sum();
            println!(
                "{ch}: {} flyable hulls; {} rows: {matched} matched {:?}, {} ambiguous, {} unmatched",
                cat.flyable().count(),
                r.total,
                r.tiers,
                r.ambiguous.len(),
                r.unmatched.len()
            );
            for a in &r.ambiguous {
                println!("  ambiguous: {} -> {}", a.name, a.candidates.join(", "));
            }
            for u in &r.unmatched {
                println!("  unmatched: {}", u.name);
            }
            // Trial on LIVE before aliases (§6.3): 250 matched; aliases only add. The other
            // channels have fewer hulls (PTU 278, HOTFIX 281 vs LIVE 286).
            let floor = if ch == "LIVE" { 250 } else { 240 };
            assert!(
                matched >= floor,
                "{ch}: {matched} matched, at least {floor}"
            );
            assert!(r.tiers.get(&Tier::Exact).copied().unwrap_or(0) >= 200);
            ran += 1;
        }
    }
    assert!(
        ran > 0,
        "no cached extraction under {}",
        cache_root().display()
    );
}

#[test]
#[ignore = "needs a cached extraction of LIVE"]
fn every_alias_target_exists_in_live() {
    let dirs = cached("LIVE");
    assert!(!dirs.is_empty(), "no cached LIVE extraction");
    for dir in dirs {
        let cat = catalog(&dir);
        for (name, hull) in import::aliases() {
            let h = cat
                .get(&hull)
                .unwrap_or_else(|| panic!("alias {name:?}: {hull} not in LIVE"));
            assert!(h.flyable, "alias {name:?}: {hull} is not player-flyable");
        }
    }
}

#[test]
#[ignore = "needs a cached extraction of LIVE"]
fn odd_name_keys_and_short_spellings_are_caught() {
    let dirs = cached("LIVE");
    assert!(!dirs.is_empty(), "no cached LIVE extraction");
    for dir in dirs {
        let cat = catalog(&dir);
        let odd: Vec<&str> = cat
            .flyable()
            .filter(|h| !h.hull.to_ascii_lowercase().starts_with("vehicle_name"))
            .map(|h| h.hull.as_str())
            .collect();
        println!("player hulls named by other keys: {odd:?}");
        // The Wikelo hulls name themselves `TheCollector_ShipMod_*_VehicleName` (4.10).
        assert!(
            odd.iter().any(|k| k.starts_with("TheCollector_ShipMod_")),
            "{odd:?}"
        );
        for h in cat.flyable() {
            if h.hull.starts_with("TheCollector_ShipMod_") {
                assert!(h.short_key.is_some(), "{} has no short key", h.hull);
            }
        }
        let missing: Vec<&str> = cat
            .flyable()
            .filter(|h| h.short_key.is_none())
            .map(|h| h.hull.as_str())
            .collect();
        println!("flyable hulls without a short key: {missing:?}");
        assert!(
            odd.iter().all(|k| k
                .replace('_', "")
                .to_ascii_lowercase()
                .contains("vehiclename")),
            "{odd:?}"
        );
        for (hull, short) in [
            (
                "vehicle_NameAEGS_Firebird_Collector_Milt",
                "vehicle_NameAGES_Firebird_Collector_Milt_short",
            ),
            (
                "vehicle_NameKRIG_L22_Alpha_Wolf",
                "vehicle_NameKRIG_L-22_Alpha_Wolf_short",
            ),
        ] {
            if let Some(h) = cat.get(hull) {
                assert_eq!(h.short_key.as_deref(), Some(short), "{hull}");
            }
        }
    }
}
