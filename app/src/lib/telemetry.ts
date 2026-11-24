/**
 * Live telemetry screen logic (experimental; docs/plans/specs/live-telemetry.md), kept out of
 * the components so node can test it. The panels follow sc-telemetry's dashboard
 * (`sctelemetry/static/index.html`, same owner, MIT) in this app's own style: status, pilot
 * vitals, ship, shields, systems and annunciators, actions, raw frame.
 *
 * Frames come at most 10 a second. Everything here is a cheap pure function over one frame,
 * so a panel re-renders only the numbers that changed.
 */
import type { LinkStatus, TelemetryHistory, TelemetryJson, TelemetryRecording, TelemetrySeriesKey, TelemetrySnapshot, TelemetryShip, TriggerableAction } from './contract/telemetry';
import type { StatusTone } from './format';

type Obj = { [k: string]: TelemetryJson };

/** The value at a dotted path (`vehicle.state.lights_enabled`), or undefined. */
export function at(raw: TelemetryJson | undefined, path: string): TelemetryJson | undefined {
  let v: TelemetryJson | undefined = raw;
  for (const k of path.split('.')) {
    if (v === null || typeof v !== 'object' || Array.isArray(v)) return undefined;
    v = (v as Obj)[k];
  }
  return v;
}

/** server.py `_num`: a finite number, not the FLT_MAX "no body" sentinel, 4 decimals. */
function numAt(raw: TelemetryJson | undefined, path: string): number | null {
  const v = path ? at(raw, path) : raw;
  if (typeof v !== 'number' || !Number.isFinite(v) || Math.abs(v) > 1e30) return null;
  return Math.round(v * 10_000) / 10_000;
}

function textAt(raw: TelemetryJson | undefined, path: string): string | null {
  const v = at(raw, path);
  return typeof v === 'string' && v !== '' ? v : null;
}

function enumAt(raw: TelemetryJson | undefined, path: string, prefix: string): string | null {
  const s = textAt(raw, path);
  return s === null ? null : s.startsWith(prefix) ? s.slice(prefix.length) : s;
}

/** The snapshot of a raw frame: the same rules as the Rust `Snapshot::from_json` (tests compare them on the samples). */
export function snapshotFromRaw(raw: Obj): TelemetrySnapshot {
  const vitals = {
    health: numAt(raw, 'actor.vitals.health.current'),
    oxygen: numAt(raw, 'actor.vitals.oxygen.current'),
    heart_rate: numAt(raw, 'actor.vitals.heart_rate'),
    body_temperature: numAt(raw, 'actor.vitals.body_temperature.current'),
    gravity: numAt(raw, 'actor.vitals.gravity'),
  };
  const vs = at(raw, 'vehicle.state');
  if (vs === null || vs === undefined || typeof vs !== 'object' || Array.isArray(vs)) return { vitals, ship: null };
  const faces = at(vs, 'shields.faces');
  const lights = at(vs, 'lights_enabled');
  const ship: TelemetryShip = {
    name: textAt(vs, 'name'),
    class_name: textAt(vs, 'class_name'),
    speed: numAt(vs, 'flight.instruments.speed'),
    hydrogen_fuel: numAt(vs, 'flight.instruments.hydrogen_fuel'),
    quantum_fuel: numAt(vs, 'flight.instruments.quantum_fuel'),
    hull: numAt(vs, 'health'),
    shield: numAt(vs, 'shields.level'),
    shield_faces: Array.isArray(faces) ? faces.map((x) => numAt(x, '')).filter((x): x is number => x !== null) : [],
    g: numAt(vs, 'flight.instruments.g_meter.current'),
    boost: numAt(vs, 'flight.instruments.boost.reserves'),
    vsi: numAt(vs, 'flight.instruments.vertical_speed_indicator'),
    master_mode: enumAt(vs, 'master_mode.mode', 'VEHICLE_MASTER_MODE_'),
    gear: enumAt(vs, 'flight.state.gear', 'VEHICLE_COMPONENT_DEPLOYMENT_STATE_'),
    lights: typeof lights === 'boolean' ? lights : null,
  };
  return { vitals, ship };
}

