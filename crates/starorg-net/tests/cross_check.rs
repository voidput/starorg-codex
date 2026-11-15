//! Cross-check: the published dump of LIVE 4.10.200.58746 and local extraction of the same
//! build agree (ignored: needs the network and the real LIVE `Data.p4k`; reads only).

use starorg_core::build::BuildInfo;
use starorg_core::ini::StockIni;
use starorg_core::model::GameData;
use starorg_core::source::dump::{DumpCache, fetch_verified, recover_loc_keys, to_game_data};
use std::path::PathBuf;

fn live_dir() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
        .join("LIVE")
}

fn close(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a - b).abs() <= 1e-6 * a.abs().max(1.0),
        (None, None) => true,
        _ => false,
    }
}

#[test]
#[ignore = "network + real LIVE Data.p4k"]
fn dump_and_local_extraction_agree() {
    let dir = live_dir();
    let build = BuildInfo::read(&dir.join("build_manifest.id")).unwrap();
    if build.version != "4.10.200.58746" {
        eprintln!(
            "local LIVE is {}, not the cross-check build; skipped",
            build.version
        );
        return;
    }
    let cache_dir = tempfile::tempdir().unwrap();
    let (files, how) = fetch_verified(
        &starorg_net::GitHubTransport::new(),
        &DumpCache::new(cache_dir.path()),
        &build,
    )
    .unwrap();
    eprintln!(
        "dump {} at {} matched {how:?}",
        files.meta.tag, files.meta.commit
    );
    let mut dump: GameData = to_game_data(&files).unwrap();
    let local = starorg_data::extract::extract(&dir.join("Data.p4k")).unwrap();
    let stock = StockIni::parse(local.base_ini.clone());
    let (filled, total) = recover_loc_keys(&mut dump, &stock);
    eprintln!("loc keys recovered for {filled} of {total} dump records");
    let l = &local.game_data;

    // A few headline values, both sources.
    for gun in [
        "BEHR_BallisticGatling_S5",
        "GATS_BallisticGatling_S3",
        "KLWE_LaserRepeater_S3",
    ] {
        let (a, b) = (l.weapon(gun).unwrap(), dump.weapon(gun).unwrap());
        assert!(
            close(a.dps, b.dps) && close(a.alpha, b.alpha),
            "{gun}: {:?} vs {:?}",
            a.dps,
            b.dps
        );
        assert!(
            close(a.fire_rate, b.fire_rate) && close(a.range, b.range),
            "{gun}"
        );
        assert_eq!(
            a.item.loc.desc, b.item.loc.desc,
            "{gun}: recovered desc key"
        );
    }
    for ship in ["AEGS_Gladius", "ESPR_Prowler", "ANVL_Carrack"] {
        let (a, b) = (l.ship(ship).unwrap(), dump.ship(ship).unwrap());
        let (fa, fb) = (a.flight.as_ref().unwrap(), b.flight.as_ref().unwrap());
        assert_eq!(
            (fa.pitch, fa.yaw, fa.roll),
            (fb.pitch, fb.yaw, fb.roll),
            "{ship}"
        );
        assert_eq!(fa.scm_speed, fb.scm_speed, "{ship}");
        assert_eq!(a.totals, b.totals, "{ship}");
        assert_eq!(a.loc.desc, b.loc.desc, "{ship}: recovered desc key");
    }
    // Components: every stat local extraction keeps equals the dump's (the dump keeps a
    // few more that no generator reads).
    for c in &l.components {
        let Some(d) = dump.component(c.item.short_name()) else {
            continue;
        };
        assert_eq!(c.category, d.category, "{}", c.item.record_name);
        for (k, v) in &c.stats {
            assert!(
                close(Some(*v), d.stats.get(k).copied()),
                "{} {k}",
                c.item.record_name
            );
        }
        assert!(
            close(c.health, d.health) && close(c.power, d.power),
            "{}",
            c.item.record_name
        );
    }

    // Every record both sides have agrees on the stats the generators read.
    let mut checked = 0;
    for w in &l.weapons {
        if let Some(d) = dump.weapon(w.item.short_name()) {
            assert!(close(w.dps, d.dps), "{}", w.item.record_name);
            checked += 1;
        }
    }
    for s in &l.ships {
        if let Some(d) = dump.ship(s.short_name()) {
            assert_eq!(s.flight, d.flight, "{}", s.record_name);
            checked += 1;
        }
    }
    // Known dump defect (4.10.200.58746): vehicles renamed since 4.10.197 (Hammerhead ->
    // Hammerhead_GS, Andromeda -> Andromeda_GS, ...) are still in the dump.
    let stale: Vec<_> = dump
        .ships
        .iter()
        .filter(|s| l.ship(s.short_name()).is_none())
        .map(|s| s.short_name().to_string())
        .collect();
    eprintln!(
        "checked {checked} shared records; {} dump-only (stale) ships",
        stale.len()
    );
    assert!(l.ships.iter().all(|s| dump.ship(s.short_name()).is_some()));
}
