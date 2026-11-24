//! Live telemetry contract tests (experimental): samples for every telemetry command and the
//! event, the TS mirror's unions and command list, the 5.0 gate, the trigger gate and the
//! synthetic stream on the mock. No CIG data: the stream is `starorg_telemetry::mock`.

use super::Backend;
use super::mock::MockBackend;
use super::telemetry::*;
use super::tests::{app_src, ipc, names, ts_union};
use super::types::*;
use serde_json::{Value, json};
use std::sync::mpsc;
use std::time::Duration;

fn ptu() -> ChannelRef {
    ChannelRef {
        install: "inst-7f3a".into(),
        channel: "PTU".into(),
    }
}

fn live() -> ChannelRef {
    ChannelRef {
        install: "inst-7f3a".into(),
        channel: "LIVE".into(),
    }
}

fn on(triggers: bool) -> TelemetrySettings {
    TelemetrySettings {
        enabled: true,
        port: 50051,
        triggers,
        record: false,
        gamelog: false,
        people: false,
    }
}

fn off() -> TelemetrySettings {
    TelemetrySettings {
        enabled: false,
        ..on(false)
    }
}

fn configure(
    b: &MockBackend,
    target: ChannelRef,
    s: TelemetrySettings,
) -> TelemetryConfigureResult {
    b.telemetry_configure(TelemetryConfigureRequest {
        target: Some(target),
        settings: s,
        replace_other: false,
    })
    .unwrap()
}

/// A sink into a channel, and the receiver.
fn sink() -> (Sink, mpsc::Receiver<TelemetryEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let s: Sink = std::sync::Arc::new(move |ev| {
        let _ = tx.lock().unwrap().send(ev);
    });
    (s, rx)
}

/// The next frame within 5 s (statuses before it are skipped).
fn next_frame(rx: &mpsc::Receiver<TelemetryEvent>) -> Box<starorg_telemetry::frames::Frame> {
    loop {
        let ev = rx.recv_timeout(Duration::from_secs(5)).expect("a frame");
        if let Some(f) = ev.frame {
            return f;
        }
    }
}

/// The next Game.log view within 5 s (other events are skipped).
fn next_gamelog(rx: &mpsc::Receiver<TelemetryEvent>) -> Box<GameLogView> {
    loop {
        let ev = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("a Game.log view");
        if let Some(g) = ev.gamelog {
            return g;
        }
    }
}

fn with_gamelog(people: bool) -> TelemetrySettings {
    TelemetrySettings {
        gamelog: true,
        people,
        ..on(false)
    }
}

/// The synthetic Game.log session at a fixed time, as the samples carry it.
fn gamelog_sample() -> GameLogView {
    use starorg_telemetry::gamelog::{GameLog, mock};
    let now = 1_795_123_456_789;
    let mut log = GameLog::new(true);
    for l in mock::session_lines(now, true) {
        log.fold_line(&l);
    }
    log.end_poll(now);
    log.view("mock", true, 0)
}

