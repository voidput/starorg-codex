// Live telemetry (experimental): the screen's logic (src/lib/telemetry.ts) against the Rust
// mock's samples, and the TS mock's stream and trigger gate (mock-telemetry.ts) against the
// Rust mock's answers. Synthetic data only. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { actionState, annunciators, at, barWidth, clock, eventLabel, eventTone, faceLabels, fixed, gate, groupActions, groupPeople, humanName, inspectorText, linkView, needsConfirm, pct, recentEvents, sequencer, sessionStep, snapshotFromRaw, systems, telemetryTarget, triggerText, whereText } from '../src/lib/telemetry.ts';
import { createTelemetryMock, mockFrame, type TelemetryMockData } from '../src/lib/contract/mock-telemetry.ts';
import type { GameLogView, TelemetryActions, TelemetryEvent, TelemetryStatus, TelemetryTriggerResult } from '../src/lib/contract/telemetry.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const frameEv = samples.telemetry_event_frame as TelemetryEvent;
const frame = frameEv.frame!;

class Fail extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

test('the TS snapshot reads a frame exactly as the Rust one', () => {
  assert.deepEqual(snapshotFromRaw(frame.raw), frame.snapshot);
  assert.equal(frame.snapshot.ship?.name, 'Mock Gladius');
  // A frame outside a vehicle and a FLT_MAX "no body" value.
  const onFoot = snapshotFromRaw({ actor: { vitals: { heart_rate: 3.4e38, gravity: 1 } } });
  assert.equal(onFoot.ship, null);
  assert.equal(onFoot.vitals.heart_rate, null);
  assert.equal(onFoot.vitals.gravity, 1);
  assert.equal(at(frame.raw, 'vehicle.state.name'), 'Mock Gladius');
  assert.equal(at(frame.raw, 'vehicle.nope.deeper'), undefined);
});

test('readouts: percent, fixed, bars and face labels', () => {
  assert.equal(pct(0.8234), '82%');
  assert.equal(pct(null), '—');
  assert.equal(fixed(1234.5, 0, 'm/s'), '1,235 m/s');
  assert.equal(fixed(undefined), '—');
  assert.equal(barWidth(1.7), '100.0%');
  assert.equal(barWidth(-1), '0.0%');
  assert.deepEqual(faceLabels(1), ['ALL']);
  assert.deepEqual(faceLabels(4), ['F1', 'F2', 'F3', 'F4']);
  assert.equal(humanName('v_power_toggle_shields'), 'Power toggle shields');
  assert.equal(humanName('spaceship_power'), 'Spaceship power');
});

test('every link state has words, and a stopped stream says so', () => {
  const states = ['disabled', 'connecting', 'connected', 'game_not_running', 'unreachable', 'no_schema'] as const;
  for (const s of states) {
    const v = linkView(s === 'unreachable' || s === 'no_schema' ? { state: s, reason: 'x' } : { state: s }, true);
    assert.ok(v.label, s);
  }
  assert.equal(linkView({ state: 'no_schema', reason: 'no exe' }, true).detail, 'no exe');
  assert.equal(linkView({ state: 'disabled' }, false).label, 'Stopped');
});

test('systems and annunciators come from the frame', () => {
  const sys = Object.fromEntries(systems(frame.raw).map((s) => [s.id, s]));
  assert.equal(sys.lights!.on, true);
  assert.equal(sys.gear!.text, 'UP');
  assert.equal(sys.engines!.on, true);
  assert.equal(systems({}).every((s) => s.on === null), true, 'a build without the fields shows dashes');
  const low = structuredClone(frame);
  low.snapshot.ship!.hydrogen_fuel = 0.05;
  low.snapshot.vitals.oxygen = 0.1;
  (low.raw.vehicle as { state: { power: { shield_generators: boolean } } }).state.power.shield_generators = false;
  const ids = annunciators(low.snapshot, low.raw).map((a) => a.id);
  assert.deepEqual(ids, ['oxygen', 'fuel', 'shields-off']);
});

test('actions group by category and show the state they change', () => {
  const a = samples.telemetry_actions as TelemetryActions;
  const g = groupActions(a.actions);
  assert.deepEqual(
    g.map((x) => x.category),
    [...new Set(a.actions.map((x) => x.category))].sort(),
  );
  assert.equal(actionState(frame.raw, 'v_lights_off'), 'ON');
  assert.equal(actionState(frame.raw, 'v_deploy_landing_system'), 'UP');
  assert.equal(actionState(frame.raw, 'v_power_toggle_shields'), 'ON');
  assert.equal(actionState(frame.raw, 'v_invoke_ping'), null);
});

