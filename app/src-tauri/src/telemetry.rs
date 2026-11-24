//! Settings -> Experimental -> "Live telemetry" (docs/user-guide.md#live-telemetry): the app's
//! side of `starorg-telemetry`, shared by both backends.
//!
//! Star Citizen 5.0 can host a local gRPC service on `127.0.0.1:<port>`. When the user has
//! turned the feature on and opens the Live telemetry screen, a stream runs on Tauri's Tokio
//! runtime ([`starorg_telemetry::live::run`]: reconnect with backoff, frames throttled to
//! [`starorg_telemetry::frames::UI_HZ`]) and every status change and frame goes to a
//! [`Sink`] (the commands turn it into a [`crate::contract::telemetry::TELEMETRY_EVENT`]).
//! One stream at a time; it stops on `telemetry_stop`, when the feature is switched off and
//! when the app exits (`crate::run`).
//!
//! The mock backend runs the same host on the synthetic generator ([`starorg_telemetry::mock`])
//! instead of a game, so the screen works without one. Nothing here talks to anything but
//! loopback ([`LoopbackAddr`]).
//!
//! Pressing a game input goes through a [`TriggerGate`]: off unless the user's
//! "Allow game actions" switch is on, only actions the game lists as triggerable, a
//! confirmation for the dangerous ones and a 1 s debounce per action.
//!
//! Alerts ([`starorg_telemetry::alerts`]) are evaluated on the stream's frames only while a
//! stream runs and the user's own Notifications choice is not Off (it is Off by default and
//! independent of Metrics and actions). They ride on the frame event that fired them and are
//! kept in a short list ([`RECENT_MAX`]) for the screen's Alerts panel. The host strips the
//! tone and the speech text unless the user chose sound (and speech), so the webview plays
//! only what was asked for.

use crate::contract::telemetry::{
    TelemetryActions, TelemetryAlertRule, TelemetryAlerts, TelemetryEvent, TelemetryRefusal,
    TelemetrySchemaInfo, TelemetryTriggerResult,
};
use crate::contract::types::ChannelRef;
use starorg_telemetry::alerts::{self, Alert, AlertEngine, AlertSettings, Notifications, Trigger};
use starorg_telemetry::client::{LoopbackAddr, TelemetryClient};
use starorg_telemetry::frames::{Frame, Snapshot, UI_INTERVAL};
use starorg_telemetry::live::{self, LinkStatus, LiveEvent, LiveOptions};
use starorg_telemetry::mock::{self, MockGame};
use starorg_telemetry::schema::{Schema, SchemaOrigin};
use starorg_telemetry::trigger::{DANGEROUS, TriggerGate, TriggerableAction};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Most alerts kept for the Alerts panel (newest first).
pub const RECENT_MAX: usize = 50;

/// Where the events of a stream go (the command emits them to the window).
pub type Sink = Arc<dyn Fn(TelemetryEvent) + Send + Sync>;

/// What the stream reads from.
#[derive(Clone)]
enum Source {
    Game {
        schema: Arc<Schema>,
        addr: LoopbackAddr,
    },
    /// The synthetic generator; trigger presses flip its state.
    Mock {
        schema: Arc<Schema>,
        game: Arc<Mutex<MockGame>>,
    },
}

impl Source {
    fn schema(&self) -> &Arc<Schema> {
        match self {
            Source::Game { schema, .. } | Source::Mock { schema, .. } => schema,
        }
    }
}

enum Task {
    Live(tauri::async_runtime::JoinHandle<()>),
    Mock(Arc<AtomicBool>),
}

impl Task {
    fn stop(self) {
        match self {
            Task::Live(h) => h.abort(),
            Task::Mock(stop) => stop.store(true, Ordering::Relaxed),
        }
    }
}

struct Running {
    /// Bumped at every start and stop; events of an older stream are dropped.
    session: u64,
    target: Option<ChannelRef>,
    link: LinkStatus,
    source: Option<Source>,
    task: Option<Task>,
    gate: TriggerGate,
    /// Why the last list request failed, if it did.
    actions_error: Option<String>,
    alerts: AlertHub,
}

