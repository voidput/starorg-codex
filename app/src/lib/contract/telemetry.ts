/**
 * Live telemetry (experimental): types mirrored from app/src-tauri/src/contract/telemetry.rs
 * (and the starorg-telemetry shapes it re-exports) plus the client calls. Re-exported by
 * types.ts. The app reads Star Citizen's own local service on 127.0.0.1 only, and only while
 * the user has turned it on (Settings > Experimental); it is offered for 5.0+ builds only.
 */
import { call } from './client';
import type { ChannelRef } from './types';

/** Event name of `TelemetryEvent`. */
export const TELEMETRY_EVENT = 'telemetry';

export interface TelemetrySettings {
  enabled: boolean;
  /** GRPC_CLIENT_SERVER_PORT, 1024-65535. */
  port: number;
  /** The "Metrics + actions" choice; counts only with `enabled` on. */
  triggers: boolean;
  /** "Record sessions" (its own switch, `telemetry_record`); counts only with `enabled` on. */
  record: boolean;
  /** "Read Game.log events" (its own switch, off by default); counts only with `enabled` on. */
  gamelog: boolean;
  /** "People": keep the handles Game.log names, memory only; counts only with `gamelog` on. */
  people: boolean;
  /** "Shard history": keep the shards joined in the data folder, read logbackups in; counts only with `gamelog` on. */
  sessions: boolean;
}

export type CvarState = 'no-file' | 'missing' | 'set' | 'other';

export interface TelemetryCfgLine {
  line: string;
  state: CvarState;
  value: string | null;
}

export interface TelemetryChannel {
  install: string;
  channel: string;
  game_version: string | null;
  offered: boolean;
  /** Why not offered. */
  reason: string | null;
  user_cfg: string;
  telemetry_line: TelemetryCfgLine;
  triggers_line: TelemetryCfgLine;
  exe_found: boolean;
}

export interface TelemetrySchemaInfo {
  key: string;
  /** cache | extracted | synthetic */
  origin: string;
  files: number;
}

export type HelpOs = 'windows' | 'linux';

export type LinkState = 'disabled' | 'no_schema' | 'connecting' | 'connected' | 'game_not_running' | 'unreachable';

export type LinkStatus =
  | { state: 'disabled' }
  | { state: 'no_schema'; reason: string }
  | { state: 'connecting' }
  | { state: 'connected' }
  | { state: 'game_not_running' }
  | { state: 'unreachable'; reason: string };

export interface TelemetryStatus {
  settings: TelemetrySettings;
  channels: TelemetryChannel[];
  help_os: HelpOs;
  env_help: string;
  /** Always 127.0.0.1:<port>. */
  endpoint: string;
  link: LinkStatus;
  streaming: ChannelRef | null;
  session: number;
  schema: TelemetrySchemaInfo | null;
  ui_hz: number;
  dangerous: string[];
  /** Frames from the synthetic generator (mock backend). */
  mock: boolean;
  /** A recording plays instead of the game (the "Replay" mode). */
  replay: TelemetryReplayState | null;
  /** The recording being written now. */
  recording: string | null;
  /** Why the last recording could not start or stopped early. */
  record_error: string | null;
}

export interface TelemetryReplayState {
  /** The recording's file name. */
  name: string;
  /** Unix ms of its first frame. */
  start_ms: number;
  position_ms: number;
  duration_ms: number;
  playing: boolean;
  /** 0.25-16. */
  speed: number;
  /** At the last frame; Play starts over. */
  ended: boolean;
}

/** The graphs' series, one value a second (null: not sent then). Health 0-100, oxygen and the ship's fractions 0-1, speed m/s. */
export interface TelemetrySeries {
  speed: (number | null)[];
  g: (number | null)[];
  hydrogen_fuel: (number | null)[];
  quantum_fuel: (number | null)[];
  hull: (number | null)[];
  shield: (number | null)[];
  health: (number | null)[];
  oxygen: (number | null)[];
}

