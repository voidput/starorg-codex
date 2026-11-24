//! Live telemetry on the real backend (experimental) against a synthetic fake install: the
//! 5.0 gate, the `[experimental]` switches in `config.toml`, the `USER.cfg` lines written and
//! taken out again, and a stream that finds no game on a closed loopback port. The schema is
//! the synthetic one (`starorg_telemetry::mock`), put in the cache folder as if it had been
//! read from the exe; no CIG file and no real game are involved.

use starorg_codex_app::contract::Backend;
use starorg_codex_app::contract::real::{DialogSlot, RealBackend, RealConfig, SourceFactory};
use starorg_codex_app::contract::telemetry::*;
use starorg_codex_app::contract::types::*;
use starorg_core::appdirs::AppDirs;
use starorg_core::apply::Ctx;
use starorg_core::discovery::{DiscoveryEnv, DiscoveryRequest};
use starorg_core::source::GameSource;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

struct NoSource;
impl SourceFactory for NoSource {
    fn source(&self, _: &AppDirs) -> Result<Box<dyn GameSource>, String> {
        Err("no extraction in these tests".into())
    }
}

struct Fake {
    _t: tempfile::TempDir,
    lib: PathBuf,
    dirs: AppDirs,
    backend: RealBackend,
}

const PTU_CHANGE: u64 = 13189690;
const CFG: &str = "g_language = english\r\nr_width = 2560\r\n";

