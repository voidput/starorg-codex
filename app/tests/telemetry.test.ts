// Live telemetry (experimental): the screen's logic (src/lib/telemetry.ts) against the Rust
// mock's samples, and the TS mock's stream and trigger gate (mock-telemetry.ts) against the
// Rust mock's answers. Synthetic data only. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  SERIES,
  actionState,
  annunciators,
  at,
  barWidth,
  clock,
  clockText,
  eventLabel,
  eventTone,
  faceLabels,
  fixed,
  gate,
  groupActions,
  groupPeople,
  humanName,
  inspectorText,
  latestValue,
  linkView,
  mergeHistory,
  needsConfirm,
  pct,
  recentEvents,
  recordingText,
  sequencer,
  seriesRange,
  sessionStep,
  snapshotFromRaw,
  sparkPath,
  systems,
  telemetryTarget,
  triggerText,
  whereText,
  windowStats,
} from '../src/lib/telemetry.ts';
import { createTelemetryMock, mockFrame, stamp, type TelemetryMockData } from '../src/lib/contract/mock-telemetry.ts';
import type { GameLogView, TelemetryActions, TelemetryEvent, TelemetryHistory, TelemetryRecordings, TelemetryStatus, TelemetryTriggerResult } from '../src/lib/contract/telemetry.ts';

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
  const data: TelemetryMockData = { status: samples.telemetry_status, actions: samples.telemetry_actions, frame: frameEv, recordings: samples.telemetry_recordings, alerts: samples.telemetry_alerts, gamelog: samples.telemetry_gamelog };
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
const on = (triggers: boolean, gamelog = false, people = false) => ({ enabled: true, port: 50051, triggers, record: false, gamelog, people });

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
  const off = await h.telemetry_configure({ req: { target: null, settings: { enabled: false, port: 50051, triggers: false, record: false, gamelog: false, people: false }, replace_other: false } });
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

// ---- History, recordings, replay -------------------------------------------------------

const hist = samples.telemetry_history as TelemetryHistory;

function more(h: TelemetryHistory, t: number[], speed: number[]): TelemetryHistory {
  const nulls = t.map(() => null);
  return {
    ...h,
    reset: false,
    t_ms: t,
    series: { speed, g: nulls, hydrogen_fuel: nulls, quantum_fuel: nulls, hull: nulls, shield: nulls, health: nulls, oxygen: nulls },
  };
}

test('the Rust history sample has every graphed series, one value per time', () => {
  assert.equal(hist.interval_ms, 1000);
  assert.equal(hist.capacity, 600);
  for (const s of SERIES) assert.equal(hist.series[s.key].length, hist.t_ms.length, s.key);
});

test('history answers merge: new samples appended, a reset or another epoch replaces, the ring stays bounded', () => {
  const last = hist.t_ms.at(-1)!;
  const m = mergeHistory(hist, more(hist, [last + 1000, last + 2000], [1, 2]));
  assert.equal(m.t_ms.length, hist.t_ms.length + 2);
  assert.deepEqual(m.series.speed.slice(-2), [1, 2]);
  assert.equal(m.series.health.length, m.t_ms.length);
  // An overlapping answer adds only what is newer.
  const again = mergeHistory(m, more(hist, [last + 2000, last + 3000], [2, 3]));
  assert.deepEqual(again.series.speed.slice(-3), [1, 2, 3]);
  // Nothing new keeps the same object (no redraw).
  assert.equal(mergeHistory(m, more(hist, [], [])), m);
  const reset = { ...more(hist, [5], [9]), reset: true };
  assert.deepEqual(mergeHistory(m, reset).t_ms, [5]);
  assert.deepEqual(mergeHistory(m, { ...more(hist, [last + 9000], [9]), epoch: hist.epoch + 1 }).t_ms, [last + 9000]);
  const small = { ...hist, capacity: 3 };
  const cut = mergeHistory(small, { ...more(small, [last + 1000, last + 2000], [1, 2]) });
  assert.equal(cut.t_ms.length, 3);
  assert.deepEqual(cut.t_ms, [hist.t_ms.at(-1), last + 1000, last + 2000]);
  // The input is never changed.
  assert.equal(hist.t_ms.length, 3);
});

test('graphs: a fixed or fitted range, paths broken at gaps and missing values', () => {
  assert.deepEqual(seriesRange([0.2, 0.5], { min: 0, max: 1 }), { min: 0, max: 1 });
  const r = seriesRange([10, 20], { min: null, max: null });
  assert.ok(r.min < 10 && r.max > 20);
  const flat = seriesRange([5, 5], { min: null, max: null });
  assert.ok(flat.max > flat.min);
  assert.deepEqual(seriesRange([null], { min: 0, max: null }), { min: 0, max: 1.08 });
  const o = { w: 100, h: 10, end: 10_000, span: 10_000, min: 0, max: 1, gapMs: 3000 };
  assert.equal(sparkPath([0, 1000, 2000], [0, 1, 0.5], o), 'M0 10L10 0L20 5');
  // A missing value and a 5 s gap each start a new run; values outside the range are clamped.
  assert.equal(sparkPath([0, 1000, 2000, 3000, 8000], [0, null, 1, 2, 0], o), 'M0 10M20 0L30 0M80 10');
  // Before the window: left out.
  assert.equal(sparkPath([-5000, 5000], [1, 1], o), 'M50 0');
  assert.equal(sparkPath([], [], o), '');
  assert.equal(latestValue([1, 2, null]), 2);
  assert.equal(latestValue([null]), null);
  assert.deepEqual(windowStats([0, 1, 2], [5, 1, 3], 1), { min: 1, max: 3 });
  assert.equal(windowStats([0], [null], 0), null);
  const speed = SERIES.find((s) => s.key === 'speed')!;
  assert.equal(speed.text(1234.4), '1,234 m/s');
  assert.equal(SERIES.find((s) => s.key === 'health')!.text(81.6), '82%');
  assert.equal(SERIES.find((s) => s.key === 'hull')!.text(0.5), '50%');
});

