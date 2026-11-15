//! The wizard's preview and Review dry run against a real Star Citizen install, read only
//! (Windows report: an empty preview column and a Review block that never resolved).
//! Ignored by default; run with
//! `SC_GAME_DIR=<folder that contains LIVE> SC_WORKER=<starorg-strings CLI> cargo test -p starorg-strings-app --test real_install -- --ignored --nocapture`.
//! App folders go to a temp folder; nothing is written into the game folder (checked).

use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::{GameSource, local::LocalExtraction};
use starorg_strings_app::contract::Backend;
use starorg_strings_app::contract::real::{DialogSlot, RealBackend, RealConfig, SourceFactory};
use starorg_strings_app::contract::types::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// The CLI binary as the extraction worker (the app uses itself; a test binary cannot).
struct CliWorker(PathBuf);

impl SourceFactory for CliWorker {
    fn source(&self, dirs: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        let mut s = LocalExtraction::new(&self.0).with_keyrefs();
        s.work_root = Some(dirs.cache.join("work"));
        Ok(Box::new(s))
    }
}

fn ini_mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

#[test]
#[ignore = "needs a real install: SC_GAME_DIR and SC_WORKER"]
fn wizard_preview_and_review_on_a_real_install() {
    let (Some(game), Some(worker)) = (
        std::env::var_os("SC_GAME_DIR"),
        std::env::var_os("SC_WORKER"),
    ) else {
        eprintln!("SC_GAME_DIR / SC_WORKER not set; skipped");
        return;
    };
    let game = PathBuf::from(game);
    let t = tempfile::tempdir().unwrap();
    let dirs = AppDirs::under(t.path());
    let ctx = Ctx::new(
        dirs,
        "0.0.0-test",
        starorg_data::starbreaker::STARBREAKER_COMMIT,
    );
    let b = RealBackend::new(RealConfig {
        ctx,
        discovery: DiscoveryRequest {
            game_dir: Some(game.clone()),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(CliWorker(PathBuf::from(worker))),
        dialogs: DialogSlot::default(),
    });
    let ini = game.join("LIVE/Data/Localization/english/global.ini");
    let before = ini_mtime(&ini);

    let inst = b.overview().unwrap().installs.remove(0);
    let target = ChannelRef {
        install: inst.id.clone(),
        channel: "LIVE".into(),
    };
    let preview = || {
        b.build_preview(PreviewRequest {
            target: target.clone(),
            profile: None,
            preset: Some(Preset::Recommended),
        })
    };
    let t0 = Instant::now();
    let first = preview().unwrap();
    eprintln!(
        "preview before setup: {} items in {:?}",
        first.len(),
        t0.elapsed()
    );
    assert!(
        first.is_empty(),
        "nothing read yet: the wizard must explain the empty preview"
    );

    let t0 = Instant::now();
    let plan = b
        .plan_setup(SetupRequest {
            install: inst.id.clone(),
            channels: inst
                .channels
                .iter()
                .map(|c| ChannelConsent {
                    channel: c.channel.clone(),
                    consent: c.channel == "LIVE",
                    switch_language: false,
                })
                .collect(),
            data_source: DataSource::Local,
            preset: Preset::Recommended,
            fix_known_bugs: true,
        })
        .unwrap();
    eprintln!(
        "plan_setup (first read of Data.p4k): {:?}, warnings {:?}, changed {:?}",
        t0.elapsed(),
        plan.warnings,
        plan.channels.first().map(|c| c.counts.changed)
    );
    assert!(
        plan.warnings.iter().all(|w| w.kind != "game-data"),
        "{:?}",
        plan.warnings
    );

    let t0 = Instant::now();
    let after = preview().unwrap();
    eprintln!(
        "preview after the dry run: {} items in {:?}",
        after.len(),
        t0.elapsed()
    );
    assert!(
        !after.is_empty(),
        "a real build always has ship/component rows"
    );

    let t0 = Instant::now();
    b.plan_setup(SetupRequest {
        install: inst.id.clone(),
        channels: vec![ChannelConsent {
            channel: "LIVE".into(),
            consent: true,
            switch_language: false,
        }],
        data_source: DataSource::Local,
        preset: Preset::Minimal,
        fix_known_bugs: true,
    })
    .unwrap();
    eprintln!("plan_setup again (data in memory): {:?}", t0.elapsed());
    assert_eq!(
        ini_mtime(&ini),
        before,
        "the game's global.ini was not touched"
    );
}
