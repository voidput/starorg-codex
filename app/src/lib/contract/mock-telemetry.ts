/**
 * Live telemetry in the in-browser mock: the same behaviour as the Rust mock
 * (app/src-tauri/src/contract/mock_telemetry.rs) seeded from its samples. The stream is a
 * synthetic generator in the shape of the Rust one (`starorg_telemetry::mock::frame`): 10
 * frames a second, values on slow sine waves, lights, gear and shields flipped by presses,
 * and threshold alerts (mockAlertEngine) once Notifications is on.
 * No game, no socket, no CIG data. No JSON import here, so node can load it.
 */
import type * as T from './types';
import type * as L from './telemetry';
import { snapshotFromRaw } from '../telemetry.ts';

export interface TelemetryMockData {
  status: L.TelemetryStatus;
  actions: L.TelemetryActions;
  /** A frame event of the Rust mock (its raw frame is the template). */
  frame: L.TelemetryEvent;
  /** The Rust mock's alert view with notifications off (the rules and their defaults). */
  alerts: L.TelemetryAlerts;
}

export interface TelemetryMockCtx {
  fail(code: T.ErrorCode, message: string): never;
  log(message: string, channel: string | null): void;
  /** `setInterval` / `clearInterval`, swappable in tests. */
  every?(ms: number, f: () => void): () => void;
}

type H<A, R> = (args: A) => R | Promise<R>;
export interface TelemetryHandlers {
  telemetry_status: H<Record<string, never>, L.TelemetryStatus>;
  telemetry_configure: H<{ req: L.TelemetryConfigureRequest }, L.TelemetryConfigureResult>;
  telemetry_start: H<{ req: T.ChannelRef }, L.TelemetryStatus>;
  telemetry_stop: H<Record<string, never>, L.TelemetryStatus>;
  telemetry_actions: H<Record<string, never>, L.TelemetryActions>;
  telemetry_trigger: H<{ req: L.TelemetryTriggerRequest }, L.TelemetryTriggerResult>;
  telemetry_alerts: H<Record<string, never>, L.TelemetryAlerts>;
  telemetry_alerts_configure: H<{ req: L.TelemetryAlertSettings }, L.TelemetryAlerts>;
  telemetry_alert_test: H<{ req: L.TelemetryAlertTestRequest }, L.TelemetryAlert>;
}

const clone = <X>(x: X): X => JSON.parse(JSON.stringify(x)) as X;

/** `{pct}`, `{pct_words}` (as digits here), `{value}` and `{value:.Nf}` (the Rust engine's `render`). */
export function renderAlert(template: string, value: number | null): string {
  const pct = value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100);
  return template.replace(/\{(value|pct|pct_words)(?::\.(\d)f)?\}/g, (_, name: string, d: string | undefined) => {
    const n = name === 'value' ? value : pct;
    if (n === null) return '?';
    return d ? n.toFixed(Number(d)) : String(n);
  });
}

/**
 * The mock's alerts: threshold rules on plain frame paths only (the Rust engine does the rest),
 * with the same baseline, hysteresis and cooldown. Enough for the browser preview.
 */
export function mockAlertEngine() {
  const st = new Map<string, { armed: boolean; last: number }>();
  let t0: number | null = null;
  return {
    reset(): void {
      st.clear();
      t0 = null;
    },
    evaluate(rules: readonly L.TelemetryAlertRule[], raw: Obj, t: number, atMs: number): L.TelemetryAlert[] {
      t0 ??= t;
      const settling = t - t0 < 1.5;
      const out: L.TelemetryAlert[] = [];
      for (const r of rules) {
        if (!r.enabled || r.kind !== 'threshold' || !r.path || r.path.startsWith('$') || r.threshold === null) continue;
        const v = r.path.split('.').reduce<L.TelemetryJson | undefined>((o, k) => (o && typeof o === 'object' && !Array.isArray(o) ? o[k] : undefined), raw);
        if (typeof v !== 'number') continue;
        const below = r.op === '<' || r.op === '<=';
        const on = r.op === '<' ? v < r.threshold : r.op === '<=' ? v <= r.threshold : r.op === '>' ? v > r.threshold : v >= r.threshold;
        const s = st.get(r.id) ?? { armed: false, last: -Infinity };
        st.set(r.id, s);
        if (settling) {
          s.armed = !on;
          continue;
        }
        if (!s.armed) {
          const past = r.rearm === null ? !on : below ? v >= r.rearm : v <= r.rearm;
          if (past && !on) s.armed = true;
          continue;
        }
        if (!on || t - s.last < r.cooldown_s) continue;
        s.armed = false;
        s.last = t;
        out.push({ rule: r.id, severity: r.severity, text: renderAlert(r.message, v), speech: null, sound: r.sound, value: v, at_ms: atMs, test: false });
      }
      return out;
    },
  };
}