/** The connection as a badge: tone, short label and the sentence under it. */
export function linkView(l: LinkStatus, streaming: boolean): { tone: StatusTone; label: string; detail: string } {
  switch (l.state) {
    case 'connected':
      return { tone: 'success', label: 'Connected', detail: 'Reading the game. Frames start once you have spawned.' };
    case 'connecting':
      return { tone: 'info', label: 'Connecting', detail: 'Looking for the game on this computer…' };
    case 'game_not_running':
      return { tone: 'muted', label: 'Game not running', detail: 'Nothing answers on the port yet. Start the game with the launcher set up as in Settings; this retries by itself.' };
    case 'unreachable':
      return { tone: 'warning', label: 'Unreachable', detail: `${l.reason}. Retrying.` };
    case 'no_schema':
      return { tone: 'destructive', label: 'No schema', detail: l.reason };
    default:
      return streaming ? { tone: 'muted', label: 'Stopped', detail: '' } : { tone: 'muted', label: 'Stopped', detail: 'Not reading the game.' };
  }
}

/** `0.8234` as `82%`; null as an em dash. */
export function pct(v: number | null | undefined): string {
  return v === null || v === undefined ? '—' : `${Math.round(v * 100)}%`;
}

/** A number with fixed decimals and an optional unit; null as an em dash. */
export function fixed(v: number | null | undefined, digits = 0, unit = ''): string {
  if (v === null || v === undefined) return '—';
  const s = v.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return unit ? `${s} ${unit}` : s;
}

/** 0..1 to a bar width, clamped. */
export function barWidth(v: number | null | undefined): string {
  if (v === null || v === undefined) return '0%';
  return `${Math.max(0, Math.min(100, v * 100)).toFixed(1)}%`;
}

/** Shield face labels by count (sc-telemetry `shield_faces.json`: 1 is a bubble; the order of more is not verified, so F1..Fn). */
export function faceLabels(n: number): string[] {
  if (n === 1) return ['ALL'];
  return Array.from({ length: n }, (_, i) => `F${i + 1}`);
}

export interface Indicator {
  id: string;
  label: string;
  /** null = the build does not send it. */
  on: boolean | null;
  /** Shown instead of ON/OFF (gear: UP, DOWN, DEPLOYING). */
  text?: string;
}

const GEAR: Record<string, string> = { RETRACTED: 'UP', DEPLOYING: 'DEPLOYING', DEPLOYED: 'DOWN', RETRACTING: 'RETRACTING' };

const flag = (raw: Obj, path: string): boolean | null => {
  const v = at(raw, path);
  return typeof v === 'boolean' ? v : null;
};

/** The Systems panel: switches the frame reports, in a fixed order. */
export function systems(raw: Obj): Indicator[] {
  const gear = enumAt(raw, 'vehicle.state.flight.state.gear', 'VEHICLE_COMPONENT_DEPLOYMENT_STATE_');
  return [
    { id: 'engines', label: 'Engines', on: flag(raw, 'vehicle.state.power.engines') },
    { id: 'shield-gen', label: 'Shield generators', on: flag(raw, 'vehicle.state.power.shield_generators') },
    { id: 'lights', label: 'Lights', on: flag(raw, 'vehicle.state.lights_enabled') },
    { id: 'gear', label: 'Landing gear', on: gear === null ? null : gear === 'DEPLOYED' || gear === 'DEPLOYING', ...(gear ? { text: GEAR[gear] ?? gear } : {}) },
    { id: 'landed', label: 'Landed', on: flag(raw, 'vehicle.state.flight.state.is_landed') },
    { id: 'spooled', label: 'Quantum spooled', on: flag(raw, 'vehicle.state.master_mode.spooled') },
    { id: 'boost', label: 'Boost engaged', on: flag(raw, 'vehicle.state.flight.instruments.boost.engaged') },
  ];
}

export interface Annunciator {
  id: string;
  label: string;
  tone: 'warning' | 'destructive';
}