/// The alert engine of the running stream and how alerts are delivered.
struct AlertHub {
    engine: AlertEngine,
    notifications: Notifications,
    speech: bool,
    /// Newest first, at most [`RECENT_MAX`].
    recent: VecDeque<Alert>,
    /// The engine's clock.
    epoch: Instant,
}

impl Default for AlertHub {
    fn default() -> Self {
        Self {
            engine: AlertEngine::default(),
            notifications: Notifications::Off,
            speech: false,
            recent: VecDeque::new(),
            epoch: Instant::now(),
        }
    }
}

/// Only what the user asked for: no tone without sound, no speech without both.
fn deliver(mut a: Alert, notifications: Notifications, speech: bool) -> Alert {
    if notifications != Notifications::InAppSound {
        a.sound = None;
        a.speech = None;
    } else if !speech {
        a.speech = None;
    }
    a
}

/// A test alert of `rule` with the user's settings, shaped as a real one would be. Refused
/// while notifications are off.
pub fn alert_test(
    a: &AlertSettings,
    rule: &str,
) -> Result<Alert, (crate::contract::types::ErrorCode, String)> {
    use crate::contract::types::ErrorCode;
    if a.notifications == Notifications::Off {
        return Err((
            ErrorCode::Invalid,
            "Turn on Notifications under Settings -> Experimental first.".into(),
        ));
    }
    let (rules, _) = alerts::apply_overrides(&alerts::defaults(), &a.overrides);
    let t = AlertEngine::new(rules)
        .test_alert(rule, now_ms())
        .ok_or_else(|| (ErrorCode::NotFound, format!("No alert rule {rule}")))?;
    Ok(deliver(t, a.notifications, a.speech))
}

impl AlertHub {
    /// The alerts of one frame (none while notifications are off).
    fn frame(&mut self, f: &Frame) -> Vec<Alert> {
        if self.notifications == Notifications::Off {
            return Vec::new();
        }
        let now = self.epoch.elapsed().as_secs_f64();
        self.frame_at(f, now)
    }

    fn frame_at(&mut self, f: &Frame, now: f64) -> Vec<Alert> {
        if self.notifications == Notifications::Off {
            return Vec::new();
        }
        let fired: Vec<Alert> = self
            .engine
            .evaluate(&f.raw, now, f.received_ms)
            .into_iter()
            .map(|a| deliver(a, self.notifications, self.speech))
            .collect();
        for a in &fired {
            self.recent.push_front(a.clone());
        }
        self.recent.truncate(RECENT_MAX);
        fired
    }
}

impl Default for Running {
    fn default() -> Self {
        Self {
            session: 0,
            target: None,
            link: LinkStatus::Disabled,
            source: None,
            task: None,
            gate: TriggerGate::new(),
            actions_error: None,
            alerts: AlertHub::default(),
        }
    }
}