type FakeChannel = (&'static str, &'static str, u64, &'static str);
const LIVE_4_10: FakeChannel = ("LIVE", "sc-alpha-4.10.0", 13165946, "4.10.200.58746");
const PTU_5_0: FakeChannel = ("PTU", "sc-alpha-5.0.0-ptu", PTU_CHANGE, "5.0.0.1");

fn fake() -> Fake {
    fake_with(&[LIVE_4_10, PTU_5_0])
}

fn fake_with(channels: &[FakeChannel]) -> Fake {
    let t = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(t.path()).unwrap();
    let lib = base.join("games/StarCitizen");
    for &(ch, branch, change, version) in channels {
        let d = lib.join(ch);
        std::fs::create_dir_all(d.join("Data")).unwrap();
        std::fs::write(d.join("Data.p4k"), b"fake archive").unwrap();
        std::fs::write(
            d.join("build_manifest.id"),
            format!(
                r#"{{"Data":{{"Branch":"{branch}","RequestedP4ChangeNum":"{change}","Version":"{version}"}}}}"#
            ),
        )
        .unwrap();
        std::fs::write(d.join("USER.cfg"), CFG).unwrap();
    }
    let dirs = AppDirs::under(&base.join("app"));
    let backend = RealBackend::new(RealConfig {
        ctx: Ctx::new(dirs.clone(), "0.0.0-test", "test"),
        discovery: DiscoveryRequest {
            game_dir: Some(lib.clone()),
            prefix: None,
        },
        env: DiscoveryEnv::from_process(),
        source: Arc::new(NoSource),
        dialogs: DialogSlot::default(),
    });
    Fake {
        _t: t,
        lib,
        dirs,
        backend,
    }
}

impl Fake {
    fn r(&self, ch: &str) -> ChannelRef {
        ChannelRef {
            install: self.backend.overview().unwrap().installs[0].id.clone(),
            channel: ch.into(),
        }
    }

    fn configure(
        &self,
        ch: Option<&str>,
        s: TelemetrySettings,
    ) -> CmdResult<TelemetryConfigureResult> {
        self.backend.telemetry_configure(TelemetryConfigureRequest {
            target: ch.map(|c| self.r(c)),
            settings: s,
            replace_other: false,
        })
    }

    fn cfg(&self, ch: &str) -> String {
        std::fs::read_to_string(self.lib.join(ch).join("USER.cfg")).unwrap()
    }
}

fn on(port: u16, triggers: bool) -> TelemetrySettings {
    TelemetrySettings {
        enabled: true,
        port,
        triggers,
        record: false,
        gamelog: false,
        people: false,
        sessions: false,
    }
}

fn off() -> TelemetrySettings {
    TelemetrySettings {
        enabled: false,
        ..on(50051, false)
    }
}

#[test]
fn offered_for_the_5_0_channel_only_and_off_by_default() {
    let f = fake();
    let s = f.backend.telemetry_status().unwrap();
    assert!(!s.settings.enabled);
    assert_eq!(s.settings.port, 50051);
    assert!(!s.mock);
    let ch = |n: &str| s.channels.iter().find(|c| c.channel == n).unwrap().clone();
    assert!(ch("PTU").offered);
    assert!(!ch("LIVE").offered);
    assert!(ch("LIVE").reason.unwrap().contains("4.10.0"));
    assert_eq!(ch("PTU").telemetry_line.state, CvarState::Missing);
    assert!(!ch("PTU").exe_found);
    // Looking writes nothing.
    assert!(!f.dirs.config_toml().exists());
}

#[test]
fn switching_on_writes_config_and_user_cfg_and_off_takes_them_out() {
    let f = fake();
    let e = f.configure(Some("LIVE"), on(50051, false)).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(!f.dirs.config_toml().exists(), "nothing saved for LIVE");
    assert_eq!(f.cfg("LIVE"), CFG);

    let r = f.configure(Some("PTU"), on(50123, false)).unwrap();
    assert_eq!(
        r.telemetry_line.as_ref().unwrap().outcome,
        LineOutcome::Written
    );
    assert_eq!(
        r.triggers_line.as_ref().unwrap().outcome,
        LineOutcome::NotOurs
    );
    let cfg = f.cfg("PTU");
    assert!(cfg.starts_with(CFG), "other lines stay: {cfg:?}");
    assert!(cfg.contains("server.game.telemetry.enabled = 1"));
    assert!(!cfg.contains("trigger.actions"));
    let toml = std::fs::read_to_string(f.dirs.config_toml()).unwrap();
    assert!(
        toml.contains("[experimental]") && toml.contains("telemetry_port = 50123"),
        "{toml}"
    );
    assert_eq!(r.status.endpoint, "127.0.0.1:50123");
    assert!(r.status.env_help.contains("50123"));

    let r = f.configure(Some("PTU"), on(50123, true)).unwrap();
    assert_eq!(r.triggers_line.unwrap().outcome, LineOutcome::Written);
    assert!(
        f.cfg("PTU")
            .contains("server.game.telemetry.trigger.actions.enabled = 1")
    );

    let r = f.configure(None, off()).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(f.cfg("PTU"), CFG, "both lines taken out again");
    assert!(!f.backend.telemetry_status().unwrap().settings.enabled);
}

#[test]
fn an_explicit_other_value_is_replaced_only_after_asking() {
    let f = fake();
    std::fs::write(
        f.lib.join("PTU/USER.cfg"),
        format!("{CFG}server.game.telemetry.enabled = 0\r\n"),
    )
    .unwrap();
    let r = f.configure(Some("PTU"), on(50051, false)).unwrap();
    let l = r.telemetry_line.unwrap();
    assert_eq!(l.outcome, LineOutcome::NeedsDecision);
    assert_eq!(l.value.as_deref(), Some("0"));
    assert!(f.cfg("PTU").contains("enabled = 0"));
    let r = f
        .backend
        .telemetry_configure(TelemetryConfigureRequest {
            target: Some(f.r("PTU")),
            settings: on(50051, false),
            replace_other: true,
        })
        .unwrap();
    assert_eq!(r.telemetry_line.unwrap().outcome, LineOutcome::Written);
    assert!(f.cfg("PTU").contains("server.game.telemetry.enabled = 1"));
    f.configure(None, off()).unwrap();
    assert!(
        f.cfg("PTU").contains("server.game.telemetry.enabled = 0"),
        "the user's line is back"
    );
}

#[test]
fn the_lines_follow_the_target_channel_and_triggers_off_takes_them_out_everywhere() {
    // LIVE and PTU both on 5.0: the lines are only ever in the target's USER.cfg.
    let f = fake_with(&[("LIVE", "sc-alpha-5.0.0", 13165946, "5.0.0.2"), PTU_5_0]);
    f.configure(Some("PTU"), on(50051, true)).unwrap();
    assert!(f.cfg("PTU").contains("trigger.actions.enabled = 1"));

    // Another channel, game actions off: PTU keeps neither line.
    let r = f.configure(Some("LIVE"), on(50051, false)).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(f.cfg("PTU"), CFG, "both lines left PTU");
    let live = f.cfg("LIVE");
    assert!(live.contains("server.game.telemetry.enabled = 1"));
    assert!(!live.contains("trigger.actions"), "{live:?}");

    // Game actions on for LIVE, then back to PTU with them still on: LIVE is clean.
    f.configure(Some("LIVE"), on(50051, true)).unwrap();
    f.configure(Some("PTU"), on(50051, true)).unwrap();
    assert_eq!(f.cfg("LIVE"), CFG);
    assert!(f.cfg("PTU").contains("trigger.actions.enabled = 1"));

    // Off with no target named: every channel clean.
    f.configure(None, off()).unwrap();
    assert_eq!(f.cfg("PTU"), CFG);
    assert_eq!(f.cfg("LIVE"), CFG);
}

#[test]
fn a_user_cfg_that_cannot_be_changed_leaves_the_switches_as_they_were() {
    let f = fake();
    // Larger than the 1 MiB the app edits.
    let pad = "; padding\r\n".repeat(120_000);
    let big = format!("{CFG}{pad}");
    std::fs::write(f.lib.join("PTU/USER.cfg"), &big).unwrap();
    let e = f.configure(Some("PTU"), on(50051, true)).unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
    assert!(e.message.contains("USER.cfg not changed"), "{}", e.message);
    assert!(
        !f.backend.telemetry_status().unwrap().settings.enabled,
        "the switch stays off"
    );
    assert_eq!(f.cfg("PTU"), big);

    // On already, then game actions refused: telemetry stays on, game actions stay off.
    std::fs::write(f.lib.join("PTU/USER.cfg"), CFG).unwrap();
    f.configure(Some("PTU"), on(50051, false)).unwrap();
    let grown = format!("{}{pad}", f.cfg("PTU"));
    std::fs::write(f.lib.join("PTU/USER.cfg"), &grown).unwrap();
    assert!(f.configure(Some("PTU"), on(50051, true)).is_err());
    let s = f.backend.telemetry_status().unwrap().settings;
    assert!(s.enabled && !s.triggers, "{s:?}");
    assert_eq!(f.cfg("PTU"), grown);
}

fn sink() -> (Sink, mpsc::Receiver<TelemetryEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let s: Sink = Arc::new(move |ev| {
        let _ = tx.lock().unwrap().send(ev);
    });
    (s, rx)
}

fn state(ev: &TelemetryEvent) -> Option<String> {
    ev.status.as_ref().map(|s| {
        serde_json::to_value(s).unwrap()["state"]
            .as_str()
            .unwrap()
            .to_string()
    })
}

#[test]
fn without_an_exe_or_a_cached_schema_the_stream_says_no_schema() {
    let f = fake();
    let (s, rx) = sink();
    assert_eq!(
        f.backend
            .telemetry_start(f.r("PTU"), s.clone())
            .unwrap_err()
            .code,
        ErrorCode::Invalid,
        "off by default"
    );
    f.configure(Some("PTU"), on(50051, false)).unwrap();
    let st = f.backend.telemetry_start(f.r("PTU"), s).unwrap();
    assert_eq!(
        serde_json::to_value(&st.link).unwrap()["state"],
        serde_json::json!("no_schema")
    );
    let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(state(&ev).as_deref(), Some("no_schema"));
    f.backend.telemetry_stop().unwrap();
}

#[test]
fn a_cached_schema_streams_loopback_and_reports_the_game_not_running() {
    let f = fake();
    // A closed port on 127.0.0.1: nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    f.configure(Some("PTU"), on(port, false)).unwrap();
    // The synthetic schema in the cache, as a first start would have left it.
    let dir = starorg_core::storage::telemetry_dir(&f.dirs);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("sc-alpha-5.0.0-ptu-{PTU_CHANGE}.fds")),
        starorg_telemetry::mock::schema().to_set_bytes(),
    )
    .unwrap();
    let (s, rx) = sink();
    let st = f.backend.telemetry_start(f.r("PTU"), s).unwrap();
    assert_eq!(st.streaming, Some(f.r("PTU")));
    let schema = st.schema.unwrap();
    assert_eq!(schema.origin, "cache");
    assert_eq!(schema.key, format!("sc-alpha-5.0.0-ptu-{PTU_CHANGE}"));
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let ev = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(ev.session, st.session);
        seen.extend(state(&ev));
    }
    assert_eq!(seen, ["connecting", "game_not_running"]);
    // Settings > Storage counts the schema.
    let storage = f.backend.storage_measure().unwrap();
    let area = storage
        .areas
        .iter()
        .find(|a| a.id == StorageAreaId::Telemetry)
        .unwrap();
    assert!(area.bytes > 0 && area.in_cache);
    // Triggers stay refused while their switch is off; the actions list is empty without a game.
    let r = f
        .backend
        .telemetry_trigger(TelemetryTriggerRequest {
            action: "v_lights".into(),
            hold_ms: None,
            confirmed: true,
        })
        .unwrap();
    assert_eq!(r.refused.unwrap().reason, "disabled");
    let a = f.backend.telemetry_actions().unwrap();
    assert!(a.actions.is_empty());
    assert!(a.error.is_some(), "the game did not answer");
    // Switching off stops the stream.
    let r = f.configure(None, off()).unwrap();
    assert_eq!(r.status.streaming, None);
}