/** Warnings worth a glance, from the snapshot (thresholds as sc-telemetry's default alerts, simplified). */
export function annunciators(s: TelemetrySnapshot, raw: Obj): Annunciator[] {
  const out: Annunciator[] = [];
  const v = s.vitals;
  if (v.health !== null && v.health < 30) out.push({ id: 'health', label: 'Health low', tone: 'destructive' });
  if (v.oxygen !== null && v.oxygen < 0.2) out.push({ id: 'oxygen', label: 'Oxygen low', tone: 'destructive' });
  const sh = s.ship;
  if (sh) {
    if (sh.hull !== null && sh.hull < 0.3) out.push({ id: 'hull', label: 'Hull critical', tone: 'destructive' });
    if (sh.hydrogen_fuel !== null && sh.hydrogen_fuel < 0.15) out.push({ id: 'fuel', label: 'Hydrogen low', tone: 'warning' });
    if (sh.quantum_fuel !== null && sh.quantum_fuel < 0.15) out.push({ id: 'qfuel', label: 'Quantum fuel low', tone: 'warning' });
    if (flag(raw, 'vehicle.state.power.shield_generators') === false) out.push({ id: 'shields-off', label: 'Shields off', tone: 'warning' });
    else if (sh.shield !== null && sh.shield < 0.1) out.push({ id: 'shields-down', label: 'Shields down', tone: 'warning' });
    if (sh.g !== null && Math.abs(sh.g) > 6) out.push({ id: 'g', label: 'High G', tone: 'warning' });
  }
  return out;
}

/** Actions by category (sorted), for the Actions panel. */
export function groupActions(actions: readonly TriggerableAction[]): { category: string; actions: TriggerableAction[] }[] {
  const by = new Map<string, TriggerableAction[]>();
  for (const a of actions) by.set(a.category, [...(by.get(a.category) ?? []), a]);
  return [...by.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([category, list]) => ({ category, actions: [...list].sort((x, y) => x.action_name.localeCompare(y.action_name)) }));
}

/** `spaceship_power` as `Spaceship power`; `v_power_toggle_shields` as `Power toggle shields`. */
export function humanName(id: string): string {
  const s = id.replace(/^v_/, '').replace(/_/g, ' ').trim();
  return s ? s[0]!.toUpperCase() + s.slice(1) : id;
}

/**
 * The state an action changes, read from the frame (a few of sc-telemetry's
 * `action_state_map.json` groups whose link to the frame was seen in game: lights, gear, shields).
 */
export function actionState(raw: Obj, action: string): string | null {
  if (action.startsWith('v_lights')) {
    const v = flag(raw, 'vehicle.state.lights_enabled');
    return v === null ? null : v ? 'ON' : 'OFF';
  }
  if (action.endsWith('_landing_system')) {
    const g = enumAt(raw, 'vehicle.state.flight.state.gear', 'VEHICLE_COMPONENT_DEPLOYMENT_STATE_');
    return g === null ? null : (GEAR[g] ?? g);
  }
  if (action.startsWith('v_power_') && action.includes('shields')) {
    const v = flag(raw, 'vehicle.state.power.shield_generators');
    return v === null ? null : v ? 'ON' : 'OFF';
  }
  return null;
}

/** Whether a press must ask first: every dangerous action, and the first press of a session. */
export function needsConfirm(action: string, dangerous: readonly string[], confirmedThisSession: boolean): boolean {
  return dangerous.includes(action) || !confirmedThisSession;
}

/** A trigger answer in words. */
export function triggerText(r: { outcome: { status: string; message: string } | null; refused: { message: string } | null; error: string | null }): { ok: boolean; text: string } {
  if (r.refused) return { ok: false, text: `Not sent: ${r.refused.message}.` };
  if (r.error) return { ok: false, text: r.error };
  if (!r.outcome) return { ok: false, text: 'No answer.' };
  const ok = r.outcome.status === 'OK';
  const why: Record<string, string> = {
    UNKNOWN_ACTION: 'the game does not know this action',
    MAP_INACTIVE: 'not available where you are now',
    RATE_LIMITED: 'pressed too soon; try again in a moment',
    HOLD_NOT_ALLOWED: 'this action cannot be held',
    FILTERED: 'the game filtered it out',
    FAILED: 'the game could not do it',
  };
  return { ok, text: ok ? 'Done.' : `${why[r.outcome.status] ?? r.outcome.status}${r.outcome.message ? ` (${r.outcome.message})` : ''}.` };
}

/** The raw frame as indented JSON for the inspector, cut after `maxLines` lines. */
export function inspectorText(raw: Obj, maxLines = 400): { text: string; cut: boolean } {
  const lines = JSON.stringify(raw, null, 2).split('\n');
  return lines.length <= maxLines ? { text: lines.join('\n'), cut: false } : { text: lines.slice(0, maxLines).join('\n'), cut: true };
}

