//! Live telemetry in contract form (experimental; docs/plans/specs/live-telemetry.md): the
//! game's own local gRPC service, read on `127.0.0.1` while the user has turned it on.
//! Self-contained like [`super::assets`]: types, the [`TelemetryBackend`] half of the backend
//! and the commands live here; `real/telemetry.rs` and `mock_telemetry.rs` implement it over
//! [`crate::telemetry::TelemetryHost`]. Mirrored in `app/src/lib/contract/telemetry.ts`.
//!
//! - Off by default (`[experimental] telemetry` in `config.toml`), and offered only for a
//!   channel whose build is 5.0 or newer (`starorg_core::telemetry::eligible`).
//! - `telemetry_configure` saves the switches and brings the channel's `USER.cfg` in line
//!   (the telemetry line, and the triggers line only with "Allow game actions" on); switching
//!   off releases the lines everywhere and stops the stream.
//! - `telemetry_start` streams one channel and sends [`TELEMETRY_EVENT`]s (status changes, and
//!   frames at most [`starorg_telemetry::frames::UI_HZ`] a second); `telemetry_stop` ends it.
//! - `telemetry_trigger` presses an action the game lists as triggerable, refused unless
//!   "Allow game actions" is on; dangerous ones need `confirmed`, and the UI asks before the
//!   first press of each session.
//! - `telemetry_history` answers the 10-minute ring of headline values (1 a second) for the
//!   graphs, only what is new since the caller's last answer.
//! - "Record sessions" (`telemetry_record`, its own switch, off) writes the live stream to
//!   JSONL in the app's data folder, capped (`starorg_telemetry::record::Caps`);
//!   `telemetry_recordings` lists them, `telemetry_recording_delete` removes one, and
//!   `telemetry_replay_start` / `telemetry_replay_control` play one into the same screen
//!   (events carry [`TelemetryReplayState`]); `telemetry_stop` ends a replay too.

use super::types::*;
use super::{BackendState, run};
use serde::{Deserialize, Serialize};
use starorg_telemetry::frames::Frame;
pub use starorg_telemetry::history::{HistoryView as TelemetryHistory, Series as TelemetrySeries};
use starorg_telemetry::live::{LinkStatus, LiveEvent};
pub use starorg_telemetry::record::RecordingInfo as TelemetryRecording;
use starorg_telemetry::trigger::{Refusal, TriggerOutcome, TriggerableAction};
use tauri::Emitter;

pub use crate::telemetry::Sink;
pub use starorg_core::telemetry::HelpOs;

/// Event name of [`TelemetryEvent`].
pub const TELEMETRY_EVENT: &str = "telemetry";

/// `[experimental]` telemetry settings (`starorg_core::telemetry::Settings`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySettings {
    /// Read the game's live stream.
    pub enabled: bool,
    /// The port given to the game (`GRPC_CLIENT_SERVER_PORT`), 1024 to 65535.
    pub port: u16,
    /// "Allow game actions": press allowlisted inputs. Counts only with `enabled` on.
    pub triggers: bool,
    /// "Record sessions": write the live stream to a recording. Counts only with `enabled`
    /// on; independent of `triggers`. Changed alone by `telemetry_record`.
    #[serde(default)]
    pub record: bool,
}

impl From<starorg_core::telemetry::Settings> for TelemetrySettings {
    fn from(s: starorg_core::telemetry::Settings) -> Self {
        Self {
            enabled: s.enabled,
            port: s.port,
            triggers: s.triggers,
            record: s.record,
        }
    }
}

impl From<TelemetrySettings> for starorg_core::telemetry::Settings {
    fn from(s: TelemetrySettings) -> Self {
        Self {
            enabled: s.enabled,
            port: s.port,
            triggers: s.triggers,
            record: s.record,
        }
    }
}

impl TelemetrySettings {
    pub fn triggers_active(&self) -> bool {
        self.enabled && self.triggers
    }

    pub fn record_active(&self) -> bool {
        self.enabled && self.record
    }
}

/// A managed `USER.cfg` line as found in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CvarState {
    /// No `USER.cfg` yet (one is created when needed).
    NoFile,
    Missing,
    /// Set to `1` (ours or the user's own).
    Set,
    /// Set to another value (`0`): replaced only after asking.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryCfgLine {
    /// The exact line the app writes.
    pub line: String,
    pub state: CvarState,
    /// The other value, for `other`.
    pub value: Option<String>,
}