export type TelemetrySeriesKey = keyof TelemetrySeries;

export interface TelemetryHistory {
  epoch: number;
  /** The caller's points are gone: this is the whole ring. */
  reset: boolean;
  interval_ms: number;
  capacity: number;
  /** Unix ms per sample (a replay: when it was recorded). */
  t_ms: number[];
  series: TelemetrySeries;
}

export interface TelemetryHistoryRequest {
  epoch: number | null;
  since_ms: number | null;
}

export interface TelemetryRecording {
  /** File name; the webview names a recording by this, never by a path. */
  name: string;
  bytes: number;
  started_ms: number | null;
  duration_ms: number | null;
  channel: string | null;
  schema: string | null;
}

export interface TelemetryRecordings {
  /** Newest first. */
  items: TelemetryRecording[];
  folder: string;
  /** Being written (not deletable until the stream stops). */
  recording: string | null;
  replaying: string | null;
  max_files: number;
  max_total_bytes: number;
  max_file_bytes: number;
  record_hz: number;
}

export interface TelemetryRecordingRef {
  name: string;
}

/** Each field left null stays as it is. */
export interface TelemetryReplayControl {
  playing: boolean | null;
  seek_ms: number | null;
  speed: number | null;
}

export interface TelemetryConfigureRequest {
  target: ChannelRef | null;
  settings: TelemetrySettings;
  replace_other: boolean;
}

export type LineOutcome = 'already-set' | 'written' | 'needs-decision' | 'not-ours' | 'still-used' | 'reverted' | 'changed-since';

export interface TelemetryLineChange {
  line: string;
  outcome: LineOutcome;
  value: string | null;
}

export interface TelemetryConfigureResult {
  status: TelemetryStatus;
  telemetry_line: TelemetryLineChange | null;
  triggers_line: TelemetryLineChange | null;
  warnings: string[];
}

export interface TriggerableAction {
  action_name: string;
  category: string;
  holdable: boolean;
}

export interface TelemetryActions {
  actions: TriggerableAction[];
  triggers_active: boolean;
  dangerous: string[];
  /** game | mock | none */
  source: string;
  error: string | null;
}

export interface TelemetryTriggerRequest {
  action: string;
  /** 1-10000, holdable actions only. */
  hold_ms: number | null;
  confirmed: boolean;
}

export type RefusalReason = 'disabled' | 'invalid_name' | 'invalid_hold' | 'not_listed' | 'hold_not_allowed' | 'needs_confirm' | 'debounced';

export interface TelemetryRefusal {
  reason: RefusalReason;
  message: string;
}

export interface TriggerOutcome {
  /** OK, UNKNOWN_ACTION, MAP_INACTIVE, RATE_LIMITED, FAILED, HOLD_NOT_ALLOWED, FILTERED, ... */
  status: string;
  message: string;
}

export interface TelemetryTriggerResult {
  action: string;
  outcome: TriggerOutcome | null;
  refused: TelemetryRefusal | null;
  error: string | null;
}

export interface TelemetryVitals {
  /** 0-100. */
  health: number | null;
  /** 0-1. */
  oxygen: number | null;
  heart_rate: number | null;
  /** °C. */
  body_temperature: number | null;
  gravity: number | null;
}

export interface TelemetryShip {
  name: string | null;
  class_name: string | null;
  /** m/s. */
  speed: number | null;
  /** 0-1 from here on, unless named. */
  hydrogen_fuel: number | null;
  quantum_fuel: number | null;
  hull: number | null;
  shield: number | null;
  shield_faces: number[];
  g: number | null;
  boost: number | null;
  /** Vertical speed, m/s. */
  vsi: number | null;
  master_mode: string | null;
  gear: string | null;
  lights: boolean | null;
}

export interface TelemetrySnapshot {
  vitals: TelemetryVitals;
  ship: TelemetryShip | null;
}

/** Any JSON value (the raw frame). */
export type TelemetryJson = string | number | boolean | null | TelemetryJson[] | { [k: string]: TelemetryJson };