#[test]
fn recordings_are_listed_replayed_deleted_and_counted_by_storage() {
    let f = fake();
    f.configure(Some("PTU"), on(50051, false)).unwrap();
    // "Record sessions": its own switch, saved in config.toml, off by default.
    assert!(!f.backend.telemetry_status().unwrap().settings.record);
    let st = f
        .backend
        .telemetry_record(TelemetryRecordRequest { on: true })
        .unwrap();
    assert!(st.settings.record);
    let toml = std::fs::read_to_string(f.dirs.config_toml()).unwrap();
    assert!(toml.contains("telemetry_record = true"), "{toml}");
    // A synthetic recording, as a stream would have written it.
    let dir = starorg_core::storage::telemetry_recordings_dir(&f.dirs);
    let schema = starorg_telemetry::mock::schema();
    let caps = starorg_telemetry::record::Caps {
        interval: Duration::ZERO,
        ..Default::default()
    };
    let t0 = 1_795_437_296_000u64;
    let mut r = starorg_telemetry::record::Recorder::create(&dir, "PTU", "mock", t0, caps).unwrap();
    for i in 0..20u64 {
        let raw = starorg_telemetry::json::to_json(&starorg_telemetry::mock::frame(
            &schema,
            i as f64 / 4.0,
            &starorg_telemetry::mock::MockState::default(),
        ));
        assert!(r.frame(t0 + i * 250, &raw));
    }
    let name = r.finish().unwrap();
    std::fs::write(dir.join("notes.txt"), "not ours").unwrap();
    let list = f.backend.telemetry_recordings().unwrap();
    assert_eq!(list.items.len(), 1, "only our files");
    assert_eq!(list.items[0].name, name);
    assert_eq!(list.items[0].duration_ms, Some(4750));
    assert_eq!(list.max_files, 20);
    // Settings > Storage: its own area, outside the cache and its cap.
    let storage = f.backend.storage_measure().unwrap();
    let area = storage
        .areas
        .iter()
        .find(|a| a.id == StorageAreaId::TelemetryRecordings)
        .unwrap();
    assert_eq!(area.files, 2, "the folder as a whole");
    assert!(area.bytes >= list.items[0].bytes && !area.in_cache);
    // Replayed into the screen.
    let (s, rx) = sink();
    let st = f
        .backend
        .telemetry_replay_start(TelemetryRecordingRef { name: name.clone() }, s)
        .unwrap();
    assert!(st.streaming.is_none());
    assert_eq!(st.replay.as_ref().unwrap().duration_ms, 4750);
    let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let frame = ev.frame.expect("a frame");
    assert_eq!(frame.received_ms, t0);
    assert!(frame.snapshot.ship.is_some());
    let e = f
        .backend
        .telemetry_recording_delete(TelemetryRecordingRef { name: name.clone() })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy, "playing");
    let bad = f
        .backend
        .telemetry_replay_start(
            TelemetryRecordingRef {
                name: "../config.jsonl".into(),
            },
            sink().0,
        )
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::Invalid);
    f.backend.telemetry_stop().unwrap();
    let list = f
        .backend
        .telemetry_recording_delete(TelemetryRecordingRef { name })
        .unwrap();
    assert!(list.items.is_empty());
    assert!(dir.join("notes.txt").is_file());
}

