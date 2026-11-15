//! Local integration tests against a real game install (spec 13, "Local integration").
//!
//! Ignored by default: they need the game. Run with
//!
//! ```sh
//! STARORG_GAME_DIR="/path/to/StarCitizen" cargo test --release -p starorg-data --test real_archives -- --ignored --nocapture
//! ```
//!
//! Without `STARORG_GAME_DIR` the default library of the Linux prefix
//! `/mnt/games/star-citizen` is used. Every installed channel is checked. Read only.

use starorg_data::eocd::CountCheck;
use starorg_data::extract;
use std::path::PathBuf;

fn game_dir() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
}

fn channels() -> Vec<starorg_core::discovery::Channel> {
    let dir = game_dir();
    let ch = starorg_core::discovery::list_channels(&dir);
    assert!(!ch.is_empty(), "no channels under {}", dir.display());
    ch
}

#[test]
#[ignore = "needs a game install"]
fn every_channel_passes_the_entry_count_check() {
    for c in channels() {
        let t = std::time::Instant::now();
        let (archive, check) =
            extract::open_checked(&c.p4k).unwrap_or_else(|e| panic!("{}: {e}", c.name));
        println!(
            "{}: {} entries parsed, check {:?} ({:?})",
            c.name,
            archive.parsed_entries(),
            check,
            t.elapsed()
        );
        assert!(matches!(check, CountCheck::Matches(n) if n == archive.parsed_entries() as u64));
    }
}

#[test]
#[ignore = "needs a game install"]
fn every_channel_extracts() {
    for c in channels() {
        let t = std::time::Instant::now();
        let x = extract::extract(&c.p4k).unwrap_or_else(|e| panic!("{}: {e}", c.name));
        let ini = starorg_core::ini::StockIni::parse(x.base_ini.clone());
        println!(
            "{}: global.ini {} bytes, {} keys; {} = {} records; warnings {:?} ({:?})",
            c.name,
            x.base_ini.len(),
            ini.key_count(),
            x.dcb_entry,
            x.datacore.records,
            x.warnings,
            t.elapsed()
        );
        assert!(
            ini.has_bom(),
            "{}: stock global.ini starts with the BOM",
            c.name
        );
        assert!(
            ini.key_count() >= 50_000,
            "{}: at least 50,000 keys",
            c.name
        );
        assert!(x.datacore.records > 0);
        assert_eq!(
            ini.render(&starorg_core::ini::Edits::new()),
            x.base_ini,
            "{}: round trip",
            c.name
        );
    }
}