export interface TelemetryFrame {
  seq: number;
  received_ms: number;
  snapshot: TelemetrySnapshot;
  /** The whole frame, proto field names, enums by name, int64 as strings. */
  raw: { [k: string]: TelemetryJson };
}

/** Mirrors `starorg_telemetry::gamelog::EventKind`. */
export type GameLogEventKind =
  | 'log_started'
  | 'session_join'
  | 'session_shard_change'
  | 'session_leave'
  | 'session_system'
  | 'session_queue'
  | 'session_hub_down'
  | 'session_hub_up'
  | 'session_disconnect'
  | 'session_dropped'
  | 'nearby_ship'
  | 'qt_route'
  | 'qt_arrived'
  | 'zone'
  | 'vehicle_released'
  | 'vehicle_destroyed'
  | 'corpse_items'
  | 'spawned'
  | 'respawned';

/** One line of the Events panel; never carries an entity id, an address or a handle. */
export interface GameLogEvent {
  seq: number;
  kind: GameLogEventKind;
  /** Log time, Unix ms. */
  t_ms: number | null;
  detail: string;
}

export interface GameLogSession {
  channel: string | null;
  build: string | null;
  shard: string | null;
  region_code: string | null;
  region_name: string | null;
  shard_short: string | null;
  system: string | null;
  joined_ms: number | null;
  log_started: string | null;
  queue_position: number | null;
  queue_eta_s: number | null;
  /** up | down */
  hub: string | null;
  disconnects: number;
  last_disconnect_reason: string | null;
  last_disconnect_expected: boolean | null;
}

export interface GameLogZone {
  /** armistice | monitored | ungoverned | jurisdiction | other */
  kind: string;
  text: string;
  since_ms: number | null;
}

export interface GameLogNearby {
  class: string;
  display: string;
  /** Ship instances seen this shard, not people. */
  count: number;
  first_ms: number | null;
  last_ms: number | null;
}

export interface GameLogQt {
  /** idle | routed */
  state: string;
  destination: string | null;
  fuel_estimate: number | null;
  since_ms: number | null;
  last_arrival: string | null;
}

export interface GameLogSignals {
  zone: GameLogZone | null;
  armistice: boolean;
  jurisdiction: string | null;
  location: string | null;
  qt: GameLogQt;
  own_ship: { class: string; display: string } | null;
  nearby: GameLogNearby[];
  nearby_count: number;
  nearby_classes: number;
  deaths: number;
  respawns: number;
  vehicles_destroyed: number;
}

export type PersonRelation = 'crew' | 'party' | 'party_likely' | 'nearby';

export interface GameLogPerson {
  name: string;
  relation: PersonRelation;
  relations: PersonRelation[];
  /** crew | party | party? | nearby */
  label: string;
  sources: string[];
  first_ms: number | null;
  last_ms: number | null;
  count: number;
  ship: string | null;
  left: boolean;
  leader: boolean;
  valid_handle: boolean;
}

export interface GameLogPeople {
  counts: { crew: number; party: number; party_likely: number; nearby: number; total: number };
  people: GameLogPerson[];
  self_name: string | null;
}

export interface GameLogView {
  /** game_log | mock */
  source: string;
  found: boolean;
  rotations: number;
  last_line_ms: number | null;
  session: GameLogSession;
  signals: GameLogSignals;
  /** Oldest first, at most 200. */
  events: GameLogEvent[];
  /** null while People is off (no handle is kept then). */
  people: GameLogPeople | null;
  /** The shard stint you are on now; null off a shard. */
  stint: TelemetryStint | null;
  /** The join history's revision: when it moves, read `telemetry_sessions` again. */
  sessions_rev: number;
}

/** Mirrors `starorg_telemetry::gamelog::LeftReason`. */
export type LeftReason = 'leave' | 'shard_change' | 'disconnect' | 'dropped' | 'log_end';