/// The next Game.log view that passes `ok`, within 10 s.
fn gamelog_until(
    rx: &mpsc::Receiver<TelemetryEvent>,
    ok: impl Fn(&GameLogView) -> bool,
) -> Box<GameLogView> {
    let end = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let left = end.saturating_duration_since(std::time::Instant::now());
        let ev = rx.recv_timeout(left).expect("a Game.log view in time");
        if let Some(g) = ev.gamelog
            && ok(&g)
        {
            return g;
        }
    }
}

#[test]
fn game_log_events_follow_the_channel_log_and_handles_stay_out_of_activity_and_diagnostics() {
    let f = fake();
    // Synthetic lines only. The own handle is "StarCitizen", a word the fake install's path
    // holds, so the diagnostics text shows whether held handles are replaced.
    let note = "[Notice] <SHUDEvent_OnNotification> Added notification \"";
    let tail = ": \" [1] to queue. New queue size: 1";
    let log = f.lib.join("PTU").join("Game.log");
    std::fs::write(
        &log,
        format!(
            "Log started on Mon Nov 23 10:00:00 2026\r\n\
             <2026-11-23T10:00:01.000Z> [Notice] <Legacy login response> [CIG-net] User Login Success - Handle[StarCitizen] - Time[1]\r\n\
             <2026-11-23T10:00:02.000Z> [Notice] <Join PU> address[203.0.113.9] port[64000] shard[ptu_use1b_1_020] locationId[1]\r\n\
             <2026-11-23T10:00:03.000Z> {note}New Member Joined\r\n\
             <2026-11-23T10:00:03.000Z> TestPilot_B has joined the party.{tail}\r\n"
        ),
    )
    .unwrap();
    let mut s = on(50051, false);
    s.gamelog = true;
    s.people = true;
    f.configure(Some("PTU"), s).unwrap();
    let (sink, rx) = sink();
    // No exe and no cached schema: the stream says no_schema, Game.log is read anyway.
    let st = f.backend.telemetry_start(f.r("PTU"), sink).unwrap();
    assert_eq!(
        serde_json::to_value(&st.link).unwrap()["state"],
        serde_json::json!("no_schema")
    );
    let g = gamelog_until(&rx, |g| {
        g.people.as_ref().is_some_and(|p| p.counts.total == 1)
    });
    assert!(g.found && g.source == "game_log");
    assert_eq!(g.session.shard.as_deref(), Some("ptu_use1b_1_020"));
    let p = g.people.unwrap();
    assert_eq!(p.people[0].name, "TestPilot_B");
    assert_eq!(p.self_name.as_deref(), Some("StarCitizen"));
    assert!(
        !serde_json::to_string(&g.events)
            .unwrap()
            .contains("TestPilot")
    );
    assert!(
        !serde_json::to_string(&g.session)
            .unwrap()
            .contains("203.0.113.9")
    );

    // Lines added later are read (only the new bytes); the handle is closed at once.
    let mut fh = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    std::io::Write::write_all(
        &mut fh,
        b"<2026-11-23T10:00:09.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Entering Armistice Zone - Combat Prohibited: \" [2] to queue.\r\n",
    )
    .unwrap();
    drop(fh);
    let g = gamelog_until(&rx, |g| g.signals.armistice);
    assert_eq!(g.signals.zone.as_ref().unwrap().kind, "armistice");

    // No handle in the Activity log; the diagnostics text replaces every one held.
    let activity = f.backend.activity(500).unwrap();
    assert!(
        activity
            .iter()
            .all(|e| !e.message.contains("TestPilot") && !e.message.contains("Handle[")),
        "{activity:?}"
    );
    let d = f.backend.export_diagnostics().unwrap();
    assert!(!d.text.contains("StarCitizen"), "{}", d.text);
    assert!(d.text.contains("<handle>"), "{}", d.text);

    // Clear forgets everyone; People off drops the roster; off stops the reader.
    let v = f.backend.telemetry_people_clear().unwrap().unwrap();
    assert!(v.people.unwrap().people.is_empty());
    s.people = false;
    f.configure(Some("PTU"), s).unwrap();
    let g = gamelog_until(&rx, |g| g.people.is_none());
    assert!(g.found);
    assert!(
        f.backend
            .telemetry_gamelog()
            .unwrap()
            .unwrap()
            .people
            .is_none()
    );
    f.backend.telemetry_stop().unwrap();
    assert_eq!(f.backend.telemetry_gamelog().unwrap(), None);
    let d = f.backend.export_diagnostics().unwrap();
    assert!(
        d.text.contains("StarCitizen"),
        "nothing held any more, nothing replaced"
    );
}

