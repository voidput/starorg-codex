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
//! Around the stream (sc-telemetry `server.py` `history`, `--record`, `--replay`):
//! - every frame handed to the UI also goes into a 10-minute [`History`] (one sample a second)
//!   for the graphs; it is kept across a stop and start of the same channel and cleared for
//!   another channel, a replay or a seek;
//! - with "Record sessions" on ([`TelemetryHost::set_record`]) the frames of a live stream go
//!   to a [`Recorder`] in the recordings folder, a new file per stream, opened at its first
//!   frame. The mock backend has no folder: its recordings are a list in memory;
//! - a replay plays a recording through the same [`Sink`] on a thread of its own, at the
//!   recorded pace times its speed, with pause and seek ([`TelemetryHost::replay_control`]).
//!
//! Alerts ([`starorg_telemetry::alerts`]) are evaluated on the stream's frames only while a
//! stream runs and the user's own Notifications choice is not Off (it is Off by default and
//! independent of Metrics and actions). They ride on the frame event that fired them and are
//! kept in a short list ([`RECENT_MAX`]) for the screen's Alerts panel. The host strips the
//! tone and the speech text unless the user chose sound (and speech), so the webview plays
//! only what was asked for. A replay never fires alerts: they are of the live game only.
//!
//! With "Read Game.log events" on, the same stream session also follows the channel's
//! `Game.log` ([`starorg_telemetry::gamelog`]) on a thread of its own: polled every 500 ms,
//! only new bytes read, the view sent at most twice a second and only when it changed. With
//! "People" on as well, the handles Game.log names are kept in memory for the People panel;
//! they are never logged, and [`TelemetryHost::gamelog_handles`] feeds the diagnostics
//! redaction. The mock backend feeds synthetic lines instead of a file.

use crate::contract::telemetry::{
    TelemetryActions, TelemetryAlertRule, TelemetryAlerts, TelemetryEvent, TelemetryRecording,
    TelemetryRefusal, TelemetryReplayState, TelemetrySchemaInfo, TelemetryTriggerResult,
};
use crate::contract::types::ChannelRef;
use starorg_telemetry::alerts::{self, Alert, AlertEngine, AlertSettings, Notifications, Trigger};
use starorg_telemetry::client::{LoopbackAddr, TelemetryClient};
use starorg_telemetry::frames::{Frame, Snapshot, UI_INTERVAL};
use starorg_telemetry::gamelog::stints::{self, Backfill, Stint, Store};
use starorg_telemetry::gamelog::{self, GameLog, GameLogView};
use starorg_telemetry::history::{History, HistoryView};
use starorg_telemetry::live::{self, LinkStatus, LiveEvent, LiveOptions};
use starorg_telemetry::mock::{self, MockGame};
use starorg_telemetry::record::{self, Caps, Recorder, Stopped};
use starorg_telemetry::replay::{self, FileReplay, ReplaySource, SyntheticReplay};
use starorg_telemetry::schema::{Schema, SchemaOrigin};
use starorg_telemetry::trigger::{DANGEROUS, TriggerGate, TriggerableAction};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    /// The mock stream or a replay: a thread that ends when the flag is set.
    Thread(Arc<AtomicBool>),
}

impl Task {
    fn stop(self) {
        match self {
            Task::Live(h) => h.abort(),
            Task::Thread(stop) => stop.store(true, Ordering::Relaxed),
        }
    }
}

/// A replay's controls, shared by the player thread and the commands.
#[derive(Debug, Clone)]
struct ReplayCtl {
    name: String,
    start_ms: u64,
    duration_ms: u64,
    position_ms: u64,
    playing: bool,
    speed: f64,
    ended: bool,
    /// A seek asked for, taken by the player.
    seek: Option<u64>,
}

impl ReplayCtl {
    fn state(&self) -> TelemetryReplayState {
        TelemetryReplayState {
            name: self.name.clone(),
            start_ms: self.start_ms,
            position_ms: self.position_ms,
            duration_ms: self.duration_ms,
            playing: self.playing,
            speed: self.speed,
            ended: self.ended,
        }
    }
}

/// Where the Game.log reader reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameLogSource {
    /// The channel folder: its `Game.log`, in the on-disk spelling.
    Channel(PathBuf),
    /// Synthetic lines (`starorg_telemetry::gamelog::mock`), no file.
    Mock,
}

/// What the reader thread and the commands share.
struct GameLogShared {
    log: GameLog,
    source: &'static str,
    found: bool,
    rotations: u32,
    /// Backups still to read into the join history.
    backfill_pending: u32,
}

impl GameLogShared {
    fn view(&self, sessions_rev: u64) -> GameLogView {
        let mut v = self.log.view(self.source, self.found, self.rotations);
        v.sessions_rev = sessions_rev;
        v
    }
}

struct GameLogTask {
    source: GameLogSource,
    people: bool,
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<GameLogShared>>,
}

impl GameLogTask {
    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The view goes to the window at most this often.
const GAMELOG_EMIT: Duration = Duration::from_millis(500);
/// A mock line this often.
const GAMELOG_MOCK_EVERY: Duration = Duration::from_secs(4);

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
    /// The replay playing instead of a stream.
    replay: Option<Arc<Mutex<ReplayCtl>>>,
    /// What the history holds (`live <install>/<CHANNEL>` or `replay <name>`).
    history_of: Option<String>,
    alerts: AlertHub,
    /// Where the stream's events go (the Game.log reader sends there too).
    sink: Option<Sink>,
    gamelog: Option<GameLogTask>,
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
            replay: None,
            history_of: None,
            alerts: AlertHub::default(),
            sink: None,
            gamelog: None,
        }
    }
}

