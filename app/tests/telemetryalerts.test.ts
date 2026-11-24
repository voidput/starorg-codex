// Live telemetry alerts (experimental): the webview's audio queue, recent list and threshold
// text (src/lib/telemetryAlerts.ts), and the TS mock's alerts against the Rust mock's samples.
// Synthetic data only. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { AlertAudioQueue, fromShown, isChanged, mergeRecent, thresholdText, tonePattern, toShown, withOverride } from '../src/lib/telemetryAlerts.ts';
import { createTelemetryMock, mockAlertEngine, renderAlert, type TelemetryMockData } from '../src/lib/contract/mock-telemetry.ts';
import type { TelemetryAlert, TelemetryAlerts, TelemetryEvent } from '../src/lib/contract/telemetry.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const view = samples.telemetry_alerts as TelemetryAlerts;
const rule = (id: string) => view.rules.find((r) => r.id === id)!;

const alert = (rule: string, severity: TelemetryAlert['severity'], sound: string | null = severity): TelemetryAlert => ({ rule, severity, text: rule, speech: null, sound, value: null, at_ms: 0, test: false });

test('the Rust samples: off by default, every rule on, alerts on the frame event', () => {
  assert.equal(view.settings.notifications, 'off');
  assert.equal(view.settings.speech, false);
  assert.deepEqual(view.settings.overrides, {});
  assert.ok(view.rules.length >= 15 && view.rules.every((r) => r.enabled && !isChanged(r)));
  const ev = samples.telemetry_event_alert as TelemetryEvent;
  assert.equal(ev.alerts.length, 1);
  assert.equal(ev.alerts[0]!.rule, 'hull_half');
  assert.equal((samples.telemetry_event_frame as TelemetryEvent).alerts.length, 0);
  const on = samples.telemetry_alerts_configure as TelemetryAlerts;
  assert.equal(on.settings.notifications, 'in_app_sound');
  const hull = on.rules.find((r) => r.id === 'hull_half')!;
  assert.equal(hull.threshold, 0.6);
  assert.ok(Math.abs(hull.rearm! - 0.7) < 1e-9, 'the re-arm gap moves with the threshold');
  assert.ok(isChanged(hull));
});

test('one alert at a time, critical first and cutting in', () => {
  const q = new AlertAudioQueue(2000, 6, 10_000);
  q.submit(alert('a', 'warn'), 0);
  q.submit(alert('b', 'info'), 0);
  q.submit(alert('silent', 'warn', null), 0);
  assert.equal(q.size, 2, 'an alert without sound or speech is not queued');
  assert.equal(q.next(0)?.alert.rule, 'a', 'warn before info');
  assert.equal(q.next(200), null, 'one at a time');
  q.submit(alert('c', 'critical'), 300);
  const c = q.next(300);
  assert.equal(c?.alert.rule, 'c');
  assert.equal(c?.preempt, true, 'critical cuts off a warn still playing');
  assert.equal(q.next(1500), null, 'the gap after a start');
  assert.equal(q.next(2700)?.alert.rule, 'b');
  // A newer alert of the same rule replaces the waiting one; old ones expire.
  q.submit(alert('d', 'warn'), 3000);
  q.submit(alert('d', 'warn'), 3100);
  assert.equal(q.size, 1);
  assert.equal(q.next(20_000), null, 'expired after 10 s');
  // At most `max` wait: the least important, oldest goes first.
  const small = new AlertAudioQueue(2000, 2, 10_000);
  small.submit(alert('x', 'info'), 0);
  small.submit(alert('y', 'warn'), 1);
  small.submit(alert('z', 'warn'), 2);
  assert.equal(small.size, 2);
  assert.equal(small.next(3)?.alert.rule, 'y');
  assert.ok(small.wakeAt(3)! >= 2003);
});

test('tones, the recent list and thresholds as typed', () => {
  assert.equal(tonePattern(null, 'critical'), null);
  assert.equal(tonePattern('critical', 'critical')?.beeps, 3);
  assert.equal(tonePattern('custom', 'warn')?.beeps, 2, 'an unknown id falls back to its severity');
  const list = Array.from({ length: 50 }, (_, i) => alert(`r${i}`, 'info'));
  const merged = mergeRecent(list, [alert('new1', 'warn'), alert('new2', 'warn')], 50);
  assert.equal(merged.length, 50);
  assert.deepEqual(
    merged.slice(0, 2).map((a) => a.rule),
    ['new2', 'new1'],
  );
  assert.equal(mergeRecent(list, [], 50), list, 'no alert, same list (no re-render)');
  const boost = rule('boost_low');
  assert.equal(toShown(boost, 0.25), 25);
  assert.equal(fromShown(boost, 30), 0.3);
  assert.equal(thresholdText(boost), '≤ 25% (arms again above 35%)');
  assert.equal(thresholdText(rule('g_load')), null, 'a composite has no threshold');
  assert.equal(toShown(rule('health_low'), 30), 30);
});