/** What the user asked for only: no tone without sound, no speech without both. */
function shape(a: L.TelemetryAlert, s: L.TelemetryAlertSettings): L.TelemetryAlert {
  if (s.notifications !== 'in_app_sound') return { ...a, sound: null, speech: null };
  return s.speech ? a : { ...a, speech: null };
}

interface MockState {
  lights: boolean;
  gear: boolean;
  shields: boolean;
}

type Obj = { [k: string]: L.TelemetryJson };

function put(raw: Obj, path: string, v: L.TelemetryJson): void {
  const ks = path.split('.');
  let o = raw;
  for (const k of ks.slice(0, -1)) {
    const next = o[k];
    if (next === null || typeof next !== 'object' || Array.isArray(next)) o[k] = {};
    o = o[k] as Obj;
  }
  o[ks.at(-1)!] = v;
}

const f32 = (x: number) => Math.round(x * 10_000) / 10_000;

/** One synthetic frame at `t` seconds, from the template's raw frame. */
export function mockFrame(template: Obj, seq: number, t: number, st: MockState, now = Date.now()): L.TelemetryFrame {
  const raw = clone(template);
  const s = Math.sin;
  const g = f32(1 + 0.5 * s(t / 2));
  put(raw, 'actor.vitals.health.current', f32(80 + 20 * s(t / 9)));
  put(raw, 'actor.vitals.oxygen.current', f32(0.7 + 0.3 * s(t / 13)));
  put(raw, 'actor.vitals.body_temperature.current', f32(37 + 0.6 * s(t / 7)));
  put(raw, 'actor.vitals.heart_rate', f32(78 + 25 * (0.5 + 0.5 * s(t / 5))));
  put(raw, 'actor.vitals.gravity', g);
  const vs = 'vehicle.state';
  put(raw, `${vs}.health`, f32(0.75 + 0.25 * s(t / 17)));
  put(raw, `${vs}.lights_enabled`, st.lights);
  put(raw, `${vs}.shields.level`, f32(0.5 + 0.5 * s(t / 6)));
  put(raw, `${vs}.shields.faces`, [0, 1, 2, 3].map((k) => f32(Math.min(1, Math.max(0, 0.6 + 0.4 * s(t / 3 + k))))));
  put(raw, `${vs}.master_mode.mode`, Math.floor(t / 15) % 2 === 1 ? 'VEHICLE_MASTER_MODE_SCM' : 'VEHICLE_MASTER_MODE_NAVIGATION');
  put(raw, `${vs}.power.shield_generators`, st.shields);
  put(raw, `${vs}.flight.state.gear`, st.gear ? 'VEHICLE_COMPONENT_DEPLOYMENT_STATE_DEPLOYED' : 'VEHICLE_COMPONENT_DEPLOYMENT_STATE_RETRACTED');
  const ins = `${vs}.flight.instruments`;
  put(raw, `${ins}.speed`, f32(180 + 120 * s(t / 4)));
  put(raw, `${ins}.hydrogen_fuel`, f32(Math.max(0, 1 - (t % 300) / 300)));
  put(raw, `${ins}.g_meter.current`, g);
  put(raw, `${ins}.boost.engaged`, s(t / 4) > 0.6);
  put(raw, `${ins}.boost.output`, f32(Math.max(0, s(t / 4))));
  put(raw, `${ins}.boost.reserves`, f32(0.5 + 0.5 * s(t / 8)));
  put(raw, `${ins}.altitude`, f32(12_000 + 3_000 * s(t / 30)));
  put(raw, `${ins}.vertical_speed_indicator`, f32(50 * s(t / 6)));
  put(raw, `${ins}.heading`, f32((t * 12) % 360));
  return { seq, received_ms: now, snapshot: snapshotFromRaw(raw), raw };
}