/// "Record sessions" and the recording being written.
struct RecSlot {
    on: bool,
    /// The recordings folder; `None` on the mock backend (recordings in memory only).
    dir: Option<PathBuf>,
    caps: Caps,
    /// The file being written and the stream it belongs to.
    current: Option<(u64, Recorder)>,
    /// A stream whose recording could not be opened (not tried again).
    failed: Option<u64>,
    /// Streams up to this one have ended: a late frame of theirs opens nothing.
    closed: u64,
    error: Option<String>,
    /// The mock's recordings, newest last, and the one growing (session, name).
    mock: Vec<TelemetryRecording>,
    mock_current: Option<(u64, String)>,
}

impl RecSlot {
    fn new(dir: Option<PathBuf>) -> Self {
        Self {
            on: false,
            dir,
            caps: Caps::default(),
            current: None,
            failed: None,
            closed: 0,
            error: None,
            mock: Vec::new(),
            mock_current: None,
        }
    }

    /// Streams up to `session` have ended: close their recording.
    fn close(&mut self, session: u64) {
        self.closed = self.closed.max(session);
        self.finish();
    }

    /// Close the file of the current recording (if any); its name.
    fn finish(&mut self) -> Option<String> {
        self.mock_current = None;
        let (_, r) = self.current.take()?;
        match r.finish() {
            Ok(n) => Some(n),
            Err(e) => {
                self.error = Some(format!("the recording could not be finished: {e}"));
                None
            }
        }
    }

    fn recording(&self) -> Option<String> {
        if let Some((_, r)) = &self.current {
            return Some(r.name().to_owned());
        }
        self.mock_current.as_ref().map(|(_, n)| n.clone())
    }
}

/// The running stream, if any, the trigger gate, the graphs' history and the recorder.
/// Cheap to clone (shared state).
#[derive(Clone)]
pub struct TelemetryHost {
    state: Arc<Mutex<Running>>,
    history: Arc<Mutex<History>>,
    rec: Arc<Mutex<RecSlot>>,
    /// The server and shard join history (a file in the data folder; memory on the mock).
    sessions: Arc<Mutex<Store>>,
}

impl Default for TelemetryHost {
    /// No recordings folder: recordings stay in memory (the mock backend, tests).
    fn default() -> Self {
        Self::with_recordings(None)
    }
}

/// The mock's recordings: a frame of the synthetic stream written is about this big.
const MOCK_FRAME_BYTES: u64 = 1_800;

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