test('an override back at its default drops out', () => {
  const s = view.settings;
  const boost = rule('boost_low');
  const off = withOverride(s, boost, { enabled: false });
  assert.deepEqual(off.overrides.boost_low, { enabled: false, cooldown_s: null, value: null, rearm: null });
  assert.deepEqual(withOverride(off, boost, { enabled: true }).overrides, {});
  const moved = withOverride(s, boost, { value: 0.3, rearm: null });
  assert.equal(moved.overrides.boost_low?.value, 0.3);
  assert.deepEqual(withOverride(moved, boost, { value: 0.25 }).overrides, {});
  assert.deepEqual(withOverride(moved, boost, null).overrides, {});
});

test('templates render like the Rust engine', () => {
  assert.equal(renderAlert('Boost {pct}%', 0.234), 'Boost 23%');
  assert.equal(renderAlert('Health {value:.0f}', 29.6), 'Health 30');
  assert.equal(renderAlert('Hull {pct}%', null), 'Hull ?%');
});

test('the mock engine: a baseline first, one alert per crossing, cooldown', () => {
  const e = mockAlertEngine();
  const rules = view.rules.filter((r) => r.id === 'boost_low');
  const raw = (b: number) => ({ vehicle: { state: { name: 'X', flight: { instruments: { boost: { reserves: b } } } } } });
  const fired: number[] = [];
  const run = (from: number, n: number, b: number) => {
    for (let i = 0; i < n; i++) {
      const t = from + i * 0.1;
      if (e.evaluate(rules, raw(b), t, 0).length) fired.push(Math.round(t * 10) / 10);
    }
  };
  run(0, 20, 0.1); // low from the start: baseline
  run(2, 5, 0.5); // past the re-arm point
  run(2.5, 5, 0.2); // fires
  run(3, 5, 0.5);
  run(3.5, 5, 0.2); // within the 15 s cooldown
  assert.deepEqual(fired, [2.5]);
});

function mock() {
  const ticks: (() => void)[] = [];
  const data: TelemetryMockData = { status: samples.telemetry_status, actions: samples.telemetry_actions, frame: samples.telemetry_event_frame, alerts: view };
  const m = createTelemetryMock(data, {
    fail: (c, msg) => {
      throw Object.assign(new Error(msg), { code: c });
    },
    log: () => {},
    every: (_ms, f) => {
      ticks.push(f);
      return () => ticks.splice(ticks.indexOf(f), 1);
    },
  });
  const events: TelemetryEvent[] = [];
  m.onTelemetry((e) => events.push(e));
  return { h: m.handlers, events, tick: (n: number) => Array.from({ length: n }, () => [...ticks].forEach((t) => t())) };
}

const ptu = { install: 'inst-7f3a', channel: 'PTU' };

test('the TS mock: notifications are their own choice, and a test alert matches the Rust one', async () => {
  const { h, events, tick } = mock();
  await h.telemetry_configure({ req: { target: ptu, settings: { enabled: true, port: 50051, triggers: true }, replace_other: false } });
  assert.equal((await h.telemetry_alerts({})).settings.notifications, 'off', 'Metrics + actions leave notifications off');
  await assert.rejects(async () => h.telemetry_alert_test({ req: { rule: 'hull_half' } }), (e: { code: string }) => e.code === 'invalid');
  await h.telemetry_start({ req: ptu });
  tick(5);
  assert.ok(events.every((e) => e.alerts.length === 0));
  const rust = samples.telemetry_alerts_configure as TelemetryAlerts;
  const v = await h.telemetry_alerts_configure({ req: rust.settings });
  assert.deepEqual(
    v.rules.map((r) => [r.id, r.enabled, r.cooldown_s, r.threshold]),
    rust.rules.map((r) => [r.id, r.enabled, r.cooldown_s, r.threshold]),
  );
  assert.equal(v.evaluating, true);
  const t = await h.telemetry_alert_test({ req: { rule: 'hull_half' } });
  const r = samples.telemetry_alert_test as TelemetryAlert;
  assert.deepEqual({ ...t, at_ms: 0 }, { ...r, at_ms: 0 });
  await assert.rejects(async () => h.telemetry_alerts_configure({ req: { ...rust.settings, overrides: { g_load: { enabled: null, cooldown_s: null, value: 3, rearm: null } } } }), (e: { code: string }) => e.code === 'invalid');
  await h.telemetry_stop({});
});