/// The running stream, if any, and the trigger gate. Cheap to clone (shared state).
#[derive(Clone, Default)]
pub struct TelemetryHost {
    state: Arc<Mutex<Running>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// One synthetic frame as the UI gets it.
pub fn mock_frame(schema: &Schema, seq: u64, t: f64, state: &mock::MockState) -> Frame {
    let raw = starorg_telemetry::json::to_json(&mock::frame(schema, t, state));
    Frame {
        seq,
        received_ms: now_ms(),
        snapshot: Snapshot::from_json(&raw),
        raw,
    }
}

/// The schema in contract form.
pub fn schema_info(s: &Schema) -> TelemetrySchemaInfo {
    TelemetrySchemaInfo {
        key: s.key().to_owned(),
        origin: match s.origin() {
            SchemaOrigin::Cache => "cache",
            SchemaOrigin::Extracted => "extracted",
            SchemaOrigin::Synthetic => "synthetic",
        }
        .into(),
        files: s.file_names().len() as u32,
    }
}

/// Overrides that match the defaults dropped, then checked against the rules: `Err` names
/// each override that does not fit.
pub fn alerts_normalize(a: &AlertSettings) -> Result<AlertSettings, String> {
    let defaults = alerts::defaults();
    let mut out = a.clone();
    out.overrides.clear();
    let mut errors = Vec::new();
    for (id, o) in &a.overrides {
        let Some(d) = defaults.iter().find(|r| &r.id == id) else {
            errors.push(format!("{id}: no such alert rule"));
            continue;
        };
        if let Err(e) = alerts::apply_override(d, o) {
            errors.push(format!("{id}: {e}"));
            continue;
        }
        let (dv, dr) = threshold(&d.trigger);
        let mut o = *o;
        if o.enabled == Some(d.enabled) {
            o.enabled = None;
        }
        if o.cooldown_s == Some(d.cooldown_s) {
            o.cooldown_s = None;
        }
        // The threshold and its re-arm point go together.
        if o.value.is_some_and(|v| Some(v) == dv) && o.rearm.is_none_or(|x| Some(x) == dr) {
            o.value = None;
            o.rearm = None;
        }
        if o.value.is_none() && o.rearm.is_some() && o.rearm == dr {
            o.rearm = None;
        }
        if !o.is_empty() {
            out.overrides.insert(id.clone(), o);
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors.join("; "))
    }
}

fn threshold(t: &Trigger) -> (Option<f64>, Option<f64>) {
    match t {
        Trigger::Threshold { value, rearm, .. } => (Some(*value), *rearm),
        _ => (None, None),
    }
}

/// Settings -> Live telemetry -> Notifications and the screen's Alerts panel: the rules as
/// the user has them next to their defaults, and the recent alerts.
pub fn alerts_view(host: &TelemetryHost, a: &AlertSettings) -> TelemetryAlerts {
    let defaults = alerts::defaults();
    let (rules, warnings) = alerts::apply_overrides(&defaults, &a.overrides);
    let view = rules
        .iter()
        .zip(&defaults)
        .map(|(r, d)| {
            let (value, rearm) = threshold(&r.trigger);
            let (dv, dr) = threshold(&d.trigger);
            let (path, op) = match &r.trigger {
                Trigger::Threshold { path, op, .. } => (Some(path.clone()), Some(op.as_str())),
                Trigger::Transition { path, .. } | Trigger::Edge { path, .. } => {
                    (Some(path.clone()), None)
                }
                Trigger::Composite { .. } => (None, None),
            };
            TelemetryAlertRule {
                id: r.id.clone(),
                description: r.description.clone(),
                kind: r.trigger.kind().into(),
                severity: r.severity,
                scope: r.scope,
                path,
                op: op.map(str::to_owned),
                unit: r.unit.clone(),
                message: r.message.clone(),
                sound: r.sound.clone(),
                verified: r.verified,
                debounce_s: r.debounce_s,
                enabled: r.enabled,
                cooldown_s: r.cooldown_s,
                threshold: value,
                rearm,
                default_enabled: d.enabled,
                default_cooldown_s: d.cooldown_s,
                default_threshold: dv,
                default_rearm: dr,
            }
        })
        .collect();
    let r = lock(&host.state);
    TelemetryAlerts {
        settings: a.clone(),
        rules: view,
        recent: r.alerts.recent.iter().cloned().collect(),
        recent_max: RECENT_MAX as u32,
        evaluating: r.source.is_some() && r.alerts.notifications != Notifications::Off,
        warnings,
    }
}

/// The dangerous actions, sorted (they always need a confirmation).
pub fn dangerous() -> Vec<String> {
    let mut v: Vec<String> = DANGEROUS.iter().map(|s| (*s).to_owned()).collect();
    v.sort();
    v
}

impl TelemetryHost {
    pub fn session(&self) -> u64 {
        lock(&self.state).session
    }

    pub fn link(&self) -> LinkStatus {
        lock(&self.state).link.clone()
    }