test('the first press of a session and every dangerous one ask first', () => {
  const d = ['v_self_destruct'];
  assert.equal(needsConfirm('v_lights', d, false), true);
  assert.equal(needsConfirm('v_lights', d, true), false);
  assert.equal(needsConfirm('v_self_destruct', d, true), true);
  assert.equal(triggerText(samples.telemetry_trigger as TelemetryTriggerResult).ok, true);
  const refused = triggerText(samples.telemetry_trigger_refused as TelemetryTriggerResult);
  assert.equal(refused.ok, false);
  assert.match(refused.text, /^Not sent: this action needs a confirmation/);
  assert.match(triggerText({ outcome: { status: 'MAP_INACTIVE', message: '' }, refused: null, error: null }).text, /not available where you are/);
});

test('the inspector cuts long frames and follows at most once a second', () => {
  const big = Object.fromEntries(Array.from({ length: 500 }, (_, i) => [`k${i}`, i]));
  const v = inspectorText(big, 100);
  assert.equal(v.cut, true);
  assert.equal(v.text.split('\n').length, 100);
  assert.equal(inspectorText(frame.raw).cut, false);
  let now = 0;
  const g = gate(1000, () => now);
  assert.equal(g(), true);
  now = 500;
  assert.equal(g(), false);
  now = 1000;
  assert.equal(g(), true);
});

function mock() {
  const ticks: (() => void)[] = [];
  const logs: string[] = [];
  const data: TelemetryMockData = { status: samples.telemetry_status, actions: samples.telemetry_actions, frame: frameEv, gamelog: samples.telemetry_gamelog };
  const m = createTelemetryMock(data, {
    fail: (c, msg) => {
      throw new Fail(c, msg);
    },
    log: (msg) => logs.push(msg),
    every: (_ms, f) => {
      ticks.push(f);
      return () => ticks.splice(ticks.indexOf(f), 1);
    },
  });
  const events: TelemetryEvent[] = [];
  m.onTelemetry((e) => events.push(e));
  const tick = (n = 1) => {
    for (let i = 0; i < n; i++) for (const t of [...ticks]) t();
  };
  return { m, h: m.handlers, events, tick, logs, ticks };
}

const ptu = { install: 'inst-7f3a', channel: 'PTU' };
const live = { install: 'inst-7f3a', channel: 'LIVE' };
const on = (triggers: boolean, gamelog = false, people = false) => ({ enabled: true, port: 50051, triggers, gamelog, people });

test('the TS mock starts off, offers PTU only and answers like the Rust mock', async () => {
  const { h } = mock();
  const s = (await h.telemetry_status({})) as TelemetryStatus;
  assert.equal(s.settings.enabled, false);
  assert.deepEqual(s.settings, (samples.telemetry_status as TelemetryStatus).settings);
  await assert.rejects(async () => h.telemetry_configure({ req: { target: live, settings: on(false), replace_other: false } }), (e: Fail) => e.code === 'invalid' && /4\.10\.0/.test(e.message));
  await assert.rejects(async () => h.telemetry_start({ req: ptu }), (e: Fail) => e.code === 'invalid');
  const r = await h.telemetry_configure({ req: { target: ptu, settings: on(true), replace_other: false } });
  const rust = samples.telemetry_configure;
  assert.equal(r.telemetry_line?.outcome, rust.telemetry_line.outcome);
  assert.equal(r.triggers_line?.outcome, rust.triggers_line.outcome);
  const c = r.status.channels.find((x) => x.channel === 'PTU')!;
  assert.equal(c.telemetry_line.state, 'set');
  assert.equal(c.triggers_line.state, 'set');
});

test('the TS mock streams synthetic frames, and presses flip the lights', async () => {
  const { h, events, tick, ticks } = mock();
  await h.telemetry_configure({ req: { target: ptu, settings: on(false), replace_other: false } });
  const st = await h.telemetry_start({ req: ptu });
  assert.deepEqual(st.streaming, ptu);
  tick(3);
  assert.deepEqual(
    events.filter((e) => e.status).map((e) => e.status!.state),
    ['connecting', 'connected'],
  );
  const frames = events.filter((e) => e.frame);
  assert.equal(frames.length, 2);
  assert.equal(frames[0]!.session, st.session);
  assert.deepEqual(frames[0]!.frame!.snapshot, snapshotFromRaw(frames[0]!.frame!.raw));
  const press = (action: string, confirmed = true, hold_ms: number | null = null) => h.telemetry_trigger({ req: { action, hold_ms, confirmed } }) as TelemetryTriggerResult;
  assert.equal(press('v_lights').refused?.reason, 'disabled');
  await h.telemetry_configure({ req: { target: ptu, settings: on(true), replace_other: false } });
  assert.equal(press('v_lights').outcome?.status, 'OK');
  assert.equal(press('v_lights').refused?.reason, 'debounced');
  assert.equal(press('v_self_destruct', false).refused?.reason, 'needs_confirm');
  assert.equal(press('nope').refused?.reason, 'not_listed');
  assert.equal(press('toggle_flashlight', true, 500).refused?.reason, 'hold_not_allowed');
  tick();
  assert.equal(events.at(-1)!.frame!.snapshot.ship!.lights, false);
  // Off stops the stream and releases the lines.
  const off = await h.telemetry_configure({ req: { target: null, settings: { enabled: false, port: 50051, triggers: false, gamelog: false, people: false }, replace_other: false } });
  assert.equal(off.status.streaming, null);
  assert.equal(ticks.length, 0);
});

