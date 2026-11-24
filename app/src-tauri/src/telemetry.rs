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

use crate::contract::telemetry::{
    TelemetryActions, TelemetryEvent, TelemetryRefusal, TelemetrySchemaInfo, TelemetryTriggerResult,
};
use crate::contract::types::ChannelRef;
use starorg_telemetry::client::{LoopbackAddr, TelemetryClient};
use starorg_telemetry::frames::{Frame, Snapshot, UI_INTERVAL};
use starorg_telemetry::live::{self, LinkStatus, LiveEvent, LiveOptions};
use starorg_telemetry::mock::{self, MockGame};
use starorg_telemetry::schema::{Schema, SchemaOrigin};
use starorg_telemetry::trigger::{DANGEROUS, TriggerGate, TriggerableAction};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

    /// Stop any stream and start a new session for `target`; returns its number.
    fn begin(&self, r: &mut Running, target: ChannelRef) -> u64 {
        if let Some(t) = r.task.take() {
            t.stop();
        }
        r.session += 1;
        r.target = Some(target);
        r.source = None;
        r.actions_error = None;
        r.session
    }

    /// Record a status of `session` (ignored when a newer stream started); false when stale.
    fn note(state: &Mutex<Running>, session: u64, ev: &LiveEvent) -> bool {
        let mut r = lock(state);
        if r.session != session {
            return false;
        }
        if let LiveEvent::Status(s) = ev {
            r.link = s.clone();
        }
        true
    }

    fn forward(state: &Mutex<Running>, sink: &Sink, session: u64, ev: LiveEvent) -> bool {
        if !Self::note(state, session, &ev) {
            return false;
        }
        sink(TelemetryEvent::of(session, ev));
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
        let on = r.gate.enabled();
        r.gate.set_enabled(false);
        r.gate.set_enabled(on);
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