/** One shard stint (join to leave). Never an address, a port, a session id or a handle. */
export interface TelemetryStint {
  channel: string | null;
  build: string | null;
  shard: string;
  shard_short: string | null;
  region_code: string | null;
  region: string | null;
  log_started: string | null;
  joined_ms: number;
  /** null while still on the shard. */
  left_ms: number | null;
  left_reason: LeftReason | null;
  uptime_s: number | null;
  systems: string[];
  disconnects: number;
  dropped: boolean;
}

/** The server and shard join history, newest first. */
export interface TelemetrySessions {
  items: TelemetryStint[];
  max_items: number;
  /** The history file (null on the mock: memory only). */
  path: string | null;
  revision: number;
  /** logbackups files still to read in while the reader runs. */
  backfill_pending: number;
  error: string | null;
}

export interface TelemetryEvent {
  session: number;
  status: LinkStatus | null;
  frame: TelemetryFrame | null;
  /** In a replay: its position (its own events may carry only this). */
  replay: TelemetryReplayState | null;
  /** Alerts this frame fired (none while Notifications is off). */
  alerts: TelemetryAlert[];
  /** The Game.log reader's view, when it changed (at most twice a second). */
  gamelog: GameLogView | null;
}

/** Their own choice, off by default; Metrics never turns them on. */
export type TelemetryNotifications = 'off' | 'in_app' | 'in_app_sound';

export type AlertSeverity = 'info' | 'warn' | 'critical';

export type AlertScope = 'any' | 'vehicle' | 'pilot' | 'on_foot';

export interface TelemetryAlert {
  rule: string;
  severity: AlertSeverity;
  text: string;
  /** Set only with "In app + sound" and speech on. */
  speech: string | null;
  /** Tone id (info | warn | critical), set only with "In app + sound". */
  sound: string | null;
  value: number | null;
  at_ms: number;
  test: boolean;
}

/** The user's change to a default rule; null keeps the default. */
export interface TelemetryRuleOverride {
  enabled: boolean | null;
  cooldown_s: number | null;
  value: number | null;
  rearm: number | null;
}

export interface TelemetryAlertSettings {
  notifications: TelemetryNotifications;
  /** Spoken callouts (this computer's own voices only); counts only with in_app_sound. */
  speech: boolean;
  overrides: { [rule: string]: TelemetryRuleOverride };
}

export interface TelemetryAlertRule {
  id: string;
  description: string;
  /** threshold | transition | edge | composite */
  kind: string;
  severity: AlertSeverity;
  scope: AlertScope;
  path: string | null;
  op: string | null;
  /** fraction (0-1, shown as %) | count | value */
  unit: string | null;
  message: string;
  sound: string | null;
  verified: boolean;
  debounce_s: number;
  enabled: boolean;
  cooldown_s: number;
  threshold: number | null;
  rearm: number | null;
  default_enabled: boolean;
  default_cooldown_s: number;
  default_threshold: number | null;
  default_rearm: number | null;
}

export interface TelemetryAlerts {
  settings: TelemetryAlertSettings;
  rules: TelemetryAlertRule[];
  /** Newest first. */
  recent: TelemetryAlert[];
  recent_max: number;
  /** A stream runs and notifications are on. */
  evaluating: boolean;
  /** Saved overrides that no longer fit (skipped). */
  warnings: string[];
}

export interface TelemetryAlertTestRequest {
  rule: string;
}