impl TelemetryCfgLine {
    pub fn of(text: Option<&str>, m: &starorg_core::usercfg::ManagedLine) -> Self {
        use starorg_core::usercfg::CvarLine;
        let (state, value) = match starorg_core::usercfg::inspect_line(text, m) {
            CvarLine::NoFile => (CvarState::NoFile, None),
            CvarLine::Missing => (CvarState::Missing, None),
            CvarLine::Set { .. } => (CvarState::Set, None),
            CvarLine::Other { value, .. } => (CvarState::Other, Some(value)),
        };
        Self {
            line: m.line.into(),
            state,
            value,
        }
    }
}

/// One channel as Settings shows it: offered or why not, and its `USER.cfg` lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryChannel {
    pub install: String,
    pub channel: String,
    pub game_version: Option<String>,
    /// The build is 5.0 or newer.
    pub offered: bool,
    /// Why not, when not offered.
    pub reason: Option<String>,
    /// `USER.cfg` (shown; the webview never sends a path).
    pub user_cfg: String,
    pub telemetry_line: TelemetryCfgLine,
    pub triggers_line: TelemetryCfgLine,
    /// `Bin64/StarCitizen.exe` is there to read the schema from.
    pub exe_found: bool,
}

/// The schema the stream uses (read from the user's `StarCitizen.exe`; never shipped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetrySchemaInfo {
    /// `<Branch>-<change>` (the cache file name).
    pub key: String,
    /// `cache`, `extracted` (scanned just now) or `synthetic` (the mock).
    pub origin: String,
    pub files: u32,
}

/// Settings -> Experimental -> Live telemetry, and the screen's header.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryStatus {
    pub settings: TelemetrySettings,
    /// Every channel of every install, offered or not.
    pub channels: Vec<TelemetryChannel>,
    pub help_os: HelpOs,
    /// How to give the RSI Launcher the two variables (`starorg_core::telemetry::env_help`).
    pub env_help: String,
    /// Where the game's service is read: always `127.0.0.1:<port>`.
    pub endpoint: String,
    /// `{"state": "disabled" | "no_schema" | "connecting" | "connected" | "game_not_running" |
    /// "unreachable", "reason"?}`.
    pub link: LinkStatus,
    /// The channel the stream runs for.
    pub streaming: Option<ChannelRef>,
    /// Number of the current stream; events of another session are stale.
    pub session: u64,
    pub schema: Option<TelemetrySchemaInfo>,
    /// Frames a second at most.
    pub ui_hz: u32,
    /// Actions that always ask first (self destruct, eject, power off...).
    pub dangerous: Vec<String>,
    /// The mock backend: frames come from the synthetic generator.
    pub mock: bool,
    /// A recording plays instead of the game (the screen's "Replay" mode).
    pub replay: Option<TelemetryReplayState>,
    /// The recording being written now ("Record sessions" on and frames arriving).
    pub recording: Option<String>,
    /// Why the last recording could not be started or stopped early, if it did.
    pub record_error: Option<String>,
}

/// A replay as the screen shows it (in status and on every replayed event).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryReplayState {
    /// The recording's file name.
    pub name: String,
    /// Unix ms of its first frame.
    pub start_ms: u64,
    /// From the first frame to the one shown.
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
    /// 0.25 to 16.
    pub speed: f64,
    /// It reached the last frame (Play starts over).
    pub ended: bool,
}

/// `telemetry_history`: what the caller already has (both `None` for everything).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TelemetryHistoryRequest {
    /// The epoch of the caller's last answer; another one means start over.
    pub epoch: Option<u64>,
    /// The time of the caller's newest sample.
    pub since_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRecordRequest {
    pub on: bool,
}

/// One recording, named by its file name (the webview never sends a path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRecordingRef {
    pub name: String,
}

/// Recordings, newest first, and their bounds.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryRecordings {
    pub items: Vec<TelemetryRecording>,
    /// The folder (shown only).
    pub folder: String,
    /// Being written now (not deletable until the stream stops).
    pub recording: Option<String>,
    /// Playing now.
    pub replaying: Option<String>,
    pub max_files: u32,
    pub max_total_bytes: u64,
    pub max_file_bytes: u64,
    /// Frames a second written at most.
    pub record_hz: u32,
}

/// `telemetry_replay_control`: each field left out stays as it is.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TelemetryReplayControl {
    pub playing: Option<bool>,
    /// Jump to this far from the first frame.
    pub seek_ms: Option<u64>,
    /// 0.25 to 16 (clamped).
    pub speed: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryConfigureRequest {
    /// The channel whose `USER.cfg` gets the lines (needed to switch on; `None` when off).
    pub target: Option<ChannelRef>,
    pub settings: TelemetrySettings,
    /// Replace an explicit other value (`= 0`) after the user agreed; undo restores it.
    pub replace_other: bool,
}

