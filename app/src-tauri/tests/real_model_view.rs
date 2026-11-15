//! The Explorer's Model tab (addendum 7.20) against a real Star Citizen install, read only:
//! Print shell GLBs at LOD 4, 3 and 0 through the real `asset-worker`, into an asset cache in
//! the target's tmp folder, handed over in chunks. Ignored by default; run with
//! `SC_GAME_DIR=<folder that contains LIVE> [SC_APP=<release starorg-codex-app>] cargo test -p starorg-codex-app --test real_model_view -- --ignored --nocapture`.
//! Nothing is written into the game folder (checked).

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::assets::*;
use starorg_codex_app::contract::real::{
    AssetSpawner, DialogSlot, RealBackend, RealConfig, SourceFactory,
};
use starorg_codex_app::contract::types::*;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{Channel, DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::GameSource;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

struct NoSource;
impl SourceFactory for NoSource {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Err("no extraction here".into())
    }
}

struct App(PathBuf);
impl AssetSpawner for App {
    fn command(&self, ch: &Channel) -> Result<Command, String> {
        let mut c = Command::new(&self.0);
        c.arg("asset-worker").arg("--p4k").arg(&ch.p4k);
        Ok(c)
    }
}

fn noop(_: JobProgress) {}

#[test]
#[ignore = "needs a real install: SC_GAME_DIR"]
fn model_view_on_a_real_install() {
    let Some(game) = std::env::var_os("SC_GAME_DIR").map(PathBuf::from) else {
        eprintln!("SC_GAME_DIR not set; skipped");
        return;
    };
    let app = std::env::var_os("SC_APP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_starorg-codex-app")));
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let ctx = Ctx::new(AppDirs::under(t.path()), "0.0.0-test", "test");
    let b = RealBackend::new(RealConfig {
        ctx,
        discovery: DiscoveryRequest {
            game_dir: Some(game.clone()),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(NoSource),
        dialogs: DialogSlot::default(),
    })
    .with_asset_spawner(Arc::new(App(app)));
    let mut s = b.get_settings().unwrap();
    s.advanced.developer_mode = true;
    b.save_settings(s).unwrap();
    let p4k = game.join("LIVE/Data.p4k");
    let before = std::fs::metadata(&p4k).and_then(|m| m.modified()).ok();
    let target = ChannelRef {
        install: b.overview().unwrap().installs[0].id.clone(),
        channel: "LIVE".into(),
    };
    for ship in ["AEGS_Gladius", "AEGS_Javelin"] {
        let info = b
            .asset_model_info(AssetModelInfoRequest {
                target: target.clone(),
                what: ship.into(),
                name: ship.into(),
            })
            .unwrap();
        eprintln!(
            "{ship}: default LOD {}, needs {:?}",
            info.default_lod,
            info.lods.iter().map(|l| l.need).collect::<Vec<_>>()
        );
        for lod in [4, 3, 0] {
            let t0 = Instant::now();
            let v = b
                .asset_model_view(
                    AssetModelViewRequest {
                        target: target.clone(),
                        what: ship.into(),
                        name: ship.into(),
                        lod,
                        force: true,
                    },
                    &noop,
                )
                .unwrap();
            let export = t0.elapsed();
            let t1 = Instant::now();
            let mut n = 0u64;
            let mut head = Vec::new();
            while n < v.bytes {
                let c = b
                    .asset_model_chunk(AssetModelChunkRequest {
                        target: target.clone(),
                        what: ship.into(),
                        lod: v.lod,
                        offset: n,
                        len: v.chunk,
                    })
                    .unwrap();
                assert!(!c.is_empty());
                if head.is_empty() {
                    head = c[..4].to_vec();
                }
                n += c.len() as u64;
            }
            assert_eq!(head, b"glTF");
            eprintln!(
                "{ship} LOD {lod} -> {} (mesh LODs {:?}): {:.1} MB, {:?} triangles, cached {}, view {:?} (worker {:?} ms, peak {:?} MB), chunks {:?}",
                v.lod,
                v.available,
                v.bytes as f64 / 1_048_576.0,
                v.triangles,
                v.cached,
                export,
                v.ms,
                v.peak_rss.map(|b| b / 1_048_576),
                t1.elapsed()
            );
        }
        let after = b
            .asset_model_info(AssetModelInfoRequest {
                target: target.clone(),
                what: ship.into(),
                name: ship.into(),
            })
            .unwrap();
        assert!(after.lods[0].cached && after.lods[3].cached);
    }
    // Monotonic detail on the slider's steps (LOD 3 -> 0), and a Stop that comes before the
    // worker has the request (the job's first event) still stops it.
    for ship in ["AEGS_Gladius", "ANVL_Carrack", "ORIG_890Jump"] {
        let mut tris = Vec::new();
        for lod in [3, 2, 1, 0, 4] {
            let v = b
                .asset_model_view(
                    AssetModelViewRequest {
                        target: target.clone(),
                        what: ship.into(),
                        name: ship.into(),
                        lod,
                        force: true,
                    },
                    &noop,
                )
                .unwrap();
            tris.push((
                v.lod,
                v.triangles.unwrap_or(0),
                v.ms,
                v.peak_rss.map(|b| b / 1_048_576),
            ));
        }
        eprintln!("{ship}: (lod, triangles, ms, peak MB) {tris:?}");
        assert!(
            tris[..4].windows(2).all(|w| w[0].1 <= w[1].1),
            "LOD 3 -> 0 adds detail: {tris:?}"
        );
    }
    let stopped = b
        .asset_model_view(
            AssetModelViewRequest {
                target: target.clone(),
                what: "AEGS_Javelin".into(),
                name: "AEGS_Javelin".into(),
                lod: 1,
                force: true,
            },
            &|p: JobProgress| {
                if p.phase == JobPhase::Running && p.step <= 1 {
                    let _ = b.cancel_job(p.job.clone());
                }
            },
        )
        .unwrap_err();
    assert_eq!(stopped.code, ErrorCode::Cancelled, "{}", stopped.message);
    assert_eq!(
        std::fs::metadata(&p4k).and_then(|m| m.modified()).ok(),
        before,
        "the game folder is not written"
    );
}