    /// The channel the stream runs for.
    pub fn target(&self) -> Option<ChannelRef> {
        lock(&self.state).target.clone()
    }

    pub fn schema(&self) -> Option<TelemetrySchemaInfo> {
        lock(&self.state)
            .source
            .as_ref()
            .map(|s| schema_info(s.schema()))
    }

    /// The user's alert settings (rules with their overrides, delivery); a bad override is
    /// skipped and named in the result. Every rule takes the next frame as its baseline.
    pub fn set_alerts(&self, a: &AlertSettings) -> Vec<String> {
        let (rules, warnings) = alerts::apply_overrides(&alerts::defaults(), &a.overrides);
        let mut r = lock(&self.state);
        let h = &mut r.alerts;
        if h.notifications == Notifications::Off && a.notifications != Notifications::Off {
            // Turned on mid-stream: what holds now is the baseline.
            h.engine.reset();
        }
        h.engine.set_rules(rules);
        h.notifications = a.notifications;
        h.speech = a.speech;
        warnings
    }

    /// Stop any stream and start a new session for `target`; returns its number.
    fn begin(&self, r: &mut Running, target: ChannelRef) -> u64 {
        if let Some(t) = r.task.take() {
            t.stop();
        }
        // A new stream starts with a baseline, never with what the last one saw.
        r.alerts.engine.reset();
        r.session += 1;
        r.target = Some(target);
        r.source = None;
        r.actions_error = None;
        // The allowlist belongs to the old stream's build: a new one starts empty, as on stop.
        r.gate.clear_actions();
        r.session
    }

    /// Record a status of `session` and evaluate the alerts of a frame (`None` when a newer
    /// stream started: the event is stale).
    fn note(state: &Mutex<Running>, session: u64, ev: &LiveEvent) -> Option<Vec<Alert>> {
        let mut r = lock(state);
        if r.session != session {
            return None;
        }
        match ev {
            LiveEvent::Status(s) => {
                r.link = s.clone();
                Some(Vec::new())
            }
            LiveEvent::Frame(f) => Some(r.alerts.frame(f)),
        }
    }

    fn forward(state: &Mutex<Running>, sink: &Sink, session: u64, ev: LiveEvent) -> bool {
        let Some(alerts) = Self::note(state, session, &ev) else {
            return false;
        };
        let mut e = TelemetryEvent::of(session, ev);
        e.alerts = alerts;
        sink(e);
        true
    }

    /// No stream: the schema could not be had (`reason` shown on the screen).
    pub fn no_schema(&self, target: ChannelRef, reason: String, sink: &Sink) {
        let session = {
            let mut r = lock(&self.state);
            self.begin(&mut r, target)
        };
        let ev = LiveEvent::Status(LinkStatus::NoSchema { reason });
        Self::forward(&self.state, sink, session, ev);
    }

    /// Stream the game's service at `addr` (Tauri's runtime).
    pub fn start_game(
        &self,
        target: ChannelRef,
        schema: Arc<Schema>,
        addr: LoopbackAddr,
        sink: Sink,
    ) {
        let mut r = lock(&self.state);
        let session = self.begin(&mut r, target);
        r.source = Some(Source::Game {
            schema: schema.clone(),
            addr,
        });
        r.link = LinkStatus::Connecting;
        let state = self.state.clone();
        let fut = live::run(schema, addr, LiveOptions::default(), move |ev| {
            Self::forward(&state, &sink, session, ev)
        });
        r.task = Some(Task::Live(tauri::async_runtime::spawn(fut)));
    }

