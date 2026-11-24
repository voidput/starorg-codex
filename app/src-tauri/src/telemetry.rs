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

use crate::contract::telemetry::{
    TelemetryActions, TelemetryEvent, TelemetryRecording, TelemetryRefusal, TelemetryReplayState,
    TelemetrySchemaInfo, TelemetryTriggerResult,
};
use crate::contract::types::ChannelRef;
use starorg_telemetry::client::{LoopbackAddr, TelemetryClient};
use starorg_telemetry::frames::{Frame, Snapshot, UI_INTERVAL};
use starorg_telemetry::history::{History, HistoryView};
use starorg_telemetry::live::{self, LinkStatus, LiveEvent, LiveOptions};
use starorg_telemetry::mock::{self, MockGame};
use starorg_telemetry::record::{self, Caps, Recorder, Stopped};
use starorg_telemetry::replay::{self, FileReplay, ReplaySource, SyntheticReplay};
use starorg_telemetry::schema::{Schema, SchemaOrigin};
use starorg_telemetry::trigger::{DANGEROUS, TriggerGate, TriggerableAction};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// The dangerous actions, sorted (they always need a confirmation).
pub fn dangerous() -> Vec<String> {
    let mut v: Vec<String> = DANGEROUS.iter().map(|s| (*s).to_owned()).collect();
    v.sort();
    v
}

impl TelemetryHost {
    /// Recordings in `dir` (the real backend: `<data>/telemetry/recordings`), or in memory.
    pub fn with_recordings(dir: Option<PathBuf>) -> Self {
        Self {
            state: Arc::default(),
            history: Arc::default(),
            rec: Arc::new(Mutex::new(RecSlot::new(dir))),
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

    /// Stop any stream or replay and start a new session for `target` (`None`: a replay);
    /// returns its number. The history goes unless it is of the same thing (`history_of`).
    fn begin(&self, r: &mut Running, target: Option<ChannelRef>, history_of: String) -> u64 {
        if let Some(t) = r.task.take() {
            t.stop();
        }
        lock(&self.rec).close(r.session);
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
    /// frame of a live stream also goes into the history and, with "Record sessions" on, the
    /// recording.
    fn forward(&self, sink: &Sink, session: u64, ev: LiveEvent) -> bool {
        let meta = {
            let mut r = lock(&self.state);
            if r.session != session {
                return false;
            }
            if let LiveEvent::Status(s) = &ev {
                r.link = s.clone();
            }
            let channel = r.target.as_ref().map(|t| t.channel.clone());
            let key = r.source.as_ref().map(|s| s.schema().key().to_owned());
            (channel, key)
        };
        if let LiveEvent::Frame(f) = &ev {
            lock(&self.history).push(f.received_ms, &f.snapshot);
            if let (Some(channel), Some(key)) = meta {
                self.record_frame(session, &channel, &key, f);
            }
        }
        sink(TelemetryEvent::of(session, ev));
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

    /// The graphs' history since the caller's last answer.
    pub fn history(&self, epoch: Option<u64>, since_ms: Option<u64>) -> HistoryView {
        lock(&self.history).since(epoch, since_ms)
    }

    /// No stream: the schema could not be had (`reason` shown on the screen).
    pub fn no_schema(&self, target: ChannelRef, reason: String, sink: &Sink) {
        let session = {
            let mut r = lock(&self.state);
            let key = Self::live_key(&target);
            self.begin(&mut r, Some(target), key)
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
        let session = self.begin(&mut r, Some(target), key);
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
            let session = self.begin(&mut r, Some(target), key);
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
            let session = self.begin(&mut r, None, format!("replay {name}"));
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
}