/** At most one call per `ms` (the inspector follows the stream at 1 Hz, not 10). */
export function gate(ms: number, now: () => number = Date.now): () => boolean {
  let last = -Infinity;
  return () => {
    const t = now();
    if (t - last < ms) return false;
    last = t;
    return true;
  };
}

/**
 * What an event of stream `event` means for a store on stream `current`: `stale` (an older
 * stream, dropped), `same`, or `new` (a newer stream: the old stream's frame and count go
 * before anything of the new one is shown).
 */
export function sessionStep(current: number, event: number): 'stale' | 'same' | 'new' {
  return event < current ? 'stale' : event > current ? 'new' : 'same';
}

/**
 * The channel live telemetry is for: the one picked, else the streaming one, else the one
 * whose USER.cfg has the telemetry line (only one channel carries it), else the app's
 * channel, else the first offered. `offered` holds offered channels only.
 */
export function telemetryTarget<C extends { install: string; channel: string; telemetry_line: { state: string } }>(
  offered: readonly C[],
  picked: string | null,
  streaming: { install: string; channel: string } | null,
  appKey: string | null,
): C | null {
  const key = (c: { install: string; channel: string }) => `${c.install}/${c.channel}`;
  const byKey = (k: string | null) => (k ? (offered.find((c) => key(c) === k) ?? null) : null);
  return byKey(picked) ?? (streaming ? byKey(key(streaming)) : null) ?? offered.find((c) => c.telemetry_line.state === 'set') ?? byKey(appKey) ?? offered[0] ?? null;
}

/** Numbers async answers so only the latest one is used (`const n = next(); ...; if (latest(n))`). */
export function sequencer(): { next: () => number; latest: (n: number) => boolean } {
  let seq = 0;
  return { next: () => ++seq, latest: (n) => n === seq };
}

// ---- History graphs (sc-telemetry `/api/history` sparklines) --------------------------------

/** One graph: its series, label, how its values read and its fixed range (null: from the data). */
export interface SeriesSpec {
  key: TelemetrySeriesKey;
  label: string;
  /** Fixed range, else fitted to the visible values. */
  min: number | null;
  max: number | null;
  /** Below this the line is drawn as a warning (fractions and health only). */
  low: number | null;
  text: (v: number | null) => string;
}

const health = (v: number | null) => (v === null ? '—' : `${Math.round(v)}%`);

/** The graphs, in screen order: flight, then ship, then pilot. */
export const SERIES: readonly SeriesSpec[] = [
  { key: 'speed', label: 'Speed', min: 0, max: null, low: null, text: (v) => fixed(v, 0, 'm/s') },
  { key: 'g', label: 'G', min: null, max: null, low: null, text: (v) => fixed(v, 2) },
  { key: 'hydrogen_fuel', label: 'Hydrogen fuel', min: 0, max: 1, low: 0.15, text: pct },
  { key: 'quantum_fuel', label: 'Quantum fuel', min: 0, max: 1, low: 0.15, text: pct },
  { key: 'hull', label: 'Hull', min: 0, max: 1, low: 0.3, text: pct },
  { key: 'shield', label: 'Shields', min: 0, max: 1, low: 0.1, text: pct },
  { key: 'health', label: 'Health', min: 0, max: 100, low: 30, text: health },
  { key: 'oxygen', label: 'Oxygen', min: 0, max: 1, low: 0.2, text: pct },
];

/** The windows a graph can show. */
export const WINDOWS = [
  { id: '60', label: '1 min', ms: 60_000 },
  { id: '300', label: '5 min', ms: 300_000 },
  { id: '600', label: '10 min', ms: 600_000 },
] as const;

const SERIES_KEYS: readonly TelemetrySeriesKey[] = ['speed', 'g', 'hydrogen_fuel', 'quantum_fuel', 'hull', 'shield', 'health', 'oxygen'];

/**
 * The client's history after an answer of `telemetry_history`: the answer itself on a reset
 * (or with nothing before), else the old samples and the new ones, the oldest cut so at most
 * `capacity` stay. Never mutates `prev`.
 */