export interface TelemetryCommandMap {
  telemetry_status: [Record<string, never>, TelemetryStatus];
  telemetry_configure: [{ req: TelemetryConfigureRequest }, TelemetryConfigureResult];
  telemetry_start: [{ req: ChannelRef }, TelemetryStatus];
  telemetry_stop: [Record<string, never>, TelemetryStatus];
  telemetry_actions: [Record<string, never>, TelemetryActions];
  telemetry_trigger: [{ req: TelemetryTriggerRequest }, TelemetryTriggerResult];
  telemetry_history: [{ req: TelemetryHistoryRequest }, TelemetryHistory];
  telemetry_record: [{ req: { on: boolean } }, TelemetryStatus];
  telemetry_recordings: [Record<string, never>, TelemetryRecordings];
  telemetry_recording_delete: [{ req: TelemetryRecordingRef }, TelemetryRecordings];
  telemetry_replay_start: [{ req: TelemetryRecordingRef }, TelemetryStatus];
  telemetry_replay_control: [{ req: TelemetryReplayControl }, TelemetryStatus];
  telemetry_alerts: [Record<string, never>, TelemetryAlerts];
  telemetry_alerts_configure: [{ req: TelemetryAlertSettings }, TelemetryAlerts];
  telemetry_alert_test: [{ req: TelemetryAlertTestRequest }, TelemetryAlert];
  telemetry_gamelog: [Record<string, never>, GameLogView | null];
  telemetry_people_clear: [Record<string, never>, GameLogView | null];
  telemetry_sessions: [Record<string, never>, TelemetrySessions];
  telemetry_sessions_clear: [Record<string, never>, TelemetrySessions];
}

/** Same order as `TELEMETRY_COMMAND_NAMES` in contract/telemetry.rs (a Rust test compares them). */
export const TELEMETRY_COMMANDS = [
  'telemetry_status',
  'telemetry_configure',
  'telemetry_start',
  'telemetry_stop',
  'telemetry_actions',
  'telemetry_trigger',
  'telemetry_history',
  'telemetry_record',
  'telemetry_recordings',
  'telemetry_recording_delete',
  'telemetry_replay_start',
  'telemetry_replay_control',
  'telemetry_alerts',
  'telemetry_alerts_configure',
  'telemetry_alert_test',
  'telemetry_gamelog',
  'telemetry_people_clear',
  'telemetry_sessions',
  'telemetry_sessions_clear',
] as const satisfies readonly (keyof TelemetryCommandMap)[];

const none = {} as Record<string, never>;

export const telemetryApi = {
  status: () => call('telemetry_status', none),
  /** Saves the switches and changes the target's USER.cfg; off releases every line and stops the stream. */
  configure: (req: TelemetryConfigureRequest) => call('telemetry_configure', { req }),
  /** Streams one channel; events arrive through `onTelemetry`. */
  start: (req: ChannelRef) => call('telemetry_start', { req }),
  stop: () => call('telemetry_stop', none),
  actions: () => call('telemetry_actions', none),
  trigger: (req: TelemetryTriggerRequest) => call('telemetry_trigger', { req }),
  /** The graphs' history since the last answer (`epoch`/`since_ms` of it; null for all). */
  history: (req: TelemetryHistoryRequest) => call('telemetry_history', { req }),
  /** "Record sessions" on or off (saved; off ends a recording in progress). */
  record: (on: boolean) => call('telemetry_record', { req: { on } }),
  recordings: () => call('telemetry_recordings', none),
  deleteRecording: (name: string) => call('telemetry_recording_delete', { req: { name } }),
  /** Plays a recording into the screen in place of the stream; events arrive through `onTelemetry`. */
  replayStart: (name: string) => call('telemetry_replay_start', { req: { name } }),
  replayControl: (req: TelemetryReplayControl) => call('telemetry_replay_control', { req }),
  alerts: () => call('telemetry_alerts', none),
  /** Notifications, speech and every rule override (a misfit refuses the whole request). */
  configureAlerts: (req: TelemetryAlertSettings) => call('telemetry_alerts_configure', { req }),
  testAlert: (req: TelemetryAlertTestRequest) => call('telemetry_alert_test', { req }),
  /** The Game.log view now (null while the reader does not run). */
  gamelog: () => call('telemetry_gamelog', none),
  /** Forget every person the People panel lists. */
  clearPeople: () => call('telemetry_people_clear', none),
  /** The server and shard join history, newest first. */
  sessions: () => call('telemetry_sessions', none),
  clearSessions: () => call('telemetry_sessions_clear', none),
};
