//! Print shell without weapons (experimental) on a real LIVE install: the guns, their mounts
//! and the missiles leave a few ships' loadouts, their turret bodies, thrusters and paint stay.
//! Read only. Ignored by default:
//!
//! ```sh
//! cargo test --release -p starorg-data --test real_weapons -- --ignored --nocapture
//! ```
//!
//! `STARORG_GAME_DIR` as in `real_archives`; `STARORG_SHIPS` (comma separated) for other ships.

use starbreaker_datacore::database::Database;
use starbreaker_datacore::loadout::{EntityIndex, LoadoutNode, resolve_loadout_indexed};
use starorg_data::assets::model::strip_weapons;
use starorg_data::assets::{AssetArchive, find_record};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn live_p4k() -> PathBuf {
    std::env::var_os("STARORG_GAME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "/mnt/games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen",
            )
        })
        .join("LIVE")
        .join("Data.p4k")
}

fn names(n: &LoadoutNode, out: &mut BTreeMap<String, usize>) {
    *out.entry(n.entity_name.clone()).or_default() += 1;
    for c in &n.children {
        names(c, out);
    }
}

fn count(n: &LoadoutNode) -> usize {
    1 + n.children.iter().map(count).sum::<usize>()
}

fn has(m: &BTreeMap<String, usize>, part: &str) -> bool {
    m.keys().any(|k| k.contains(part))
}

#[test]
#[ignore]
fn weapons_leave_real_loadouts() {
    let a = AssetArchive::open(&live_p4k()).unwrap();
    let db: &Database<'static> = a.db().unwrap();
    let idx = EntityIndex::new(db);
    let ships = std::env::var("STARORG_SHIPS")
        .unwrap_or_else(|_| "RSI_Skylark,AEGS_Gladius,DRAK_Cutlass_Black,ANVL_Arrow".into());
    for ship in ships.split(',') {
        let r = find_record(db, ship).unwrap();
        let mut tree = resolve_loadout_indexed(&idx, r);
        let before_n = count(&tree.root);
        let mut before = BTreeMap::new();
        names(&tree.root, &mut before);
        let removed = strip_weapons(db, &mut tree);
        let mut after = BTreeMap::new();
        names(&tree.root, &mut after);
        assert_eq!(count(&tree.root) + removed, before_n, "{ship}");
        let gone: Vec<String> = before
            .iter()
            .filter_map(|(k, &n)| {
                let left = after.get(k).copied().unwrap_or(0);
                (n > left).then(|| format!("{k} x{}", n - left))
            })
            .collect();
        println!(
            "{ship}: {removed} of {before_n} nodes removed: {}",
            gone.join(", ")
        );
        assert!(removed > 0, "{ship}");
        for w in ["Mount_Gimbal", "LaserRepeater", "MISL_", "MRCK_"] {
            assert!(!has(&after, w), "{ship} keeps {w}");
        }
        assert!(has(&after, "Thruster"), "{ship} lost its thrusters");
        // A manned turret's body (`TurretBase`, `DRAK_Cutlass_SCItem_Turret_Black`) stays.
        assert!(
            !has(&before, "_SCItem_Turret") || has(&after, "_SCItem_Turret"),
            "{ship} lost a turret body"
        );
    }
}