#[test]
fn shard_stints_from_game_log_and_its_backups_go_to_the_history_file_once() {
    let f = fake();
    let ch = f.lib.join("PTU");
    let header = "Log started on Mon Nov 23 09:00:00 2026\r\nFileVersion: 9.9.0.1\r\nExecutable: C:\\Games\\StarCitizen\\PTU\\Bin64\\StarCitizen.exe\r\n";
    let join = |ts: &str, shard: &str| {
        format!(
            "<{ts}> [Notice] <Join PU> address[203.0.113.9] port[64000] shard[{shard}] locationId[1]\r\n"
        )
    };
    // An older log in logbackups: one stint that dropped.
    std::fs::create_dir_all(ch.join("logbackups")).unwrap();
    std::fs::write(
        ch.join("logbackups").join("Game Build(1) 23 Nov 26 (09 00 00).log"),
        format!(
            "{header}{}<2026-11-23T09:20:00.000Z> [Notice] <Channel Disconnected> cause=3 reason=\"Remote Disconnect - player inactive\" gamerules=\"SC_Default\" remoteAddr=203.0.113.9:64000 nickname=\"TestPilot_Self\" uptime_secs=1195.0\r\n",
            join("2026-11-23T09:00:05.000Z", "ptu_use1b_1_020")
        ),
    )
    .unwrap();
    // The live Game.log: a stint that ended at the menu.
    std::fs::write(
        ch.join("Game.log"),
        format!(
            "{}{}<2026-11-23T10:30:00.000Z> Loading screen for Frontend_Main : x closed after 3.0 seconds\r\n",
            header.replace("09:00:00", "10:00:00"),
            join("2026-11-23T10:00:05.000Z", "ptu_euw1a_1_007")
        ),
    )
    .unwrap();
    let dir = starorg_core::storage::telemetry_sessions_dir(&f.dirs);
    // Game.log events alone: the Events panel, but no history written and no backup read.
    let mut s = on(50051, false);
    s.gamelog = true;
    f.configure(Some("PTU"), s).unwrap();
    let (s0, rx0) = sink();
    f.backend.telemetry_start(f.r("PTU"), s0).unwrap();
    let g = gamelog_until(&rx0, |g| g.found);
    assert_eq!(g.sessions_rev, 0);
    std::thread::sleep(Duration::from_millis(1000));
    let v = f.backend.telemetry_sessions().unwrap();
    assert!(v.items.is_empty() && v.backfill_pending == 0, "{v:?}");
    assert!(!dir.join("history.json").exists());
    f.backend.telemetry_stop().unwrap();
    // "Shard history", its own switch.
    s.sessions = true;
    f.configure(Some("PTU"), s).unwrap();
    let (s1, _rx) = sink();
    f.backend.telemetry_start(f.r("PTU"), s1).unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(10);
    let v = loop {
        let v = f.backend.telemetry_sessions().unwrap();
        if v.items.len() >= 2 && v.backfill_pending == 0 {
            break v;
        }
        assert!(std::time::Instant::now() < end, "{v:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(v.items.len(), 2);
    assert_eq!(v.items[0].shard, "ptu_euw1a_1_007", "newest first");
    assert_eq!(v.items[0].left_reason, Some(LeftReason::Leave));
    assert_eq!(v.items[1].left_reason, Some(LeftReason::Dropped));
    assert!(v.items[1].dropped);
    let text = std::fs::read_to_string(dir.join("history.json")).unwrap();
    for bad in ["203.0.113", "64000", "TestPilot", "inactive"] {
        assert!(!text.contains(bad), "{bad} in {text}");
    }
    // Read again (Game.log from its start, the backup already marked): nothing new.
    f.backend.telemetry_stop().unwrap();
    let (s2, _rx2) = sink();
    f.backend.telemetry_start(f.r("PTU"), s2).unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(f.backend.telemetry_sessions().unwrap().items.len(), 2);
    f.backend.telemetry_stop().unwrap();
    // Settings > Storage: its own area, outside the cache; Clear empties it.
    let area = |f: &Fake| {
        f.backend
            .storage_measure()
            .unwrap()
            .areas
            .into_iter()
            .find(|a| a.id == StorageAreaId::TelemetrySessions)
            .unwrap()
    };
    let a = area(&f);
    assert!(a.bytes > 0 && !a.in_cache, "{a:?}");
    assert!(
        f.backend
            .telemetry_sessions_clear()
            .unwrap()
            .items
            .is_empty()
    );
    assert!(area(&f).bytes < a.bytes);
    assert!(f.backend.telemetry_sessions().unwrap().items.is_empty());
    // And stays cleared: the reader starts again and folds the same Game.log from its start,
    // and the backup shows up again under another name (as the live log does once rotated).
    std::fs::copy(
        ch.join("Game.log"),
        ch.join("logbackups")
            .join("Game Build(1) 23 Nov 26 (10 00 00).log"),
    )
    .unwrap();
    let (s3, rx3) = sink();
    f.backend.telemetry_start(f.r("PTU"), s3).unwrap();
    let _ = gamelog_until(&rx3, |g| g.found);
    // At least 1.5 s (three polls: the backup copy is read in by then), and until no backup
    // is left to read.
    let t = std::time::Instant::now();
    while f.backend.telemetry_sessions().unwrap().backfill_pending > 0
        || t.elapsed() < Duration::from_millis(1500)
    {
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(100));
    }
    let v = f.backend.telemetry_sessions().unwrap();
    assert!(v.items.is_empty(), "cleared rows came back: {v:?}");
    f.backend.telemetry_stop().unwrap();
}