    /// Stream the synthetic generator: about 20 frames a second like the game, throttled to
    /// the UI rate like a real stream (a thread of its own; no game, no socket).
    pub fn start_mock(&self, target: ChannelRef, sink: Sink) {
        let schema = Arc::new(mock::schema());
        let game = Arc::new(Mutex::new(MockGame::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let session = {
            let mut r = lock(&self.state);
            let session = self.begin(&mut r, target);
            r.source = Some(Source::Mock {
                schema: schema.clone(),
                game: game.clone(),
            });
            r.link = LinkStatus::Connecting;
            r.task = Some(Task::Mock(stop.clone()));
            session
        };
        let state = self.state.clone();
        let started = std::thread::Builder::new()
            .name("starorg-telemetry-mock".into())
            .spawn(move || {
                let t0 = Instant::now();
                let send =
                    |ev| !stop.load(Ordering::Relaxed) && Self::forward(&state, &sink, session, ev);
                if !send(LiveEvent::Status(LinkStatus::Connecting))
                    || !send(LiveEvent::Status(LinkStatus::Connected))
                {
                    return;
                }
                let mut seq = 0u64;
                loop {
                    // The game sends about 20 a second; every second one reaches the UI.
                    std::thread::sleep(UI_INTERVAL);
                    seq += 2;
                    let st = lock(&game).state;
                    let f = mock_frame(&schema, seq, t0.elapsed().as_secs_f64(), &st);
                    if !send(LiveEvent::Frame(Box::new(f))) {
                        return;
                    }
                }
            });
        if let Err(e) = started {
            let mut r = lock(&self.state);
            if r.session == session {
                r.link = LinkStatus::Unreachable {
                    reason: format!("cannot start the mock stream: {e}"),
                };
            }
        }
    }

    /// Stop the stream (if any). The gate keeps its switch; its action list goes.
    pub fn stop(&self) {
        let mut r = lock(&self.state);
        if let Some(t) = r.task.take() {
            t.stop();
        }
        if r.target.is_some() || r.source.is_some() {
            r.session += 1;
        }
        r.target = None;
        r.source = None;
        r.link = LinkStatus::Disabled;
        r.actions_error = None;
        r.gate.clear_actions();
        r.alerts.engine.reset();
    }

    /// Run `f` against the game with a fresh client (blocking; called on the blocking pool).
    fn with_client<T>(
        schema: Arc<Schema>,
        addr: LoopbackAddr,
        f: impl AsyncFnOnce(&mut TelemetryClient) -> Result<T, starorg_telemetry::client::ClientError>,
    ) -> Result<T, String> {
        tauri::async_runtime::block_on(async move {
            let mut c = TelemetryClient::connect(schema, addr)
                .await
                .map_err(|e| e.to_string())?;
            f(&mut c).await.map_err(|e| e.to_string())
        })
    }

    /// The game's triggerable actions (`ListTriggerableActions`), kept in the gate. The game
    /// answers an empty list until the player has spawned; the last non-empty one stays.
    pub fn actions(&self, triggers_active: bool) -> TelemetryActions {
        let source = lock(&self.state).source.clone();
        let (fresh, error, from) = match &source {
            None => (Vec::new(), None, "none"),
            Some(Source::Mock { game, .. }) => (lock(game).actions().to_vec(), None, "mock"),
            Some(Source::Game { schema, addr }) => {
                match Self::with_client(schema.clone(), *addr, async |c| c.list_actions().await) {
                    Ok(a) => (a, None, "game"),
                    Err(e) => (Vec::new(), Some(e), "game"),
                }
            }
        };
        let mut r = lock(&self.state);
        r.gate.set_enabled(triggers_active);
        // Kept even while triggers are off: the list is shown either way.
        let mut list = fresh.clone();
        if triggers_active {
            r.gate.set_actions(&fresh);
            if fresh.is_empty() {
                list = r.gate.actions().into_iter().cloned().collect();
            }
        }
        r.actions_error = error.clone();
        list.sort_by(|a: &TriggerableAction, b| {
            (a.category.as_str(), a.action_name.as_str())
                .cmp(&(b.category.as_str(), b.action_name.as_str()))
        });
        TelemetryActions {
            actions: list,
            triggers_active,
            dangerous: dangerous(),
            source: from.into(),
            error,
        }
    }

    /// One press, through the gate. `triggers_active`: the user's switches (telemetry and
    /// "Allow game actions" both on).
    pub fn trigger(
        &self,
        action: &str,
        hold_ms: Option<u32>,
        confirmed: bool,
        triggers_active: bool,
    ) -> TelemetryTriggerResult {
        let refused = |r: starorg_telemetry::trigger::Refusal| TelemetryTriggerResult {
            action: action.to_owned(),
            outcome: None,
            refused: Some(TelemetryRefusal::of(&r)),
            error: None,
        };
        let failed = |e: String| TelemetryTriggerResult {
            action: action.to_owned(),
            outcome: None,
            refused: None,
            error: Some(e),
        };
        if triggers_active && lock(&self.state).source.is_none() {
            return failed("live telemetry is not running; open the Live telemetry screen".into());
        }
        let need_list = {
            let mut r = lock(&self.state);
            r.gate.set_enabled(triggers_active);
            triggers_active && r.gate.actions().is_empty()
        };
        if need_list {
            self.actions(triggers_active);
        }
        let source = {
            let mut r = lock(&self.state);
            if let Err(e) = r.gate.check(action, hold_ms, confirmed, Instant::now()) {
                return refused(e);
            }
            r.source.clone()
        };
        let sent = match source {
            None => Err("live telemetry is not running; open the Live telemetry screen".to_owned()),
            Some(Source::Mock { game, .. }) => {
                Ok(lock(&game).trigger(action, hold_ms, Instant::now()))
            }
            Some(Source::Game { schema, addr }) => {
                let name = action.to_owned();
                Self::with_client(schema, addr, async move |c| c.trigger(&name, hold_ms).await)
            }
        };
        match sent {
            Ok(o) => TelemetryTriggerResult {
                action: action.to_owned(),
                outcome: Some(o),
                refused: None,
                error: None,
            },
            Err(e) => failed(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(ch: &str) -> ChannelRef {
        ChannelRef {
            install: "inst".into(),
            channel: ch.into(),
        }
    }

    fn quiet() -> Sink {
        Arc::new(|_| {})
    }

    #[test]
    fn a_new_stream_starts_with_an_empty_allowlist() {
        let host = TelemetryHost::default();
        host.start_mock(target("PTU"), quiet());
        assert!(!host.actions(true).actions.is_empty());
        assert!(!lock(&host.state).gate.actions().is_empty());
        // Another channel without stopping first: the old build's list is gone.
        host.no_schema(target("LIVE"), "test".into(), &quiet());
        assert!(lock(&host.state).gate.actions().is_empty());
        assert!(host.actions(true).actions.is_empty());
        host.stop();
    }

    #[test]
    fn triggers_off_refuses_before_any_request() {
        let host = TelemetryHost::default();
        // A game source on a closed loopback port: any request would come back as an error.
        lock(&host.state).source = Some(Source::Game {
            schema: Arc::new(mock::schema()),
            addr: LoopbackAddr::localhost(1).unwrap(),
        });
        let r = host.trigger("v_toggle_lights", None, true, false);
        assert!(r.refused.is_some(), "{:?}", r.error);
        assert!(r.error.is_none() && r.outcome.is_none());
    }

    fn boost(b: f64, seq: u64) -> Frame {
        let raw = serde_json::json!({"vehicle": {"state": {"name": "Test Ship",
            "power": {"engines": true},
            "flight": {"instruments": {"boost": {"reserves": b}}}}}});
        Frame {
            seq,
            received_ms: 1_000 + seq,
            snapshot: Snapshot::from_json(&raw),
            raw,
        }
    }

    /// 2 s of full boost, then 1 s at 20 %: one `boost_low` crossing at 2.0 s.
    fn crossing(h: &mut AlertHub) -> Vec<Alert> {
        let mut out = Vec::new();
        for i in 0..30u64 {
            let b = if i < 20 { 0.9 } else { 0.2 };
            out.extend(h.frame_at(&boost(b, i), i as f64 * 0.1));
        }
        out
    }

    fn hub(n: Notifications, speech: bool) -> AlertHub {
        AlertHub {
            notifications: n,
            speech,
            ..AlertHub::default()
        }
    }

    #[test]
    fn alerts_only_with_notifications_on_and_only_what_was_chosen() {
        let mut h = hub(Notifications::Off, true);
        assert!(crossing(&mut h).is_empty(), "off: nothing is evaluated");
        assert!(h.recent.is_empty());

        let mut h = hub(Notifications::InApp, true);
        let a = crossing(&mut h);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].rule, "boost_low");
        assert!(
            a[0].sound.is_none() && a[0].speech.is_none(),
            "in app: silent"
        );
        assert_eq!(h.recent.len(), 1);

        let mut h = hub(Notifications::InAppSound, false);
        let a = crossing(&mut h);
        assert_eq!(a[0].sound.as_deref(), Some("warn"));
        assert!(a[0].speech.is_none(), "speech is its own switch");

        let mut h = hub(Notifications::InAppSound, true);
        let a = crossing(&mut h);
        assert_eq!(a[0].speech.as_deref(), Some("Boost twenty percent"));
    }

    #[test]
    fn the_recent_list_is_bounded() {
        let mut h = hub(Notifications::InApp, false);
        for _ in 0..RECENT_MAX + 5 {
            h.recent
                .push_front(h.engine.test_alert("hull_half", 0).unwrap());
        }
        h.frame_at(&boost(0.9, 0), 0.0);
        assert_eq!(h.recent.len(), RECENT_MAX);
    }

    #[test]
    fn stale_events_carry_no_alerts_and_a_restart_rebaselines() {
        let host = TelemetryHost::default();
        host.set_alerts(&AlertSettings {
            notifications: Notifications::InApp,
            ..AlertSettings::default()
        });
        let session = host.session();
        let ev = LiveEvent::Frame(Box::new(boost(0.2, 1)));
        assert!(TelemetryHost::note(&host.state, session + 1, &ev).is_none());
        assert_eq!(
            TelemetryHost::note(&host.state, session, &ev),
            Some(Vec::new()),
            "the first frame is a baseline"
        );
        host.no_schema(target("PTU"), "test".into(), &quiet());
        assert!(lock(&host.state).alerts.engine.rules().len() > 10);
    }

    #[test]
    fn normalize_drops_defaults_and_refuses_misfits() {
        let mut a = AlertSettings::default();
        a.overrides.insert(
            "boost_low".into(),
            alerts::RuleOverride {
                enabled: Some(true),
                cooldown_s: Some(15.0),
                value: Some(0.25),
                rearm: None,
            },
        );
        a.overrides.insert(
            "hull_half".into(),
            alerts::RuleOverride {
                enabled: Some(false),
                ..Default::default()
            },
        );
        let n = alerts_normalize(&a).unwrap();
        assert_eq!(n.overrides.len(), 1, "{:?}", n.overrides);
        assert_eq!(n.overrides["hull_half"].enabled, Some(false));
        a.overrides.insert("nope".into(), Default::default());
        assert!(alerts_normalize(&a).unwrap_err().contains("nope"));

        let host = TelemetryHost::default();
        let v = alerts_view(&host, &n);
        let hull = v.rules.iter().find(|r| r.id == "hull_half").unwrap();
        assert!(!hull.enabled && hull.default_enabled);
        assert_eq!(hull.threshold, Some(0.5));
        assert_eq!(hull.unit.as_deref(), Some("fraction"));
        assert!(!v.evaluating && v.recent.is_empty() && v.warnings.is_empty());
    }

    #[test]
    fn a_test_alert_follows_the_delivery_choice() {
        let mut a = AlertSettings::default();
        assert!(alert_test(&a, "hull_half").is_err(), "refused while off");
        a.notifications = Notifications::InApp;
        let t = alert_test(&a, "hull_half").unwrap();
        assert!(t.test && t.sound.is_none());
        a.notifications = Notifications::InAppSound;
        assert_eq!(
            alert_test(&a, "hull_half").unwrap().sound.as_deref(),
            Some("warn")
        );
        assert!(alert_test(&a, "nope").is_err());
    }
}