test('a synthetic frame keeps the template and moves its values', () => {
  const a = mockFrame(frame.raw, 1, 0, { lights: false, gear: true, shields: true }, 1);
  const b = mockFrame(frame.raw, 2, 10, { lights: false, gear: true, shields: true }, 2);
  assert.equal(a.snapshot.ship?.gear, 'DEPLOYED');
  assert.equal(a.snapshot.ship?.lights, false);
  assert.notEqual(a.snapshot.ship?.speed, b.snapshot.ship?.speed);
  assert.equal(at(a.raw, 'actor.state.seat_name'), at(frame.raw, 'actor.state.seat_name'), 'fields the generator does not move stay');
});

test('a newer stream clears the old frame; older events are dropped', () => {
  assert.equal(sessionStep(3, 2), 'stale');
  assert.equal(sessionStep(3, 3), 'same');
  assert.equal(sessionStep(3, 4), 'new');
});

test('the telemetry channel: picked, streaming, the one with the line, the app channel, the first', () => {
  const ch = (channel: string, state = 'missing') => ({ install: 'i', channel, telemetry_line: { state } });
  const offered = [ch('LIVE'), ch('PTU', 'set'), ch('TECH')];
  assert.equal(telemetryTarget(offered, 'i/TECH', { install: 'i', channel: 'LIVE' }, 'i/LIVE')?.channel, 'TECH');
  assert.equal(telemetryTarget(offered, null, { install: 'i', channel: 'LIVE' }, 'i/TECH')?.channel, 'LIVE');
  assert.equal(telemetryTarget(offered, null, null, 'i/TECH')?.channel, 'PTU', 'the channel that has the line beats the app channel');
  assert.equal(telemetryTarget([ch('LIVE'), ch('TECH')], null, null, 'i/TECH')?.channel, 'TECH');
  assert.equal(telemetryTarget([ch('LIVE'), ch('TECH')], 'i/GONE', null, null)?.channel, 'LIVE');
  assert.equal(telemetryTarget([], 'i/PTU', null, null), null);
});

test('a sequencer keeps only the latest answer', () => {
  const s = sequencer();
  const a = s.next();
  const b = s.next();
  assert.equal(s.latest(a), false);
  assert.equal(s.latest(b), true);
});

const gamelog = samples.telemetry_gamelog as GameLogView;

test('Game.log events: newest first, tones, labels, where and the people groups', () => {
  const ev = recentEvents(gamelog);
  assert.equal(ev.length, gamelog.events.length);
  assert.ok(ev[0]!.seq > ev.at(-1)!.seq, 'newest first');
  assert.equal(recentEvents(gamelog, 3).length, 3);
  assert.deepEqual(recentEvents(null), []);
  assert.equal(eventTone('session_dropped'), 'destructive');
  assert.equal(eventTone('spawned'), 'muted');
  assert.equal(eventLabel('respawned'), 'Respawn');
  assert.equal(clock(Date.UTC(2026, 10, 23, 14, 3, 27), 'UTC'), '14:03:27');
  assert.equal(clock(null), '—');
  assert.match(whereText(gamelog), /^Stanton · Mock Planet One · /);
  const g = groupPeople(gamelog.people!.people);
  assert.deepEqual(
    g.map((x) => x.relation),
    ['crew', 'party', 'party_likely', 'nearby'],
  );
  assert.equal(g[0]!.people[0]!.name, 'TestPilot_Lead');
  assert.deepEqual(groupPeople(null), []);
  // Samples carry synthetic handles only.
  for (const p of gamelog.people!.people) assert.match(p.name, /^TestPilot_/);
  // Events never carry a handle.
  assert.ok(!JSON.stringify(gamelog.events).includes('TestPilot'));
});

test('the TS mock sends Game.log only with its switch, and people only with theirs', async () => {
  const { h, events, tick } = mock();
  await h.telemetry_configure({ req: { target: ptu, settings: on(false), replace_other: false } });
  await h.telemetry_start({ req: ptu });
  tick(3);
  assert.ok(events.every((e) => e.gamelog === null), 'off by default');
  assert.equal(await h.telemetry_gamelog({}), null);
  await h.telemetry_configure({ req: { target: ptu, settings: on(false, true), replace_other: false } });
  const g = events.findLast((e) => e.gamelog)?.gamelog;
  assert.ok(g);
  assert.equal(g.people, null);
  await h.telemetry_configure({ req: { target: ptu, settings: on(false, true, true), replace_other: false } });
  assert.equal(events.findLast((e) => e.gamelog)?.gamelog?.people?.counts.total, gamelog.people!.counts.total);
  const cleared = await h.telemetry_people_clear({});
  assert.equal(cleared?.people?.people.length, 0);
  await h.telemetry_stop({});
  assert.equal(await h.telemetry_gamelog({}), null);
  assert.equal(await h.telemetry_people_clear({}), null);
});
