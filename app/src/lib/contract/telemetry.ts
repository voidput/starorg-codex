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

export interface TelemetryEvent {
  session: number;
  status: LinkStatus | null;
  frame: TelemetryFrame | null;
}

export interface TelemetryCommandMap {
  telemetry_status: [Record<string, never>, TelemetryStatus];
  telemetry_configure: [{ req: TelemetryConfigureRequest }, TelemetryConfigureResult];
  telemetry_start: [{ req: ChannelRef }, TelemetryStatus];
  telemetry_stop: [Record<string, never>, TelemetryStatus];
  telemetry_actions: [Record<string, never>, TelemetryActions];
  telemetry_trigger: [{ req: TelemetryTriggerRequest }, TelemetryTriggerResult];
}

/** Same order as `TELEMETRY_COMMAND_NAMES` in contract/telemetry.rs (a Rust test compares them). */
export const TELEMETRY_COMMANDS = [
  'telemetry_status',
  'telemetry_configure',
  'telemetry_start',
  'telemetry_stop',
  'telemetry_actions',
  'telemetry_trigger',
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
};