/// One sample per telemetry command and both event kinds (fixed times, for the file).
pub(super) fn telemetry_samples() -> Vec<(&'static str, Value)> {
    let b = MockBackend::default();
    let off = b.telemetry_status().unwrap();
    let configured = configure(&b, ptu(), on(true));
    let (s, rx) = sink();
    let mut started = b.telemetry_start(ptu(), s).unwrap();
    // The stream's thread may have connected already; the answer of a start says connecting.
    started.link = starorg_telemetry::live::LinkStatus::Connecting;
    let _ = next_frame(&rx);
    let actions = b.telemetry_actions().unwrap();
    let pressed = b
        .telemetry_trigger(TelemetryTriggerRequest {
            action: "v_lights".into(),
            hold_ms: None,
            confirmed: true,
        })
        .unwrap();
    let refused = b
        .telemetry_trigger(TelemetryTriggerRequest {
            action: "v_self_destruct".into(),
            hold_ms: None,
            confirmed: false,
        })
        .unwrap();
    let stopped = b.telemetry_stop().unwrap();
    let recordings = b.telemetry_recordings().unwrap();
    let record_on = b
        .telemetry_record(TelemetryRecordRequest { on: true })
        .unwrap();
    b.telemetry_record(TelemetryRecordRequest { on: false })
        .unwrap();
    let name = recordings.items[0].name.clone();
    let (s2, _rx2) = sink();
    let mut replaying = b
        .telemetry_replay_start(TelemetryRecordingRef { name: name.clone() }, s2)
        .unwrap();
    // The player may have moved on already; the answer of a start is at 0.
    if let Some(r) = replaying.replay.as_mut() {
        r.position_ms = 0;
    }
    b.telemetry_replay_control(TelemetryReplayControl {
        playing: Some(false),
        ..Default::default()
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let mut controlled = b
        .telemetry_replay_control(TelemetryReplayControl {
            playing: None,
            seek_ms: Some(30_000),
            speed: Some(2.0),
        })
        .unwrap();
    if let Some(r) = controlled.replay.as_mut() {
        r.position_ms = 30_000;
    }
    let deleted = b
        .telemetry_recording_delete(TelemetryRecordingRef {
            name: recordings.items[1].name.clone(),
        })
        .unwrap();
    b.telemetry_stop().unwrap();
    let mut history = starorg_telemetry::history::History::default();
    let schema = starorg_telemetry::mock::schema();
    for i in 0..3u32 {
        let f = crate::telemetry::mock_frame(
            &schema,
            u64::from(i),
            f64::from(i),
            &starorg_telemetry::mock::MockState::default(),
        );
        history.push(1_795_123_456_000 + u64::from(i) * 1000, &f.snapshot);
    }
    let history = history.since(None, None);
    let replay_ev = TelemetryEvent {
        session: 3,
        status: None,
        frame: None,
        replay: Some(TelemetryReplayState {
            name: name.clone(),
            start_ms: 1_795_100_000_000,
            position_ms: 12_250,
            duration_ms: 240_000,
            playing: true,
            speed: 1.0,
            ended: false,
        }),
        alerts: Vec::new(),
        gamelog: None,
    };
    let mut frame = crate::telemetry::mock_frame(
        &schema,
        42,
        3.0,
        &starorg_telemetry::mock::MockState::default(),
    );
    frame.received_ms = 1_795_123_456_789;
    let status_ev = TelemetryEvent::of(
        1,
        starorg_telemetry::live::LiveEvent::Status(starorg_telemetry::live::LinkStatus::Connected),
    );
    let frame_ev = TelemetryEvent::of(
        1,
        starorg_telemetry::live::LiveEvent::Frame(Box::new(frame)),
    );
    let alerts_off = b.telemetry_alerts().unwrap();
    let mut chosen = TelemetryAlertSettings {
        notifications: TelemetryNotifications::InAppSound,
        ..TelemetryAlertSettings::default()
    };
    chosen.overrides.insert(
        "hull_half".into(),
        TelemetryRuleOverride {
            value: Some(0.6),
            cooldown_s: Some(20.0),
            ..TelemetryRuleOverride::default()
        },
    );
    let alerts_on = b.telemetry_alerts_configure(chosen).unwrap();
    let mut tested = b
        .telemetry_alert_test(TelemetryAlertTestRequest {
            rule: "hull_half".into(),
        })
        .unwrap();
    tested.at_ms = 1_795_123_456_789;
    let mut fired = tested.clone();
    fired.test = false;
    fired.text = fired.text.trim_start_matches("TEST: ").to_owned();
    let mut alert_ev = frame_ev.clone();
    alert_ev.alerts = vec![fired];
    let gamelog = gamelog_sample();
    let gamelog_ev = TelemetryEvent::gamelog(1, gamelog.clone());
    let fresh = MockBackend::default();
    let sessions = fresh.telemetry_sessions().unwrap();
    let sessions_cleared = fresh.telemetry_sessions_clear().unwrap();
    vec![
        ("telemetry_sessions", json!(sessions)),
        ("telemetry_sessions_clear", json!(sessions_cleared)),
        ("telemetry_event", json!(TELEMETRY_EVENT)),
        ("telemetry_gamelog", json!(Some(gamelog))),
        ("telemetry_people_clear", json!(None::<GameLogView>)),
        ("telemetry_event_gamelog", json!(gamelog_ev)),
        ("telemetry_status", json!(off)),
        ("telemetry_configure", json!(configured)),
        ("telemetry_start", json!(started)),
        ("telemetry_stop", json!(stopped)),
        ("telemetry_actions", json!(actions)),
        ("telemetry_trigger", json!(pressed)),
        ("telemetry_trigger_refused", json!(refused)),
        ("telemetry_event_status", json!(status_ev)),
        ("telemetry_event_frame", json!(frame_ev)),
        ("telemetry_event_replay", json!(replay_ev)),
        ("telemetry_history", json!(history)),
        ("telemetry_record", json!(record_on)),
        ("telemetry_recordings", json!(recordings)),
        ("telemetry_recording_delete", json!(deleted)),
        ("telemetry_replay_start", json!(replaying)),
        ("telemetry_replay_control", json!(controlled)),
        ("telemetry_event_alert", json!(alert_ev)),
        ("telemetry_alerts", json!(alerts_off)),
        ("telemetry_alerts_configure", json!(alerts_on)),
        ("telemetry_alert_test", json!(tested)),
    ]
}

#[test]
fn notifications_are_off_by_default_and_their_own_choice() {
    let b = MockBackend::default();
    let a = b.telemetry_alerts().unwrap();
    assert_eq!(a.settings.notifications, TelemetryNotifications::Off);
    assert!(!a.settings.speech && a.settings.overrides.is_empty());
    assert!(a.rules.len() >= 15 && a.rules.iter().all(|r| r.enabled));
    // Metrics + actions on: notifications stay off.
    configure(&b, ptu(), on(true));
    assert_eq!(
        b.telemetry_alerts().unwrap().settings.notifications,
        TelemetryNotifications::Off
    );
    let e = b
        .telemetry_alert_test(TelemetryAlertTestRequest {
            rule: "hull_half".into(),
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid, "no test alert while off");
    // Notifications on; Metrics off again: the choice is kept as it is.
    let r = b
        .telemetry_alerts_configure(TelemetryAlertSettings {
            notifications: TelemetryNotifications::InApp,
            ..TelemetryAlertSettings::default()
        })
        .unwrap();
    assert_eq!(r.settings.notifications, TelemetryNotifications::InApp);
    configure(&b, ptu(), off());
    assert_eq!(
        b.telemetry_alerts().unwrap().settings.notifications,
        TelemetryNotifications::InApp
    );
    assert!(
        !b.telemetry_alerts().unwrap().evaluating,
        "no stream, no evaluation"
    );
    let t = b
        .telemetry_alert_test(TelemetryAlertTestRequest {
            rule: "hull_half".into(),
        })
        .unwrap();
    assert!(
        t.test && t.sound.is_none() && t.speech.is_none(),
        "in app only"
    );
}

#[test]
fn rule_overrides_are_checked_and_kept() {
    let b = MockBackend::default();
    let mut a = TelemetryAlertSettings::default();
    a.overrides.insert(
        "boost_low".into(),
        TelemetryRuleOverride {
            rearm: Some(0.1),
            ..TelemetryRuleOverride::default()
        },
    );
    let e = b.telemetry_alerts_configure(a.clone()).unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(e.message.contains("boost_low"), "{}", e.message);
    assert!(
        b.telemetry_alerts().unwrap().settings.overrides.is_empty(),
        "nothing saved"
    );
    a.overrides.insert(
        "boost_low".into(),
        TelemetryRuleOverride {
            enabled: Some(false),
            ..TelemetryRuleOverride::default()
        },
    );
    let r = b.telemetry_alerts_configure(a).unwrap();
    let rule = r.rules.iter().find(|r| r.id == "boost_low").unwrap();
    assert!(!rule.enabled && rule.default_enabled);
    assert_eq!(r.settings.overrides.len(), 1);
}

#[test]
fn the_synthetic_stream_carries_no_alerts_while_off() {
    let b = MockBackend::default();
    configure(&b, ptu(), on(false));
    let (s, rx) = sink();
    b.telemetry_start(ptu(), s).unwrap();
    for _ in 0..5 {
        let ev = rx.recv_timeout(Duration::from_secs(5)).expect("an event");
        assert!(ev.alerts.is_empty());
    }
    b.telemetry_stop().unwrap();
}

#[test]
fn off_by_default_and_offered_for_5_0_only() {
    let b = MockBackend::default();
    let s = b.telemetry_status().unwrap();
    assert_eq!(s.settings, off());
    assert_eq!(s.endpoint, "127.0.0.1:50051");
    assert_eq!(
        serde_json::to_value(&s.link).unwrap(),
        json!({"state": "disabled"})
    );
    let ch = |name: &str| s.channels.iter().find(|c| c.channel == name).unwrap();
    assert!(ch("PTU").offered, "PTU runs 5.0");
    assert_eq!(ch("PTU").game_version.as_deref(), Some("5.0.0"));
    for old in ["LIVE", "HOTFIX"] {
        let c = ch(old);
        assert!(!c.offered);
        assert!(
            c.reason.as_deref().unwrap().contains("4.10.0"),
            "{:?}",
            c.reason
        );
    }
    assert!(s.env_help.contains("GRPC_CLIENT_SERVER_PORT"));
    assert!(s.env_help.contains("Never set GRPC_CLIENT_SERVER_EXTERNAL"));
    assert!(s.dangerous.contains(&"v_self_destruct".to_string()));
}

#[test]
fn switching_on_needs_an_offered_channel_and_writes_the_lines() {
    let b = MockBackend::default();
    let e = b
        .telemetry_configure(TelemetryConfigureRequest {
            target: Some(live()),
            settings: on(false),
            replace_other: false,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    assert!(e.message.contains("4.10.0"), "{}", e.message);
    assert!(
        !b.telemetry_status().unwrap().settings.enabled,
        "nothing saved"
    );
    let e = b
        .telemetry_configure(TelemetryConfigureRequest {
            target: None,
            settings: on(false),
            replace_other: false,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    let e = b
        .telemetry_configure(TelemetryConfigureRequest {
            target: Some(ptu()),
            settings: TelemetrySettings {
                port: 80,
                ..on(false)
            },
            replace_other: false,
        })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);

    let r = configure(&b, ptu(), on(false));
    assert_eq!(r.telemetry_line.unwrap().outcome, LineOutcome::Written);
    assert_eq!(r.triggers_line.unwrap().outcome, LineOutcome::NotOurs);
    let c = |s: &TelemetryStatus| {
        s.channels
            .iter()
            .find(|c| c.channel == "PTU")
            .cloned()
            .unwrap()
    };
    assert_eq!(c(&r.status).telemetry_line.state, CvarState::Set);
    assert_eq!(c(&r.status).triggers_line.state, CvarState::Missing);
    let r = configure(&b, ptu(), on(true));
    assert_eq!(r.telemetry_line.unwrap().outcome, LineOutcome::AlreadySet);
    assert_eq!(r.triggers_line.unwrap().outcome, LineOutcome::Written);
    let r = b
        .telemetry_configure(TelemetryConfigureRequest {
            target: Some(ptu()),
            settings: TelemetrySettings {
                enabled: false,
                ..on(true)
            },
            replace_other: false,
        })
        .unwrap();
    assert_eq!(r.telemetry_line.unwrap().outcome, LineOutcome::Reverted);
    assert_eq!(c(&r.status).telemetry_line.state, CvarState::Missing);
    assert_eq!(c(&r.status).triggers_line.state, CvarState::Missing);
}

#[test]
fn the_stream_needs_the_switch_and_the_synthetic_one_runs_and_stops() {
    let b = MockBackend::default();
    let (s, rx) = sink();
    assert_eq!(
        b.telemetry_start(ptu(), s.clone()).unwrap_err().code,
        ErrorCode::Invalid
    );
    configure(&b, ptu(), on(false));
    assert_eq!(
        b.telemetry_start(live(), s.clone()).unwrap_err().code,
        ErrorCode::Invalid,
        "LIVE runs 4.10"
    );
    let st = b.telemetry_start(ptu(), s).unwrap();
    assert_eq!(st.streaming, Some(ptu()));
    assert_eq!(st.schema.as_ref().unwrap().origin, "synthetic");
    let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(first.session, st.session);
    assert_eq!(
        serde_json::to_value(first.status).unwrap(),
        json!({"state": "connecting"})
    );
    let f = next_frame(&rx);
    let ship = f.snapshot.ship.as_ref().expect("the mock flies a ship");
    assert_eq!(ship.lights, Some(true));
    assert!(f.raw["vehicle"]["state"].is_object());
    assert_eq!(
        serde_json::to_value(b.telemetry_status().unwrap().link).unwrap(),
        json!({"state": "connected"})
    );
    // Throttled to the UI rate: about 10 a second, never 20.
    let t0 = std::time::Instant::now();
    let mut n = 0;
    while t0.elapsed() < Duration::from_millis(1000) {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(50))
            && ev.frame.is_some()
        {
            n += 1;
        }
    }
    assert!((5..=12).contains(&n), "{n} frames in 1 s");

    // Off stops it and releases the lines.
    let r = b
        .telemetry_configure(TelemetryConfigureRequest {
            target: None,
            settings: off(),
            replace_other: false,
        })
        .unwrap();
    assert_eq!(r.status.streaming, None);
    assert_eq!(
        serde_json::to_value(&r.status.link).unwrap(),
        json!({"state": "disabled"})
    );
    std::thread::sleep(Duration::from_millis(200));
    while rx.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(300));
    assert!(rx.try_recv().is_err(), "no event after the stop");
}

#[test]
fn triggers_need_their_own_switch_and_the_gate() {
    let b = MockBackend::default();
    configure(&b, ptu(), on(false));
    let (s, rx) = sink();
    b.telemetry_start(ptu(), s).unwrap();
    let _ = next_frame(&rx);
    let press = |action: &str, hold_ms, confirmed| {
        b.telemetry_trigger(TelemetryTriggerRequest {
            action: action.into(),
            hold_ms,
            confirmed,
        })
        .unwrap()
    };
    let r = press("v_lights", None, true);
    assert_eq!(r.refused.unwrap().reason, "disabled");
    assert!(r.outcome.is_none());
    let a = b.telemetry_actions().unwrap();
    assert!(!a.triggers_active);
    assert_eq!(a.source, "mock");
    assert!(!a.actions.is_empty(), "listed even while presses are off");

    configure(&b, ptu(), on(true));
    let a = b.telemetry_actions().unwrap();
    assert!(a.triggers_active);
    assert!(
        a.actions
            .iter()
            .any(|x| x.action_name == "v_invoke_ping" && x.holdable)
    );
    let r = press("v_lights", None, true);
    assert_eq!(r.outcome.unwrap().status.as_str(), "OK");
    // The next frames show the lights off.
    let mut off = false;
    for _ in 0..20 {
        if next_frame(&rx).snapshot.ship.unwrap().lights == Some(false) {
            off = true;
            break;
        }
    }
    assert!(off, "a press reaches the synthetic game");
    assert_eq!(
        press("v_lights", None, true).refused.unwrap().reason,
        "debounced"
    );
    assert_eq!(
        press("nope_not_listed", None, true).refused.unwrap().reason,
        "not_listed"
    );
    assert_eq!(
        press("Bad Name", None, true).refused.unwrap().reason,
        "invalid_name"
    );
    assert_eq!(
        press("toggle_flashlight", Some(500), true)
            .refused
            .unwrap()
            .reason,
        "hold_not_allowed"
    );
    assert_eq!(
        press("v_self_destruct", None, false)
            .refused
            .unwrap()
            .reason,
        "needs_confirm"
    );
    let held = press("v_invoke_ping", Some(1500), true);
    assert_eq!(held.outcome.unwrap().status.as_str(), "OK");
    assert!(
        b.activity(10)
            .unwrap()
            .iter()
            .any(|e| e.message.contains("Game action v_lights: OK"))
    );
    b.telemetry_stop().unwrap();
    let r = press("v_lights_on", None, true);
    assert!(r.error.unwrap().contains("not running"));
}

#[test]
fn a_new_stream_replaces_the_old_one() {
    let b = MockBackend::default();
    configure(&b, ptu(), on(false));
    let (s1, rx1) = sink();
    let a = b.telemetry_start(ptu(), s1).unwrap().session;
    let _ = next_frame(&rx1);
    let (s2, rx2) = sink();
    let c = b.telemetry_start(ptu(), s2).unwrap().session;
    assert!(c > a);
    let f = rx2.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(f.session, c);
    std::thread::sleep(Duration::from_millis(250));
    while rx1.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(300));
    assert!(rx1.try_recv().is_err(), "the old stream is gone");
    b.telemetry_stop().unwrap();
}

#[test]
fn record_sessions_is_its_own_switch_and_replays_play_into_the_screen() {
    let b = MockBackend::default();
    // Off by default; Metrics alone never turns it on.
    assert!(!b.telemetry_status().unwrap().settings.record);
    let s = configure(&b, ptu(), on(true)).status;
    assert!(!s.settings.record && s.recording.is_none());
    // Its own command; a configure keeps it.
    assert!(
        b.telemetry_record(TelemetryRecordRequest { on: true })
            .unwrap()
            .settings
            .record
    );
    assert!(configure(&b, ptu(), on(false)).status.settings.record);
    let before = b.telemetry_recordings().unwrap().items.len();
    let (s, rx) = sink();
    b.telemetry_start(ptu(), s).unwrap();
    let _ = next_frame(&rx);
    let st = b.telemetry_status().unwrap();
    let rec = st.recording.clone().expect("recording");
    let list = b.telemetry_recordings().unwrap();
    assert_eq!(list.items.len(), before + 1);
    assert_eq!(list.recording.as_deref(), Some(rec.as_str()));
    let e = b
        .telemetry_recording_delete(TelemetryRecordingRef { name: rec.clone() })
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
    // History over the contract: incremental within an epoch.
    let h = b
        .telemetry_history(TelemetryHistoryRequest::default())
        .unwrap();
    assert!(h.reset && !h.t_ms.is_empty());
    let more = b
        .telemetry_history(TelemetryHistoryRequest {
            epoch: Some(h.epoch),
            since_ms: h.t_ms.last().copied(),
        })
        .unwrap();
    assert!(!more.reset);
    assert!(more.t_ms.iter().all(|t| t > h.t_ms.last().unwrap()));
    // A replay replaces the stream: no channel, the replay in the status.
    let (s, rx) = sink();
    let r = b
        .telemetry_replay_start(TelemetryRecordingRef { name: rec.clone() }, s)
        .unwrap();
    assert!(r.streaming.is_none());
    assert_eq!(r.replay.as_ref().unwrap().name, rec);
    assert!(r.recording.is_none(), "the stream's recording ended");
    let f = next_frame(&rx);
    assert!(f.snapshot.ship.is_some());
    let c = b
        .telemetry_replay_control(TelemetryReplayControl {
            speed: Some(0.0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(c.replay.unwrap().speed, 0.25);
    b.telemetry_stop().unwrap();
    assert!(b.telemetry_status().unwrap().replay.is_none());
    let e = b
        .telemetry_replay_control(TelemetryReplayControl::default())
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Invalid);
    b.telemetry_recording_delete(TelemetryRecordingRef { name: rec })
        .unwrap();
    assert_eq!(b.telemetry_recordings().unwrap().items.len(), before);
    // Off: recording stops with the rest, the choice is kept for next time.
    let s = configure(&b, ptu(), off()).status;
    assert!(s.settings.record && s.recording.is_none());
}

#[test]
fn ts_mirror_lists_every_variant_and_command() {
    let ts = std::fs::read_to_string(app_src().join("telemetry.ts")).unwrap();
    use CvarState as C;
    assert_eq!(
        ts_union(&ts, "CvarState"),
        names(&[C::NoFile, C::Missing, C::Set, C::Other])
    );
    use LineOutcome as L;
    assert_eq!(
        ts_union(&ts, "LineOutcome"),
        names(&[
            L::AlreadySet,
            L::Written,
            L::NeedsDecision,
            L::NotOurs,
            L::StillUsed,
            L::Reverted,
            L::ChangedSince,
        ])
    );
    assert_eq!(
        ts_union(&ts, "HelpOs"),
        names(&[HelpOs::Windows, HelpOs::Linux])
    );
    use starorg_telemetry::live::LinkStatus as S;
    let states: Vec<String> = [
        S::Disabled,
        S::NoSchema {
            reason: String::new(),
        },
        S::Connecting,
        S::Connected,
        S::GameNotRunning,
        S::Unreachable {
            reason: String::new(),
        },
    ]
    .iter()
    .map(|s| {
        serde_json::to_value(s).unwrap()["state"]
            .as_str()
            .unwrap()
            .to_string()
    })
    .collect();
    assert_eq!(ts_union(&ts, "LinkState"), states);
    use starorg_telemetry::trigger::Refusal as R;
    let refusals: Vec<String> = [
        R::Disabled,
        R::InvalidName,
        R::InvalidHold,
        R::NotListed,
        R::HoldNotAllowed,
        R::NeedsConfirm,
        R::Debounced,
    ]
    .iter()
    .map(|r| TelemetryRefusal::of(r).reason)
    .collect();
    assert_eq!(ts_union(&ts, "RefusalReason"), refusals);
    use TelemetryNotifications as N;
    assert_eq!(
        ts_union(&ts, "TelemetryNotifications"),
        names(&[N::Off, N::InApp, N::InAppSound])
    );
    use AlertSeverity as V;
    assert_eq!(
        ts_union(&ts, "AlertSeverity"),
        names(&[V::Info, V::Warn, V::Critical])
    );
    use AlertScope as P;
    assert_eq!(
        ts_union(&ts, "AlertScope"),
        names(&[P::Any, P::Vehicle, P::Pilot, P::OnFoot])
    );
    use starorg_telemetry::gamelog::EventKind as K;
    assert_eq!(
        ts_union(&ts, "GameLogEventKind"),
        names(&[
            K::LogStarted,
            K::SessionJoin,
            K::SessionShardChange,
            K::SessionLeave,
            K::SessionSystem,
            K::SessionQueue,
            K::SessionHubDown,
            K::SessionHubUp,
            K::SessionDisconnect,
            K::SessionDropped,
            K::NearbyShip,
            K::QtRoute,
            K::QtArrived,
            K::Zone,
            K::VehicleReleased,
            K::VehicleDestroyed,
            K::CorpseItems,
            K::Spawned,
            K::Respawned,
        ])
    );
    assert_eq!(
        ts_union(&ts, "PersonRelation"),
        starorg_telemetry::gamelog::people::RELATIONS
    );
    assert_eq!(ts_union(&ts, "LeftReason"), names(&LeftReason::ALL));
    let start = ts
        .find("export const TELEMETRY_COMMANDS = [")
        .expect("telemetry.ts has TELEMETRY_COMMANDS");
    let rest = &ts[start..];
    let body = &rest[rest.find('[').unwrap() + 1..rest.find(']').unwrap()];
    let listed: Vec<&str> = body
        .split(',')
        .map(|s| s.trim().trim_matches('\''))
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(listed, TELEMETRY_COMMAND_NAMES);
    assert!(ts.contains(&format!("TELEMETRY_EVENT = '{TELEMETRY_EVENT}'")));
}

#[test]
fn telemetry_over_ipc_takes_a_req() {
    let v = ipc("telemetry_status", json!({})).unwrap();
    assert_eq!(v["settings"]["enabled"], json!(false));
    let e = ipc(
        "telemetry_configure",
        json!({"req": {"target": {"install": "inst-7f3a", "channel": "LIVE"}, "settings": {"enabled": true, "port": 50051, "triggers": false}, "replace_other": false}}),
    )
    .unwrap_err();
    assert_eq!(e["code"], json!("invalid"));
    let e = ipc(
        "telemetry_start",
        json!({"req": {"install": "inst-7f3a", "channel": "PTU"}}),
    )
    .unwrap_err();
    assert_eq!(e["code"], json!("invalid"), "off by default");
    let v = ipc(
        "telemetry_trigger",
        json!({"req": {"action": "v_lights", "hold_ms": null, "confirmed": true}}),
    )
    .unwrap();
    assert_eq!(v["refused"]["reason"], json!("disabled"));
    let v = ipc("telemetry_alerts", json!({})).unwrap();
    assert_eq!(v["settings"]["notifications"], json!("off"));
    let v = ipc(
        "telemetry_alerts_configure",
        json!({"req": {"notifications": "in_app", "speech": false, "overrides": {"g_load": {"enabled": false, "cooldown_s": null, "value": null, "rearm": null}}}}),
    )
    .unwrap();
    assert_eq!(
        v["settings"]["overrides"]["g_load"]["enabled"],
        json!(false)
    );
    // A fresh backend: notifications off, so no test alert.
    let e = ipc("telemetry_alert_test", json!({"req": {"rule": "g_load"}})).unwrap_err();
    assert_eq!(e["code"], json!("invalid"));
}

#[test]
fn the_join_history_lists_newest_first_clears_and_is_counted_by_storage() {
    let b = MockBackend::default();
    let v = b.telemetry_sessions().unwrap();
    assert_eq!(v.items.len(), 4, "seeded");
    assert!(v.items[0].joined_ms > v.items[3].joined_ms, "newest first");
    assert!(v.items.iter().any(|r| r.dropped));
    assert_eq!(v.max_items, 2000);
    assert!(v.path.is_none(), "memory only on the mock");
    let json = serde_json::to_string(&v).unwrap();
    for bad in ["address", "port", "203.0.113", "session_id", "TestPilot"] {
        assert!(!json.contains(bad), "{bad} in {json}");
    }
    let area = |b: &MockBackend| {
        b.storage_status()
            .unwrap()
            .areas
            .into_iter()
            .find(|a| a.id == StorageAreaId::TelemetrySessions)
            .unwrap()
    };
    let a = area(&b);
    assert!(a.bytes > 0 && !a.in_cache);
    let cleared = b.telemetry_sessions_clear().unwrap();
    assert!(cleared.items.is_empty());
    assert!(cleared.revision > v.revision);
    assert_eq!(area(&b).bytes, 0);
}

#[test]
fn game_log_events_need_their_own_switch_and_people_another() {
    let b = MockBackend::default();
    configure(&b, ptu(), on(false));
    let (s, rx) = sink();
    b.telemetry_start(ptu(), s).unwrap();
    let _ = next_frame(&rx);
    assert_eq!(b.telemetry_gamelog().unwrap(), None, "off by default");
    for _ in 0..5 {
        let ev = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(ev.gamelog.is_none());
    }

    // On while streaming: the reader starts for the running stream; no people yet.
    let session = configure(&b, ptu(), with_gamelog(false)).status.session;
    let g = next_gamelog(&rx);
    assert_eq!(g.source, "mock");
    assert!(g.found && g.people.is_none());
    assert!(
        g.events
            .iter()
            .any(|e| e.kind == starorg_telemetry::gamelog::EventKind::Respawned)
    );
    let json = serde_json::to_string(&g).unwrap();
    assert!(
        !json.contains("TestPilot"),
        "no handle without People: {json}"
    );
    assert_eq!(
        b.telemetry_status().unwrap().session,
        session,
        "the stream kept running"
    );

    // People on: the reader starts over with the roster.
    configure(&b, ptu(), with_gamelog(true));
    let p = loop {
        if let Some(p) = next_gamelog(&rx).people {
            break p;
        }
    };
    assert!(p.counts.total >= 3, "{p:?}");
    assert_eq!(p.self_name.as_deref(), Some("TestPilot_Self"));
    let cleared = b.telemetry_people_clear().unwrap().unwrap();
    assert!(cleared.people.unwrap().people.is_empty());

    // Off again: the reader stops and holds nothing.
    configure(&b, ptu(), on(false));
    assert_eq!(b.telemetry_gamelog().unwrap(), None);
    assert_eq!(b.telemetry_people_clear().unwrap(), None);
    b.telemetry_stop().unwrap();
}

#[test]
fn the_game_log_reader_stops_with_the_stream_and_a_new_stream_starts_its_own() {
    let b = MockBackend::default();
    configure(&b, ptu(), with_gamelog(true));
    assert_eq!(b.telemetry_gamelog().unwrap(), None, "no stream, no reader");
    let (s, rx) = sink();
    let a = b.telemetry_start(ptu(), s.clone()).unwrap().session;
    let g = next_gamelog(&rx);
    assert!(g.people.is_some());
    let c = b.telemetry_start(ptu(), s).unwrap().session;
    assert!(c > a);
    let ev = loop {
        let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if ev.gamelog.is_some() && ev.session == c {
            break ev;
        }
    };
    assert_eq!(ev.session, c);
    b.telemetry_stop().unwrap();
    assert_eq!(b.telemetry_gamelog().unwrap(), None);
    std::thread::sleep(Duration::from_millis(700));
    while rx.try_recv().is_ok() {}
    std::thread::sleep(Duration::from_millis(700));
    assert!(rx.try_recv().is_err(), "no Game.log view after the stop");
}