/// What happened to one `USER.cfg` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineOutcome {
    AlreadySet,
    Written,
    /// Set to another value; nothing written (resend with `replace_other`).
    NeedsDecision,
    NotOurs,
    /// Other channels still rely on it; the file was not touched.
    StillUsed,
    Reverted,
    /// Changed by someone since we wrote it; left alone.
    ChangedSince,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryLineChange {
    pub line: String,
    pub outcome: LineOutcome,
    /// The other value, for `needs-decision`.
    pub value: Option<String>,
}

impl TelemetryLineChange {
    pub fn of(line: &str, c: &starorg_core::telemetry::LineChange) -> Self {
        use starorg_core::telemetry::LineChange as L;
        use starorg_core::usercfg::{AdoptOutcome as A, UndoOutcome as U};
        let (outcome, value) = match c {
            L::Adopted {
                outcome: A::AlreadySet,
            } => (LineOutcome::AlreadySet, None),
            L::Adopted {
                outcome: A::Written,
            } => (LineOutcome::Written, None),
            L::Adopted {
                outcome: A::NeedsDecision { value },
            } => (LineOutcome::NeedsDecision, Some(value.clone())),
            L::Released {
                outcome: U::NotOurs,
            } => (LineOutcome::NotOurs, None),
            L::Released {
                outcome: U::StillUsed { .. },
            } => (LineOutcome::StillUsed, None),
            L::Released {
                outcome: U::Reverted,
            } => (LineOutcome::Reverted, None),
            L::Released {
                outcome: U::ChangedSince,
            } => (LineOutcome::ChangedSince, None),
        };
        Self {
            line: line.into(),
            outcome,
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryConfigureResult {
    pub status: TelemetryStatus,
    /// The target channel's lines (none without a target).
    pub telemetry_line: Option<TelemetryLineChange>,
    pub triggers_line: Option<TelemetryLineChange>,
    /// Channels whose `USER.cfg` could not be changed while switching off, with the reason.
    pub warnings: Vec<String>,
}

/// `ListTriggerableActions` (the last non-empty answer while triggers are on).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryActions {
    /// By category, then name.
    pub actions: Vec<TriggerableAction>,
    pub triggers_active: bool,
    pub dangerous: Vec<String>,
    /// `game`, `mock`, or `none` while no stream runs.
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryTriggerRequest {
    pub action: String,
    /// Hold for this long (holdable actions only), 1 to 10000 ms.
    pub hold_ms: Option<u32>,
    /// The user confirmed (dangerous actions; the UI also asks before a session's first press).
    pub confirmed: bool,
}

/// Why the app refused a press before it reached the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRefusal {
    /// `disabled`, `invalid_name`, `invalid_hold`, `not_listed`, `hold_not_allowed`,
    /// `needs_confirm`, `debounced`.
    pub reason: String,
    pub message: String,
}

impl TelemetryRefusal {
    pub fn of(r: &Refusal) -> Self {
        let reason = serde_json::to_value(r)
            .ok()
            .and_then(|v| v["reason"].as_str().map(str::to_owned))
            .unwrap_or_default();
        Self {
            reason,
            message: r.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryTriggerResult {
    pub action: String,
    /// The game's answer (`OK`, `UNKNOWN_ACTION`, `RATE_LIMITED`, ...).
    pub outcome: Option<TriggerOutcome>,
    /// Refused by the app; nothing was sent.
    pub refused: Option<TelemetryRefusal>,
    /// The call failed (game gone, timeout).
    pub error: Option<String>,
}

/// Payload of [`TELEMETRY_EVENT`]: a status change or a frame of stream `session`, and in a
/// replay its position (a replay's own events may carry only that).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryEvent {
    pub session: u64,
    pub status: Option<LinkStatus>,
    pub frame: Option<Box<Frame>>,
    pub replay: Option<TelemetryReplayState>,
}

impl TelemetryEvent {
    pub fn of(session: u64, ev: LiveEvent) -> Self {
        match ev {
            LiveEvent::Status(s) => Self {
                session,
                status: Some(s),
                frame: None,
                replay: None,
            },
            LiveEvent::Frame(f) => Self {
                session,
                status: None,
                frame: Some(f),
                replay: None,
            },
        }
    }
}

/// `ok` when the build of `version` is offered, else the reason.
pub fn offer(version: Option<starorg_core::build::GameVersion>) -> Result<(), String> {
    starorg_core::telemetry::eligible_version(version).map_err(|e| e.to_string())
}

/// The live telemetry half of the backend.
pub trait TelemetryBackend {
    /// Switches, channels, help text and the stream's state (reads `USER.cfg` files only).
    fn telemetry_status(&self) -> CmdResult<TelemetryStatus>;
    /// Save the switches; with telemetry on, the target must be offered and gets the
    /// `USER.cfg` lines. Off: every line is released and the stream stops.
    fn telemetry_configure(
        &self,
        req: TelemetryConfigureRequest,
    ) -> CmdResult<TelemetryConfigureResult>;
    /// Stream one channel (replacing a running stream). Refused while off or not offered. A
    /// schema that cannot be read is a `no_schema` status, not an error.
    fn telemetry_start(&self, req: ChannelRef, sink: Sink) -> CmdResult<TelemetryStatus>;
    fn telemetry_stop(&self) -> CmdResult<TelemetryStatus>;
    fn telemetry_actions(&self) -> CmdResult<TelemetryActions>;
    fn telemetry_trigger(&self, req: TelemetryTriggerRequest) -> CmdResult<TelemetryTriggerResult>;
    /// The graphs' history since the caller's last answer.
    fn telemetry_history(&self, req: TelemetryHistoryRequest) -> CmdResult<TelemetryHistory>;
    /// "Record sessions" on or off (saved; a recording in progress ends when off).
    fn telemetry_record(&self, req: TelemetryRecordRequest) -> CmdResult<TelemetryStatus>;
    fn telemetry_recordings(&self) -> CmdResult<TelemetryRecordings>;
    /// Refused (`busy`) for the recording being written or played.
    fn telemetry_recording_delete(
        &self,
        req: TelemetryRecordingRef,
    ) -> CmdResult<TelemetryRecordings>;
    /// Play a recording into the screen (replacing a running stream or replay).
    fn telemetry_replay_start(
        &self,
        req: TelemetryRecordingRef,
        sink: Sink,
    ) -> CmdResult<TelemetryStatus>;
    fn telemetry_replay_control(&self, req: TelemetryReplayControl) -> CmdResult<TelemetryStatus>;
}

macro_rules! telemetry_commands {
    ($( $name:ident ( $($arg:ident : $ty:ty),* ) -> $ret:ty ;)*) => {
        $(
            #[tauri::command(rename_all = "snake_case")]
            pub async fn $name(
                state: tauri::State<'_, BackendState>,
                $($arg: $ty),*
            ) -> CmdResult<$ret> {
                run(&state, move |b| b.$name($($arg),*)).await
            }
        )*
    };
}

telemetry_commands! {
    telemetry_status() -> TelemetryStatus;
    telemetry_configure(req: TelemetryConfigureRequest) -> TelemetryConfigureResult;
    telemetry_stop() -> TelemetryStatus;
    telemetry_actions() -> TelemetryActions;
    telemetry_trigger(req: TelemetryTriggerRequest) -> TelemetryTriggerResult;
    telemetry_history(req: TelemetryHistoryRequest) -> TelemetryHistory;
    telemetry_record(req: TelemetryRecordRequest) -> TelemetryStatus;
    telemetry_recordings() -> TelemetryRecordings;
    telemetry_recording_delete(req: TelemetryRecordingRef) -> TelemetryRecordings;
    telemetry_replay_control(req: TelemetryReplayControl) -> TelemetryStatus;
}

/// The stream's events go to the window as [`TELEMETRY_EVENT`].
#[tauri::command(rename_all = "snake_case")]
pub async fn telemetry_start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, BackendState>,
    req: ChannelRef,
) -> CmdResult<TelemetryStatus> {
    run(&state, move |b| {
        let sink: Sink = std::sync::Arc::new(move |ev: TelemetryEvent| {
            let _ = app.emit(TELEMETRY_EVENT, ev);
        });
        b.telemetry_start(req, sink)
    })
    .await
}

/// A replay's events go to the window as [`TELEMETRY_EVENT`], like a stream's.
#[tauri::command(rename_all = "snake_case")]
pub async fn telemetry_replay_start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, BackendState>,
    req: TelemetryRecordingRef,
) -> CmdResult<TelemetryStatus> {
    run(&state, move |b| {
        let sink: Sink = std::sync::Arc::new(move |ev: TelemetryEvent| {
            let _ = app.emit(TELEMETRY_EVENT, ev);
        });
        b.telemetry_replay_start(req, sink)
    })
    .await
}

/// Command names, in handler order (the TS client's `TELEMETRY_COMMANDS` matches).
pub const TELEMETRY_COMMAND_NAMES: &[&str] = &[
    "telemetry_status",
    "telemetry_configure",
    "telemetry_start",
    "telemetry_stop",
    "telemetry_actions",
    "telemetry_trigger",
    "telemetry_history",
    "telemetry_record",
    "telemetry_recordings",
    "telemetry_recording_delete",
    "telemetry_replay_start",
    "telemetry_replay_control",
];
