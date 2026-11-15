//! Full enhanced builds from the real archives (spec 13 "Local integration", spec 14 M2).
//!
//! Ignored by default: they need the game. Read only. Run with
//!
//! ```sh
//! cargo test --release -p starorg-data --test enhance_real -- --ignored --nocapture
//! ```
//!
//! For every installed channel: extraction, all generators with the Recommended preset, no
//! whole-file validation error, no dropped edit. With a recorded baseline
//! (`fixtures/coverage-<change>.json`) the written count of no category may fall more than
//! 2% below it, and on LIVE every category meets the M2 thresholds.

use starorg_core::enhance::owned::NoneOwned;
use starorg_core::enhance::profile::Profile;
use starorg_core::enhance::{Coverage, generate, render};
use starorg_core::ini::StockIni;
use std::path::{Path, PathBuf};

fn game_dir() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
}

fn baseline(change: Option<u64>) -> Option<Coverage> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(format!("coverage-{}.json", change?));
    let text = std::fs::read_to_string(p).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

#[test]
#[ignore = "needs a game install"]
fn every_channel_builds_clean() {
    let channels = starorg_core::discovery::list_channels(&game_dir());
    assert!(!channels.is_empty());
    for c in channels {
        let t = std::time::Instant::now();
        let x =
            starorg_data::extract::extract(&c.p4k).unwrap_or_else(|e| panic!("{}: {e}", c.name));
        let stock = StockIni::parse(x.base_ini);
        let g = generate(&x.game_data, &stock, &Profile::default(), &NoneOwned);
        let bytes = render(&stock, &g).unwrap_or_else(|e| panic!("{}: {e}", c.name));
        println!(
            "{}: {} edits, {} bytes, {:.2} s",
            c.name,
            g.edits.len(),
            bytes.len(),
            t.elapsed().as_secs_f64()
        );
        for (cat, v) in &g.coverage.categories {
            println!(
                "  {:<22} {:>5}/{:<5} {:?}",
                cat.name(),
                v.written,
                v.candidates,
                v.left
            );
        }
        let dropped: Vec<_> = g
            .warnings
            .iter()
            .filter(|w| w.kind == "dropped-edit")
            .collect();
        assert!(dropped.is_empty(), "{}: dropped edits {dropped:?}", c.name);
        let change = c.build.as_ref().and_then(|b| b.change);
        if let Some(base) = baseline(change) {
            let f = g.coverage.baseline_failures(&base);
            assert!(f.is_empty(), "{}: below baseline: {f:?}", c.name);
        }
        if c.name == "LIVE" {
            let f = g.coverage.threshold_failures();
            assert!(f.is_empty(), "LIVE: {f:?}");
        }
    }
}