test('recordings read as time, channel and length', () => {
  const recs = samples.telemetry_recordings as TelemetryRecordings;
  assert.equal(recs.items.length, 2);
  assert.equal(recs.max_files, 20);
  assert.equal(clockText(95_000), '1:35');
  assert.equal(clockText(3_725_000), '1:02:05');
  assert.equal(clockText(null), '—');
  assert.match(recordingText(recs.items[0]!, 'en-US'), /PTU · 4:00$/);
  assert.equal(recordingText({ ...recs.items[0]!, started_ms: null, duration_ms: null }), `${recs.items[0]!.name} · PTU`);
  // The TS stamp matches the Rust one (the seeded names).
  assert.equal(`${stamp(recs.items[0]!.started_ms!)}-PTU.jsonl`, recs.items[0]!.name);
});

test('the TS mock: Record sessions is its own switch, and a recording replays with pause, seek and speed', async () => {
  const { h, events, tick } = mock();
  await h.telemetry_configure({ req: { target: ptu, settings: on(true), replace_other: false } });
  let st = (await h.telemetry_status({})) as TelemetryStatus;
  assert.equal(st.settings.record, false, 'Metrics + actions never turns it on');
  st = await h.telemetry_record({ req: { on: true } });
  assert.equal(st.settings.record, true);
  // A configure keeps it.
  const c = await h.telemetry_configure({ req: { target: ptu, settings: on(false), replace_other: false } });
  assert.equal(c.status.settings.record, true);
  await h.telemetry_start({ req: ptu });
  tick(3);
  st = (await h.telemetry_status({})) as TelemetryStatus;
  assert.ok(st.recording?.endsWith('-PTU.jsonl'), String(st.recording));
  let recs = (await h.telemetry_recordings({})) as TelemetryRecordings;
  assert.equal(recs.items.length, 3);
  assert.equal(recs.items[0]!.name, st.recording);
  await assert.rejects(async () => h.telemetry_recording_delete({ req: { name: st.recording! } }), (e: Fail) => e.code === 'busy');
  // The history fills from the frames.
  const hh = (await h.telemetry_history({ req: { epoch: null, since_ms: null } })) as TelemetryHistory;
  assert.ok(hh.reset && hh.t_ms.length >= 1);
  // Replay a seeded recording: no channel, the replay in the status and on its events.
  const seeded = recs.items[1]!;
  st = await h.telemetry_replay_start({ req: { name: seeded.name } });
  assert.equal(st.streaming, null);
  assert.equal(st.recording, null);
  assert.equal(st.replay?.name, seeded.name);
  const first = events.at(-1)!;
  assert.equal(first.frame?.received_ms, seeded.started_ms);
  assert.equal(first.replay?.position_ms, 0);
  tick(2);
  assert.equal(events.at(-1)!.replay!.position_ms, 200);
  st = await h.telemetry_replay_control({ req: { playing: false, seek_ms: 60_000, speed: 99 } });
  assert.equal(st.replay?.playing, false);
  assert.equal(st.replay?.speed, 16);
  assert.equal(st.replay?.position_ms, 60_000);
  const seekEv = events.filter((e) => e.frame).at(-1)!;
  assert.equal(seekEv.frame!.received_ms, seeded.started_ms! + 60_000);
  const after = (await h.telemetry_history({ req: { epoch: null, since_ms: null } })) as TelemetryHistory;
  assert.equal(after.t_ms.length, 61, 'the minute that led there, and the frame');
  tick(5);
  assert.equal((await h.telemetry_status({})).replay?.position_ms, 60_000, 'paused');
  await h.telemetry_replay_control({ req: { playing: true, seek_ms: null, speed: null } });
  tick(200);
  st = (await h.telemetry_status({})) as TelemetryStatus;
  assert.equal(st.replay?.ended, true);
  assert.equal(st.replay?.position_ms, seeded.duration_ms);
  await assert.rejects(async () => h.telemetry_recording_delete({ req: { name: seeded.name } }), (e: Fail) => e.code === 'busy');
  await h.telemetry_stop({});
  await assert.rejects(async () => h.telemetry_replay_control({ req: { playing: true, seek_ms: null, speed: null } }), (e: Fail) => e.code === 'invalid');
  recs = await h.telemetry_recording_delete({ req: { name: seeded.name } });
  assert.equal(recs.items.length, 2);
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