export function mergeHistory(prev: TelemetryHistory | null, next: TelemetryHistory): TelemetryHistory {
  if (!prev || next.reset || next.epoch !== prev.epoch) return next;
  if (next.t_ms.length === 0) return prev;
  const last = prev.t_ms.at(-1) ?? -Infinity;
  // Only what is newer than the newest kept (an answer may overlap a slow earlier one).
  const from = next.t_ms.findIndex((t) => t > last);
  if (from === -1) return prev;
  const cap = Math.max(1, next.capacity);
  const join = <X>(a: X[], b: X[]) => [...a, ...b.slice(from)].slice(-cap);
  const series = {} as TelemetryHistory['series'];
  for (const k of SERIES_KEYS) series[k] = join(prev.series[k], next.series[k]);
  return { ...next, reset: false, t_ms: join(prev.t_ms, next.t_ms), series };
}

/** The range a graph spans: the spec's fixed ends, else the visible values with a little room (never empty). */
export function seriesRange(values: readonly (number | null)[], spec: Pick<SeriesSpec, 'min' | 'max'>): { min: number; max: number } {
  let lo = Infinity;
  let hi = -Infinity;
  for (const v of values) {
    if (v === null) continue;
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  if (lo === Infinity) {
    lo = spec.min ?? 0;
    hi = spec.max ?? 1;
  }
  let min = spec.min ?? lo;
  let max = spec.max ?? hi;
  if (spec.min === null || spec.max === null) {
    const pad = (max - min) * 0.08 || Math.max(Math.abs(max), 1) * 0.1;
    if (spec.min === null) min -= pad;
    if (spec.max === null) max += pad;
  }
  if (max <= min) max = min + 1;
  return { min, max };
}

/**
 * An SVG path for one series over `[end - span, end]` in a `w` x `h` box (y up): `M`/`L`
 * runs, broken where a value is missing or two samples are more than `gapMs` apart.
 * Samples before the window are left out. Coordinates rounded to 0.1.
 */
export function sparkPath(t: readonly number[], v: readonly (number | null)[], o: { w: number; h: number; end: number; span: number; min: number; max: number; gapMs: number }): string {
  const start = o.end - o.span;
  const r = (x: number) => Math.round(x * 10) / 10;
  const sy = o.h / (o.max - o.min);
  const sx = o.w / o.span;
  let d = '';
  let prevT = -Infinity;
  let open = false;
  for (let i = 0; i < t.length; i++) {
    const ti = t[i]!;
    const vi = v[i];
    if (ti < start || vi === null || vi === undefined) {
      open = false;
      prevT = ti;
      continue;
    }
    const x = r((ti - start) * sx);
    const y = r(o.h - (Math.min(o.max, Math.max(o.min, vi)) - o.min) * sy);
    d += `${open && ti - prevT <= o.gapMs ? 'L' : 'M'}${x} ${y}`;
    open = true;
    prevT = ti;
  }
  return d;
}

/** The newest value of a series that is not missing. */
export function latestValue(v: readonly (number | null)[]): number | null {
  for (let i = v.length - 1; i >= 0; i--) if (v[i] !== null && v[i] !== undefined) return v[i]!;
  return null;
}

/** Lowest and highest value inside the window (null without any). */
export function windowStats(t: readonly number[], v: readonly (number | null)[], start: number): { min: number; max: number } | null {
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < t.length; i++) {
    const x = v[i];
    if (t[i]! < start || x === null || x === undefined) continue;
    min = Math.min(min, x);
    max = Math.max(max, x);
  }
  return min === Infinity ? null : { min, max };
}

// ---- Recordings and replay ---------------------------------------------------------------

/** `95_000` as `1:35`, `3_725_000` as `1:02:05`. */
export function clockText(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) return '—';
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** A recording's line in the list: when (local time), channel and length. */
export function recordingText(r: TelemetryRecording, locale?: string): string {
  const when = r.started_ms === null ? r.name : new Date(r.started_ms).toLocaleString(locale, { dateStyle: 'medium', timeStyle: 'short' });
  return [when, r.channel, r.duration_ms === null ? null : clockText(r.duration_ms)].filter(Boolean).join(' · ');
}

/** Replay speeds offered. */
export const SPEEDS = [0.25, 0.5, 1, 2, 4, 8, 16] as const;

export function speedText(s: number): string {
  return `${s}×`;
}