const TOGGLES: [keyof MockState, string, string, string][] = [
  ['lights', 'v_lights', 'v_lights_on', 'v_lights_off'],
  ['gear', 'v_toggle_landing_system', 'v_deploy_landing_system', 'v_retract_landing_system'],
  ['shields', 'v_power_toggle_shields', 'v_power_set_shields_on', 'v_power_set_shields_off'],
];

export function createTelemetryMock(data: TelemetryMockData, ctx: TelemetryMockCtx) {
  const every =
    ctx.every ??
    ((ms: number, f: () => void) => {
      const id = setInterval(f, ms);
      return () => clearInterval(id);
    });
  const listeners = new Set<(e: L.TelemetryEvent) => void>();
  const template = (data.frame.frame?.raw ?? {}) as Obj;
  const base = data.status;
  let settings: L.TelemetrySettings = clone(base.settings);
  /** `install/CHANNEL` -> lines held. */
  const lines = new Map<string, { telemetry: boolean; triggers: boolean }>();
  let session = 0;
  let streaming: T.ChannelRef | null = null;
  let link: L.LinkStatus = { state: 'disabled' };
  let stopTimer: (() => void) | null = null;
  const st: MockState = { lights: true, gear: false, shields: true };
  const lastPress = new Map<string, number>();
  const key = (r: T.ChannelRef) => `${r.install}/${r.channel.toUpperCase()}`;
  const emit = (e: L.TelemetryEvent) => {
    for (const l of listeners) l(e);
  };
  const cfgLine = (on: boolean, line: string): L.TelemetryCfgLine => ({ line, state: on ? 'set' : 'missing', value: null });

  function status(): L.TelemetryStatus {
    return {
      ...clone(base),
      settings: clone(settings),
      endpoint: `127.0.0.1:${settings.port}`,
      env_help: base.env_help.split(String(base.settings.port)).join(String(settings.port)),
      channels: base.channels.map((c) => {
        const l = lines.get(`${c.install}/${c.channel.toUpperCase()}`) ?? { telemetry: false, triggers: false };
        return { ...clone(c), telemetry_line: cfgLine(l.telemetry, c.telemetry_line.line), triggers_line: cfgLine(l.triggers, c.triggers_line.line) };
      }),
      link: clone(link),
      streaming: streaming ? clone(streaming) : null,
      session,
      schema: streaming ? { key: 'mock', origin: 'synthetic', files: 2 } : null,
    };
  }

  function offered(r: T.ChannelRef): void {
    const c = base.channels.find((x) => x.install === r.install && x.channel === r.channel) ?? ctx.fail('not-found', `No channel ${r.channel} in install ${r.install}`);
    if (!c.offered) ctx.fail('invalid', c.reason ?? 'Live telemetry is not offered for this channel.');
  }

  function stop(): void {
    stopTimer?.();
    stopTimer = null;
    if (streaming) session++;
    streaming = null;
    link = { state: 'disabled' };
  }

  const triggersOn = () => settings.enabled && settings.triggers;

  let alertSettings: L.TelemetryAlertSettings = clone(data.alerts.settings);
  let recent: L.TelemetryAlert[] = [];
  const engine = mockAlertEngine();

  /** The default rules with the overrides applied (the Rust `apply_overrides`, without its checks). */
  function rules(): L.TelemetryAlertRule[] {
    return data.alerts.rules.map((d) => {
      const o = alertSettings.overrides[d.id];
      if (!o) return clone(d);
      const threshold = o.value ?? d.default_threshold;
      const gap = d.default_rearm !== null && d.default_threshold !== null ? d.default_rearm - d.default_threshold : null;
      const rearm = o.rearm ?? (o.value !== null && gap !== null ? Math.round((o.value + gap) * 10_000) / 10_000 : d.default_rearm);
      return { ...clone(d), enabled: o.enabled ?? d.default_enabled, cooldown_s: o.cooldown_s ?? d.default_cooldown_s, threshold, rearm };
    });
  }

  const alertView = (): L.TelemetryAlerts => ({
    settings: clone(alertSettings),
    rules: rules(),
    recent: clone(recent),
    recent_max: data.alerts.recent_max,
    evaluating: !!streaming && alertSettings.notifications !== 'off',
    warnings: [],
  });

  function frameAlerts(raw: Obj, t: number, atMs: number): L.TelemetryAlert[] {
    if (alertSettings.notifications === 'off') return [];
    const fired = engine.evaluate(rules(), raw, t, atMs).map((a) => shape(a, alertSettings));
    recent = [...fired.reverse(), ...recent].slice(0, data.alerts.recent_max);
    return fired;
  }

  const handlers: TelemetryHandlers = {
    telemetry_status: () => status(),
    telemetry_configure: ({ req }) => {
      const s = req.settings;
      if (s.port < 1024 || s.port > 65535) ctx.fail('invalid', `port ${s.port} is below 1024; the game accepts 1024 to 65535`);
      if (s.enabled) {
        if (!req.target) ctx.fail('invalid', 'Pick the channel whose USER.cfg gets the telemetry line.');
        offered(req.target);
      }
      const was = req.target ? (lines.get(key(req.target)) ?? { telemetry: false, triggers: false }) : { telemetry: false, triggers: false };
      settings = clone(s);
      if (!s.enabled) {
        lines.clear();
        stop();
      }
      const outcome = (on: boolean, had: boolean): L.LineOutcome => (on ? (had ? 'already-set' : 'written') : had ? 'reverted' : 'not-ours');
      let tl: L.TelemetryLineChange | null = null;
      let gl: L.TelemetryLineChange | null = null;
      if (req.target) {
        const now = { telemetry: s.enabled, triggers: s.enabled && s.triggers };
        lines.set(key(req.target), now);
        const c = base.channels[0]!;
        tl = { line: c.telemetry_line.line, outcome: outcome(now.telemetry, was.telemetry), value: null };
        gl = { line: c.triggers_line.line, outcome: outcome(now.triggers, was.triggers), value: null };
      }
      ctx.log(s.enabled ? `Live telemetry on (port ${s.port}, game actions ${s.triggers ? 'allowed' : 'off'})` : 'Live telemetry off; its USER.cfg lines were released', req.target?.channel ?? null);
      return { status: status(), telemetry_line: tl, triggers_line: gl, warnings: [] };
    },
    telemetry_start: ({ req }) => {
      if (!settings.enabled) ctx.fail('invalid', 'Turn on Live telemetry under Settings -> Experimental first.');
      offered(req);
      stop();
      session++;
      const me = session;
      streaming = clone(req);
      link = { state: 'connecting' };
      const t0 = Date.now();
      let seq = 0;
      let started = false;
      engine.reset();
      stopTimer = every(100, () => {
        if (me !== session) return;
        if (!started) {
          started = true;
          emit({ session: me, status: { state: 'connecting' }, frame: null, alerts: [] });
          link = { state: 'connected' };
          emit({ session: me, status: { state: 'connected' }, frame: null, alerts: [] });
          return;
        }
        seq += 2;
        const t = (Date.now() - t0) / 1000;
        const frame = mockFrame(template, seq, t, st);
        emit({ session: me, status: null, frame, alerts: frameAlerts(frame.raw, t, frame.received_ms) });
      });
      return status();
    },
    telemetry_stop: () => {
      stop();
      return status();
    },
    telemetry_actions: () => ({ ...clone(data.actions), triggers_active: triggersOn(), source: streaming ? 'mock' : 'none', actions: streaming ? clone(data.actions.actions) : [] }),
    telemetry_trigger: ({ req }) => {
      const refuse = (reason: L.RefusalReason, message: string): L.TelemetryTriggerResult => ({ action: req.action, outcome: null, refused: { reason, message }, error: null });
      if (!triggersOn()) return refuse('disabled', 'game action triggers are off');
      if (!streaming) return { action: req.action, outcome: null, refused: null, error: 'live telemetry is not running; open the Live telemetry screen' };
      if (!/^[a-z0-9_]{1,64}$/.test(req.action)) return refuse('invalid_name', 'action names are 1-64 characters of a-z, 0-9 and _');
      if (req.hold_ms !== null && (req.hold_ms < 1 || req.hold_ms > 10_000)) return refuse('invalid_hold', 'a hold is 1-10000 ms');
      const a = data.actions.actions.find((x) => x.action_name === req.action);
      if (!a) return refuse('not_listed', 'the game does not list this action as triggerable');
      if (req.hold_ms !== null && !a.holdable) return refuse('hold_not_allowed', 'this action cannot be held');
      if (data.actions.dangerous.includes(req.action) && !req.confirmed) return refuse('needs_confirm', 'this action needs a confirmation');
      const now = Date.now();
      if (now - (lastPress.get(req.action) ?? -Infinity) < 1000) return refuse('debounced', 'pressed too soon after the last press');
      lastPress.set(req.action, now);
      for (const [k, toggle, on, off] of TOGGLES) {
        if (req.action === toggle) st[k] = !st[k];
        else if (req.action === on) st[k] = true;
        else if (req.action === off) st[k] = false;
      }
      ctx.log(`Game action ${req.action}: OK`, null);
      return { action: req.action, outcome: { status: 'OK', message: 'mock: accepted' }, refused: null, error: null };
    },
    telemetry_alerts: () => alertView(),
    telemetry_alerts_configure: ({ req }) => {
      for (const [id, o] of Object.entries(req.overrides)) {
        const d = data.alerts.rules.find((r) => r.id === id) ?? ctx.fail('invalid', `${id}: no such alert rule`);
        if (o.cooldown_s !== null && (o.cooldown_s < 0 || o.cooldown_s > 3600)) ctx.fail('invalid', `${id}: the cooldown is 0 to 3600 seconds, not ${o.cooldown_s}`);
        if ((o.value !== null || o.rearm !== null) && d.kind !== 'threshold') ctx.fail('invalid', `${id}: only a threshold rule has a value to change`);
        if (d.unit === 'fraction' && [o.value, o.rearm].some((x) => x !== null && (x < 0 || x > 1))) ctx.fail('invalid', `${id}: a fraction is 0 to 1 (0 to 100 %)`);
      }
      if (req.notifications !== alertSettings.notifications) ctx.log(`Live telemetry notifications: ${req.notifications} (speech ${req.speech ? 'on' : 'off'})`, null);
      if (alertSettings.notifications === 'off' && req.notifications !== 'off') engine.reset();
      alertSettings = clone(req);
      return alertView();
    },
    telemetry_alert_test: ({ req }) => {
      if (alertSettings.notifications === 'off') ctx.fail('invalid', 'Turn on Notifications under Settings -> Experimental first.');
      const r = rules().find((x) => x.id === req.rule) ?? ctx.fail('not-found', `No alert rule ${req.rule}`);
      const a: L.TelemetryAlert = { rule: r.id, severity: r.severity, text: `TEST: ${renderAlert(r.message, r.threshold)}`, speech: null, sound: r.sound, value: r.threshold, at_ms: Date.now(), test: true };
      return shape(a, alertSettings);
    },
  };

  return {
    handlers,
    onTelemetry(cb: (e: L.TelemetryEvent) => void): () => void {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