/// The recordings list of `host` in contract form; `folder` is shown only.
pub fn recordings_view(
    host: &TelemetryHost,
    folder: String,
) -> crate::contract::telemetry::TelemetryRecordings {
    let caps = host.caps();
    crate::contract::telemetry::TelemetryRecordings {
        items: host.recordings(),
        folder,
        recording: host.recording(),
        replaying: host.replay_state().map(|r| r.name),
        max_files: caps.files as u32,
        max_total_bytes: caps.total_bytes,
        max_file_bytes: caps.file_bytes,
        record_hz: starorg_telemetry::record::RECORD_HZ,
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
    /// Recordings in `dir` (the real backend: `<data>/telemetry/recordings`), or in memory.
    pub fn with_recordings(dir: Option<PathBuf>) -> Self {
        Self::with_dirs(dir, None)
    }

    /// Recordings in `recordings`, the join history in `sessions` (`None`: memory only).
    pub fn with_dirs(recordings: Option<PathBuf>, sessions: Option<PathBuf>) -> Self {
        Self {
            state: Arc::default(),
            history: Arc::default(),
            rec: Arc::new(Mutex::new(RecSlot::new(recordings))),
            sessions: Arc::new(Mutex::new(Store::new(sessions))),
        }
    }

    /// Replace the recordings' bounds (tests).
    pub fn set_caps(&self, caps: Caps) {
        lock(&self.rec).caps = caps;
    }

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

    /// The replay playing, if any.
    pub fn replay_state(&self) -> Option<TelemetryReplayState> {
        let ctl = lock(&self.state).replay.clone()?;
        Some(lock(&ctl).state())
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

    /// Stop any stream or replay and start a new session for `target` (`None`: a replay);
    /// returns its number. The history goes unless it is of the same thing (`history_of`).
    fn begin(
        &self,
        r: &mut Running,
        target: Option<ChannelRef>,
        history_of: String,
        sink: &Sink,
    ) -> u64 {
        if let Some(t) = r.task.take() {
            t.stop();
        }
        lock(&self.rec).close(r.session);
        // A new stream starts with a baseline, never with what the last one saw.
        r.alerts.engine.reset();
        if let Some(g) = r.gamelog.take() {
            g.stop();
        }
        r.sink = Some(sink.clone());
        r.session += 1;
        r.target = target;
        r.source = None;
        r.replay = None;
        r.actions_error = None;
        // The allowlist belongs to the old stream's build: a new one starts empty, as on stop.
        r.gate.clear_actions();
        if r.history_of.as_deref() != Some(history_of.as_str()) {
            lock(&self.history).clear();
            r.history_of = Some(history_of);
        }
        r.session
    }

    fn live_key(target: &ChannelRef) -> String {
        format!("live {}/{}", target.install, target.channel)
    }

    /// Hand an event of `session` on (dropped when a newer stream started; then false). A
    /// frame of a live stream also goes into the history, with "Record sessions" on the
    /// recording, and through the alerts (none while Notifications is Off).
    fn forward(&self, sink: &Sink, session: u64, ev: LiveEvent) -> bool {
        let (meta, alerts) = {
            let mut r = lock(&self.state);
            if r.session != session {
                return false;
            }
            let alerts = match &ev {
                LiveEvent::Status(s) => {
                    r.link = s.clone();
                    Vec::new()
                }
                LiveEvent::Frame(f) => r.alerts.frame(f),
            };
            let channel = r.target.as_ref().map(|t| t.channel.clone());
            let key = r.source.as_ref().map(|s| s.schema().key().to_owned());
            ((channel, key), alerts)
        };
        if let LiveEvent::Frame(f) = &ev {
            lock(&self.history).push(f.received_ms, &f.snapshot);
            if let (Some(channel), Some(key)) = meta {
                self.record_frame(session, &channel, &key, f);
            }
        }
        let mut e = TelemetryEvent::of(session, ev);
        e.alerts = alerts;
        sink(e);
        true
    }

    /// Write a live frame to the stream's recording (opened at its first frame).
    fn record_frame(&self, session: u64, channel: &str, schema_key: &str, f: &Frame) {
        let mut rec = lock(&self.rec);
        if !rec.on || rec.failed == Some(session) || session <= rec.closed {
            return;
        }
        if rec.current.as_ref().is_some_and(|(s, _)| *s != session)
            || rec
                .mock_current
                .as_ref()
                .is_some_and(|(s, _)| *s != session)
        {
            rec.finish();
        }
        let Some(dir) = rec.dir.clone() else {
            Self::record_mock(&mut rec, session, channel, schema_key, f);
            return;
        };
        if rec.current.is_none() {
            match Recorder::create(&dir, channel, schema_key, f.received_ms, rec.caps) {
                Ok(r) => {
                    rec.error = None;
                    rec.current = Some((session, r));
                }
                Err(e) => {
                    rec.failed = Some(session);
                    rec.error = Some(format!(
                        "cannot start a recording in {}: {e}",
                        dir.display()
                    ));
                    return;
                }
            }
        }
        if let Some((_, r)) = rec.current.as_mut() {
            r.frame(f.received_ms, &f.raw);
            let why = r.stopped();
            match why {
                Some(Stopped::Full) => {
                    rec.error = Some(
                        "the recording reached its size cap and stopped; the next stream starts a new one"
                            .into(),
                    );
                    rec.failed = Some(session);
                    rec.finish();
                }
                Some(Stopped::Failed) => {
                    rec.error = Some("writing the recording failed; it stopped".into());
                    rec.failed = Some(session);
                    rec.finish();
                }
                None => {}
            }
        }
    }

    /// The mock backend's recording: an entry in memory that grows with the stream.
    fn record_mock(rec: &mut RecSlot, session: u64, channel: &str, schema: &str, f: &Frame) {
        let name = match &rec.mock_current {
            Some((_, n)) => n.clone(),
            None => {
                let max = rec.caps.files.max(1);
                while rec.mock.len() >= max {
                    rec.mock.remove(0);
                }
                let mut name = format!("{}-{channel}.jsonl", record::stamp(f.received_ms));
                let mut n = 2;
                while rec.mock.iter().any(|m| m.name == name) {
                    name = format!("{}-{channel}-{n}.jsonl", record::stamp(f.received_ms));
                    n += 1;
                }
                rec.mock.push(TelemetryRecording {
                    name: name.clone(),
                    bytes: 0,
                    started_ms: Some(f.received_ms),
                    duration_ms: Some(0),
                    channel: Some(channel.to_owned()),
                    schema: Some(schema.to_owned()),
                });
                rec.mock_current = Some((session, name.clone()));
                name
            }
        };
        let step = rec.caps.interval.as_millis().max(1) as u64;
        if let Some(m) = rec.mock.iter_mut().find(|m| m.name == name) {
            let d = f
                .received_ms
                .saturating_sub(m.started_ms.unwrap_or(f.received_ms));
            m.duration_ms = Some(d);
            m.bytes = 200 + (d / step + 1) * MOCK_FRAME_BYTES;
        }
    }

    /// "Record sessions" on or off. Off ends the recording in progress at once.
    pub fn set_record(&self, on: bool) {
        let mut rec = lock(&self.rec);
        rec.on = on;
        if !on {
            rec.finish();
            rec.failed = None;
        }
    }

    pub fn record_on(&self) -> bool {
        lock(&self.rec).on
    }

    /// The recording being written now.
    pub fn recording(&self) -> Option<String> {
        lock(&self.rec).recording()
    }

    pub fn record_error(&self) -> Option<String> {
        lock(&self.rec).error.clone()
    }

    pub fn recordings_dir(&self) -> Option<PathBuf> {
        lock(&self.rec).dir.clone()
    }

    pub fn caps(&self) -> Caps {
        lock(&self.rec).caps
    }

    /// Every recording, newest first.
    pub fn recordings(&self) -> Vec<TelemetryRecording> {
        let rec = lock(&self.rec);
        match &rec.dir {
            Some(d) => record::list(d),
            None => {
                let mut v = rec.mock.clone();
                v.reverse();
                v
            }
        }
    }

    /// Remove a recording; refused for the one being written or played.
    pub fn delete_recording(&self, name: &str) -> Result<(), String> {
        if !record::valid_name(name) {
            return Err(format!("{name} is not a recording"));
        }
        if self.replay_state().is_some_and(|r| r.name == name) {
            return Err("This recording is playing; stop the replay first.".into());
        }
        let mut rec = lock(&self.rec);
        if rec.recording().as_deref() == Some(name) {
            return Err("This recording is being written; stop the stream first.".into());
        }
        match rec.dir.clone() {
            Some(d) => {
                drop(rec);
                // A replay just stopped may still hold the file for a moment (Windows).
                let mut tries = 0;
                loop {
                    match record::delete(&d, name) {
                        Ok(()) => return Ok(()),
                        Err(e)
                            if e.kind() == std::io::ErrorKind::PermissionDenied && tries < 10 =>
                        {
                            tries += 1;
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(e) => return Err(format!("{name} not deleted: {e}")),
                    }
                }
            }
            None => {
                let before = rec.mock.len();
                rec.mock.retain(|m| m.name != name);
                if rec.mock.len() == before {
                    return Err(format!("No recording {name}"));
                }
                Ok(())
            }
        }
    }

    /// Seed the mock's list (synthetic recordings, so the screen has some to show).
    pub fn seed_mock_recordings(&self, items: Vec<TelemetryRecording>) {
        let mut rec = lock(&self.rec);
        if rec.dir.is_none() {
            rec.mock = items;
            rec.mock_current = None;
        }
    }

    /// Synthetic join history rows (the mock backend; ignored with a folder).
    pub fn seed_mock_sessions(&self, rows: Vec<Stint>) {
        let mut s = lock(&self.sessions);
        if s.path().is_none() {
            s.add(rows, None);
        }
    }

    /// The join history, newest first, with the backups still to read.
    pub fn sessions_view(&self) -> crate::contract::telemetry::TelemetrySessions {
        let pending = {
            let r = lock(&self.state);
            r.gamelog
                .as_ref()
                .map_or(0, |g| lock(&g.shared).backfill_pending)
        };
        let mut s = lock(&self.sessions);
        let items = s.rows();
        crate::contract::telemetry::TelemetrySessions {
            items,
            max_items: stints::MAX_ROWS as u32,
            path: s.path().map(|p| p.display().to_string()),
            revision: s.revision,
            backfill_pending: pending,
            error: s.error.clone(),
        }
    }

    /// Forget the whole join history (backups read stay read).
    pub fn clear_sessions(&self) -> crate::contract::telemetry::TelemetrySessions {
        lock(&self.sessions).clear();
        self.sessions_view()
    }

    /// The history file and its size (the mock's Storage row).
    pub fn sessions_usage(&self) -> (u64, u64) {
        let mut s = lock(&self.sessions);
        let rows = s.rows();
        if rows.is_empty() {
            return (0, 0);
        }
        (serde_json::to_vec(&rows).map_or(0, |v| v.len() as u64), 1)
    }

    /// The graphs' history since the caller's last answer.
    pub fn history(&self, epoch: Option<u64>, since_ms: Option<u64>) -> HistoryView {
        lock(&self.history).since(epoch, since_ms)
    }

    /// No stream: the schema could not be had (`reason` shown on the screen).
    pub fn no_schema(&self, target: ChannelRef, reason: String, sink: &Sink) {
        let session = {
            let mut r = lock(&self.state);
            let key = Self::live_key(&target);
            self.begin(&mut r, Some(target), key, sink)
        };
        let ev = LiveEvent::Status(LinkStatus::NoSchema { reason });
        self.forward(sink, session, ev);
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
        let key = Self::live_key(&target);
        let session = self.begin(&mut r, Some(target), key, &sink);
        r.source = Some(Source::Game {
            schema: schema.clone(),
            addr,
        });
        r.link = LinkStatus::Connecting;
        let host = self.clone();
        let fut = live::run(schema, addr, LiveOptions::default(), move |ev| {
            host.forward(&sink, session, ev)
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
            let key = Self::live_key(&target);
            let session = self.begin(&mut r, Some(target), key, &sink);
            r.source = Some(Source::Mock {
                schema: schema.clone(),
                game: game.clone(),
            });
            r.link = LinkStatus::Connecting;
            r.task = Some(Task::Thread(stop.clone()));
            session
        };
        let host = self.clone();
        let started = std::thread::Builder::new()
            .name("starorg-telemetry-mock".into())
            .spawn(move || {
                let t0 = Instant::now();
                let send = |ev| !stop.load(Ordering::Relaxed) && host.forward(&sink, session, ev);
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

    /// Stop the stream or replay (if any). The gate keeps its switch; its action list goes.
    /// The history stays for the same channel; a replay's goes.
    pub fn stop(&self) {
        let mut r = lock(&self.state);
        if let Some(t) = r.task.take() {
            t.stop();
        }
        lock(&self.rec).close(r.session);
        if let Some(g) = r.gamelog.take() {
            g.stop();
        }
        r.sink = None;
        if r.target.is_some() || r.source.is_some() || r.replay.is_some() {
            r.session += 1;
        }
        if r.replay.take().is_some() {
            lock(&self.history).clear();
            r.history_of = None;
        }
        r.target = None;
        r.source = None;
        r.link = LinkStatus::Disabled;
        r.actions_error = None;
        r.gate.clear_actions();
        r.alerts.engine.reset();
    }

    /// Play recording `name` into `sink` in place of a stream (from the start, playing, at
    /// speed 1). Fails when it cannot be read or holds no frame; a running stream then goes
    /// on.
    pub fn start_replay(&self, name: &str, sink: Sink) -> Result<TelemetryReplayState, String> {
        if !record::valid_name(name) {
            return Err(format!("{name} is not a recording"));
        }
        let (dir, mock) = {
            let mut rec = lock(&self.rec);
            // The file being written is closed first, so the replay reads all of it.
            if rec.recording().as_deref() == Some(name) {
                rec.finish();
            }
            let m = rec.mock.iter().find(|m| m.name == name).cloned();
            (rec.dir.clone(), m)
        };
        let src: Box<dyn ReplaySource> = match dir {
            Some(d) => {
                let p = record::path_of(&d, name).map_err(|e| e.to_string())?;
                Box::new(FileReplay::open(&p).map_err(|e| format!("{name} cannot be read: {e}"))?)
            }
            None => {
                let m = mock.ok_or_else(|| format!("No recording {name}"))?;
                Box::new(SyntheticReplay::new(
                    m.started_ms.unwrap_or(0),
                    m.duration_ms.unwrap_or(0),
                ))
            }
        };
        if src.is_empty() {
            return Err(format!("{name} holds no frame to replay"));
        }
        let ctl = Arc::new(Mutex::new(ReplayCtl {
            name: name.to_owned(),
            start_ms: src.start_ms(),
            duration_ms: src.duration_ms(),
            position_ms: 0,
            playing: true,
            speed: 1.0,
            ended: false,
            seek: Some(0),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let session = {
            let mut r = lock(&self.state);
            let session = self.begin(&mut r, None, format!("replay {name}"), &sink);
            // A replay always starts on an empty history (its own times).
            lock(&self.history).clear();
            r.link = LinkStatus::Disabled;
            r.replay = Some(ctl.clone());
            r.task = Some(Task::Thread(stop.clone()));
            session
        };
        let state = lock(&ctl).state();
        let host = self.clone();
        let started = std::thread::Builder::new()
            .name("starorg-telemetry-replay".into())
            .spawn(move || host.play(src, &ctl, &stop, &sink, session));
        if let Err(e) = started {
            self.stop();
            return Err(format!("cannot start the replay: {e}"));
        }
        Ok(state)
    }

    /// Pause, play, seek or change the speed of the replay.
    pub fn replay_control(
        &self,
        playing: Option<bool>,
        seek_ms: Option<u64>,
        speed: Option<f64>,
    ) -> Result<TelemetryReplayState, String> {
        let ctl = lock(&self.state)
            .replay
            .clone()
            .ok_or_else(|| "No replay is playing.".to_owned())?;
        let mut c = lock(&ctl);
        if let Some(s) = speed {
            c.speed = replay::clamp_speed(s);
        }
        if let Some(ms) = seek_ms {
            let ms = ms.min(c.duration_ms);
            c.seek = Some(ms);
            c.position_ms = ms;
            c.ended = false;
        }
        if let Some(p) = playing {
            if p && c.ended {
                // Play at the end starts over.
                c.seek = Some(0);
                c.position_ms = 0;
                c.ended = false;
            }
            c.playing = p;
        }
        Ok(c.state())
    }

    /// Fill the history with up to its length of samples before frame `upto` (after a seek,
    /// so the graphs show what led there). Reads at most one frame a history interval.
    fn prefill(&self, src: &mut dyn ReplaySource, upto: usize) {
        let mut h = lock(&self.history);
        h.clear();
        let span = starorg_telemetry::history::HISTORY_LEN as u64
            * starorg_telemetry::history::HISTORY_INTERVAL.as_millis() as u64;
        let t_end = src.t_ms(upto);
        let from = src.index_at(t_end.saturating_sub(span).saturating_sub(src.start_ms()));
        let step = starorg_telemetry::history::HISTORY_INTERVAL.as_millis() as u64;
        let mut next = 0u64;
        for i in from..upto {
            let t = src.t_ms(i);
            if t < next {
                continue;
            }
            if let Some(raw) = src.frame(i) {
                h.push(t, &Snapshot::from_json(&raw));
                next = t + step;
            }
        }
    }

    /// The replay's event (dropped when a newer session started; then false).
    fn emit_replay(
        &self,
        sink: &Sink,
        session: u64,
        frame: Option<Frame>,
        state: TelemetryReplayState,
    ) -> bool {
        if lock(&self.state).session != session {
            return false;
        }
        sink(TelemetryEvent {
            session,
            status: None,
            frame: frame.map(Box::new),
            replay: Some(state),
            alerts: Vec::new(),
            gamelog: None,
        });
        true
    }

    /// The player: frames at their recorded pace (gaps cut to [`replay::MAX_GAP_MS`]) divided
    /// by the speed; at most [`UI_INTERVAL`] apart to the UI, every frame into the history.
    fn play(
        &self,
        mut src: Box<dyn ReplaySource>,
        ctl: &Mutex<ReplayCtl>,
        stop: &AtomicBool,
        sink: &Sink,
        session: u64,
    ) {
        const TICK: Duration = Duration::from_millis(40);
        let n = src.len();
        let mut i = 0usize;
        let mut next_at = Instant::now();
        let mut last_emit: Option<Instant> = None;
        let mut seen = (true, 1.0f64);
        while !stop.load(Ordering::Relaxed) {
            let (playing, speed, seek) = {
                let mut c = lock(ctl);
                (c.playing, c.speed, c.seek.take())
            };
            if let Some(ms) = seek {
                i = src.index_at(ms);
                self.prefill(src.as_mut(), i);
                let f = src.ui_frame(i);
                let state = {
                    let mut c = lock(ctl);
                    c.position_ms = src.t_ms(i).saturating_sub(src.start_ms());
                    c.state()
                };
                if let Some(f) = &f {
                    lock(&self.history).push(f.received_ms, &f.snapshot);
                }
                if !self.emit_replay(sink, session, f, state) {
                    return;
                }
                last_emit = Some(Instant::now());
                seen = (playing, speed);
                i += 1;
                next_at = Instant::now() + Self::gap(src.as_ref(), i, speed);
                continue;
            }
            if (playing, speed) != seen {
                seen = (playing, speed);
                let state = lock(ctl).state();
                if !self.emit_replay(sink, session, None, state) {
                    return;
                }
                next_at = Instant::now();
            }
            if !playing {
                std::thread::sleep(TICK);
                continue;
            }
            if i >= n {
                let state = {
                    let mut c = lock(ctl);
                    c.playing = false;
                    c.ended = true;
                    c.position_ms = c.duration_ms;
                    c.state()
                };
                seen = (false, speed);
                if !self.emit_replay(sink, session, None, state) {
                    return;
                }
                continue;
            }
            let now = Instant::now();
            if now < next_at {
                std::thread::sleep((next_at - now).min(TICK));
                continue;
            }
            let f = src.ui_frame(i);
            let state = {
                let mut c = lock(ctl);
                c.position_ms = src.t_ms(i).saturating_sub(src.start_ms());
                c.state()
            };
            if let Some(f) = &f {
                lock(&self.history).push(f.received_ms, &f.snapshot);
            }
            let due = last_emit.is_none_or(|l| l.elapsed() >= UI_INTERVAL) || i + 1 == n;
            if due && f.is_some() {
                if !self.emit_replay(sink, session, f, state) {
                    return;
                }
                last_emit = Some(now);
            }
            i += 1;
            next_at = now + Self::gap(src.as_ref(), i, speed);
        }
    }

    /// How long to wait before frame `i` (0 for the first and past the end).
    fn gap(src: &dyn ReplaySource, i: usize, speed: f64) -> Duration {
        if i == 0 || i >= src.len() {
            return Duration::ZERO;
        }
        let ms = src
            .t_ms(i)
            .saturating_sub(src.t_ms(i - 1))
            .min(replay::MAX_GAP_MS);
        Duration::from_secs_f64(ms as f64 / 1000.0 / replay::clamp_speed(speed))
    }

    /// Follow `source` for the current stream (`None` stops the reader). `people`: keep the
    /// handles Game.log names. The same source and choice keep the running reader; a change
    /// starts over from the file's start (everything held is dropped). Without a stream
    /// (`telemetry_start` not called) nothing starts.
    pub fn set_gamelog(&self, source: Option<GameLogSource>, people: bool) {
        let mut r = lock(&self.state);
        if let (Some(g), Some(s)) = (&r.gamelog, &source)
            && g.source == *s
            && g.people == people
        {
            return;
        }
        if let Some(g) = r.gamelog.take() {
            g.stop();
        }
        let (Some(source), Some(sink)) = (source, r.sink.clone()) else {
            return;
        };
        if r.target.is_none() {
            return;
        }
        let shared = Arc::new(Mutex::new(GameLogShared {
            log: GameLog::new(people),
            source: match source {
                GameLogSource::Channel(_) => "game_log",
                GameLogSource::Mock => "mock",
            },
            found: false,
            rotations: 0,
            backfill_pending: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let session = r.session;
        let state = self.state.clone();
        let store = self.sessions.clone();
        let (sh, st, src) = (shared.clone(), stop.clone(), source.clone());
        let started = std::thread::Builder::new()
            .name("starorg-telemetry-gamelog".into())
            .spawn(move || {
                Self::run_gamelog(&state, &sh, &st, session, &sink, src, people, &store)
            });
        if started.is_ok() {
            r.gamelog = Some(GameLogTask {
                source,
                people,
                stop,
                shared,
            });
        }
    }

    /// The reader thread: poll, fold, send the view when it changed (at most every
    /// [`GAMELOG_EMIT`]); ends when stopped or when a newer stream started. Shard stints that
    /// end go to the join history, and one chunk of a `logbackups/` file is read in per poll
    /// until the newest backups are all in.
    #[allow(clippy::too_many_arguments)]
    fn run_gamelog(
        state: &Mutex<Running>,
        shared: &Mutex<GameLogShared>,
        stop: &AtomicBool,
        session: u64,
        sink: &Sink,
        source: GameLogSource,
        people: bool,
        store: &Mutex<Store>,
    ) {
        let now = || now_ms() as i64;
        let mut tail = match &source {
            GameLogSource::Channel(dir) => Some(gamelog::tail::Tail::for_channel(dir)),
            GameLogSource::Mock => None,
        };
        let mut backfill = match &source {
            GameLogSource::Channel(dir) => {
                Backfill::plan(stints::backups_dir(dir).as_deref(), &mut lock(store))
            }
            GameLogSource::Mock => Backfill::default(),
        };
        if tail.is_none() {
            let mut g = lock(shared);
            for l in gamelog::mock::session_lines(now(), people) {
                g.log.fold_line(&l);
            }
            g.log.end_poll(now());
            g.found = true;
        }
        let mut sent: Option<(u64, u64)> = None;
        let mut last_emit: Option<Instant> = None;
        let mut last_mock = Instant::now();
        let mut mock_i = 0u64;
        while !stop.load(Ordering::Relaxed) {
            let more = match &mut tail {
                Some(t) => {
                    // File IO first, without the lock; then fold under it.
                    let r = t.read();
                    let mut g = lock(shared);
                    gamelog::tail::apply(&r, &mut g.log, now());
                    g.found = t.found;
                    g.rotations = t.rotations;
                    r.more
                }
                None => {
                    let mut g = lock(shared);
                    if last_mock.elapsed() >= GAMELOG_MOCK_EVERY {
                        last_mock = Instant::now();
                        mock_i += 1;
                        let l = gamelog::mock::ambient_line(mock_i, now());
                        g.log.fold_line(&l);
                    }
                    g.log.end_poll(now());
                    false
                }
            };
            // Ended stints into the history; then one chunk of a backup (file IO unlocked).
            let ended = lock(shared).log.drain_stints();
            if !ended.is_empty() {
                lock(store).add(ended, None);
            }
            if !backfill.is_done() {
                if let Some((rows, mark)) = backfill.step() {
                    lock(store).add(rows, Some(mark));
                }
                lock(shared).backfill_pending = backfill.pending() as u32;
            }
            let rev = lock(store).revision;
            let due = last_emit.is_none_or(|t| t.elapsed() >= GAMELOG_EMIT);
            let view = {
                let g = lock(shared);
                let v = (g.log.version(), rev);
                (due && sent != Some(v)).then(|| (v, g.view(rev)))
            };
            if let Some((v, view)) = view {
                if stop.load(Ordering::Relaxed) || lock(state).session != session {
                    return;
                }
                sent = Some(v);
                last_emit = Some(Instant::now());
                sink(TelemetryEvent::gamelog(session, view));
            }
            // A big log is caught up in short steps; otherwise one poll every 500 ms.
            let wait = if more {
                Duration::from_millis(50)
            } else {
                gamelog::tail::POLL
            };
            let until = Instant::now() + wait;
            while Instant::now() < until && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    /// What the Game.log reader holds now (`None` while it does not run).
    pub fn gamelog_view(&self) -> Option<GameLogView> {
        let rev = lock(&self.sessions).revision;
        let r = lock(&self.state);
        r.gamelog.as_ref().map(|g| lock(&g.shared).view(rev))
    }

    /// The People panel's Clear: every person held goes (your own handle stays, so later
    /// lines naming you are still not listed).
    pub fn clear_people(&self) -> Option<GameLogView> {
        let rev = lock(&self.sessions).revision;
        let r = lock(&self.state);
        r.gamelog.as_ref().map(|g| {
            let mut s = lock(&g.shared);
            s.log.clear_people();
            s.view(rev)
        })
    }

    /// Every handle the reader holds (yours and others'): the diagnostics text replaces them.
    pub fn gamelog_handles(&self) -> Vec<String> {
        let r = lock(&self.state);
        r.gamelog
            .as_ref()
            .map(|g| lock(&g.shared).log.handles())
            .unwrap_or_default()
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

    /// A sink into a channel.
    fn channel() -> (Sink, std::sync::mpsc::Receiver<TelemetryEvent>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        (
            Arc::new(move |ev| {
                let _ = lock(&tx).send(ev);
            }),
            rx,
        )
    }

    /// Wait up to 5 s for `f`.
    fn until(what: &str, mut f: impl FnMut() -> bool) {
        let t0 = Instant::now();
        while !f() {
            assert!(t0.elapsed() < Duration::from_secs(5), "timed out: {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn fast_caps() -> Caps {
        Caps {
            interval: Duration::from_millis(50),
            ..Caps::default()
        }
    }

    #[test]
    fn frames_fill_the_history_kept_for_the_same_channel_only() {
        let host = TelemetryHost::default();
        host.start_mock(target("PTU"), quiet());
        until("a history sample", || {
            !host.history(None, None).t_ms.is_empty()
        });
        let h = host.history(None, None);
        assert!(h.reset && h.capacity == 600 && h.interval_ms == 1000);
        assert!(h.series.speed[0].is_some() && h.series.health[0].is_some());
        host.stop();
        // Same channel again: the history stays (its epoch too).
        host.start_mock(target("PTU"), quiet());
        assert!(!host.history(Some(h.epoch), None).reset);
        // Another channel: it starts over.
        host.start_mock(target("LIVE"), quiet());
        let again = host.history(Some(h.epoch), None);
        assert!(again.reset);
        host.stop();
    }

    #[test]
    fn mock_recordings_are_in_memory_and_off_by_default() {
        let host = TelemetryHost::default();
        host.set_caps(fast_caps());
        host.start_mock(target("PTU"), quiet());
        std::thread::sleep(Duration::from_millis(250));
        assert!(host.recordings().is_empty(), "off by default");
        assert_eq!(host.recording(), None);
        host.set_record(true);
        until("a recording", || host.recording().is_some());
        let name = host.recording().unwrap();
        assert!(name.ends_with("-PTU.jsonl"), "{name}");
        until("it grows", || host.recordings()[0].bytes > 2_000);
        assert!(host.delete_recording(&name).is_err(), "being written");
        // Deleting an older one leaves the growing one growing.
        let mut seeded = host.recordings();
        seeded.reverse();
        seeded.push(TelemetryRecording {
            name: "19700101-000000-PTU.jsonl".into(),
            ..seeded[0].clone()
        });
        seeded.rotate_right(1);
        host.seed_mock_recordings(Vec::new());
        {
            let mut rec = lock(&host.rec);
            rec.mock = seeded;
            rec.mock_current = Some((host.session(), name.clone()));
        }
        host.delete_recording("19700101-000000-PTU.jsonl").unwrap();
        let before = host.recordings()[0].bytes;
        until("still grows", || host.recordings()[0].bytes > before);
        assert_eq!(host.recording().as_deref(), Some(name.as_str()));
        host.stop();
        assert_eq!(host.recording(), None);
        assert_eq!(host.recordings().len(), 1);
        host.delete_recording(&name).unwrap();
        assert!(host.recordings().is_empty());
    }

    #[test]
    fn a_recorded_stream_replays_with_pause_seek_and_speed() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("recordings");
        let host = TelemetryHost::with_recordings(Some(dir.clone()));
        host.set_caps(fast_caps());
        host.set_record(true);
        host.start_mock(target("PTU"), quiet());
        until("frames written", || {
            host.recordings().first().and_then(|r| r.duration_ms) >= Some(600)
        });
        host.stop();
        let list = host.recordings();
        assert_eq!(list.len(), 1);
        let rec = &list[0];
        assert_eq!(rec.channel.as_deref(), Some("PTU"));
        assert_eq!(rec.schema.as_deref(), Some("mock"));
        assert!(dir.join(&rec.name).is_file());
        // No recording without the switch.
        host.set_record(false);
        host.start_mock(target("PTU"), quiet());
        std::thread::sleep(Duration::from_millis(250));
        host.stop();
        assert_eq!(host.recordings().len(), 1);

        assert!(host.start_replay("../x.jsonl", quiet()).is_err());
        assert!(host.start_replay("nope.jsonl", quiet()).is_err());
        let (sink, rx) = channel();
        let st = host.start_replay(&rec.name, sink).unwrap();
        assert!(st.playing && st.speed == 1.0 && st.position_ms == 0);
        assert!(host.target().is_none());
        assert_eq!(host.replay_state().unwrap().name, rec.name);
        let ev = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let f = ev.frame.expect("the first frame at once");
        assert_eq!(f.seq, 1);
        assert_eq!(ev.replay.unwrap().name, rec.name);
        assert!(host.delete_recording(&rec.name).is_err(), "playing");
        // Pause, then a seek shows that frame while paused.
        host.replay_control(Some(false), None, Some(100.0)).unwrap();
        until("paused", || {
            rx.try_iter()
                .any(|e| e.replay.is_some_and(|r| !r.playing) && e.frame.is_none())
        });
        let s = host.replay_control(None, Some(300), None).unwrap();
        assert_eq!(s.speed, replay::MAX_SPEED);
        let seek = loop {
            let e = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if e.frame.is_some() {
                break e;
            }
        };
        let r = seek.replay.unwrap();
        assert!(!r.playing && r.position_ms >= 300, "{r:?}");
        // Played to the end at top speed: ended, stopped.
        host.replay_control(Some(true), None, None).unwrap();
        until("ended", || host.replay_state().is_some_and(|r| r.ended));
        let end = host.replay_state().unwrap();
        assert!(!end.playing && end.position_ms == end.duration_ms);
        // Play at the end starts over.
        let again = host.replay_control(Some(true), None, None).unwrap();
        assert!(again.playing && again.position_ms == 0 && !again.ended);
        host.stop();
        assert!(host.replay_state().is_none());
        assert!(host.replay_control(Some(true), None, None).is_err());
        host.delete_recording(&rec.name).unwrap();
        assert!(host.recordings().is_empty());
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
        let got: Arc<Mutex<Vec<TelemetryEvent>>> = Arc::default();
        let sink: Sink = {
            let got = got.clone();
            Arc::new(move |e| got.lock().unwrap().push(e))
        };
        let ev = || LiveEvent::Frame(Box::new(boost(0.2, 1)));
        assert!(!host.forward(&sink, session + 1, ev()));
        assert!(host.forward(&sink, session, ev()));
        let got = got.lock().unwrap();
        assert_eq!(got.len(), 1, "the stale event is dropped");
        assert!(got[0].alerts.is_empty(), "the first frame is a baseline");
        drop(got);
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
