// Slow and failing backend calls behind the wizard's preview and Review (Windows report: an empty
// preview column and a grey block that never resolved). A fake backend answers late, fails, or
// answers out of order. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { SLOW_AFTER_S, elapsed, fmtElapsed, latest, previewView, type Load } from '../src/lib/loadstate.ts';

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
/** A backend call that answers `value` after `ms`, or fails with `error`. */
const slowCall = <T>(ms: number, value: T, error?: Error) => async (): Promise<T> => {
  await wait(ms);
  if (error) throw error;
  return value;
};
const record = <T>() => {
  const seen: Load<T, string>[] = [];
  const l = latest<T, string>((x) => seen.push(x), (e) => (e as Error).message, () => 1000);
  return { seen, l, last: () => seen.at(-1)! };
};

test('a slow call shows loading (with its start time) until it answers', async () => {
  const { l, last } = record<number>();
  const p = l.run(slowCall(40, 7));
  assert.deepEqual(last(), { state: 'loading', since: 1000 });
  await wait(10);
  assert.equal(last().state, 'loading', 'still loading while the backend works');
  await p;
  assert.deepEqual(last(), { state: 'ok', value: 7 });
});

test('a failed call ends in an error with the message, and Retry starts over', async () => {
  const { l, seen, last } = record<number>();
  await l.run(slowCall(5, 0, new Error('Data.p4k is being used by another process')));
  assert.deepEqual(last(), { state: 'error', error: 'Data.p4k is being used by another process' });
  await l.run(slowCall(5, 3));
  assert.deepEqual(seen.map((s) => s.state), ['loading', 'error', 'loading', 'ok']);
});

test('an older, slower answer never replaces a newer one', async () => {
  const { l, last } = record<string>();
  const old = l.run(slowCall(60, 'old preset'));
  const now = l.run(slowCall(5, 'new preset'));
  await now;
  assert.deepEqual(last(), { state: 'ok', value: 'new preset' });
  await old;
  assert.deepEqual(last(), { state: 'ok', value: 'new preset' });
});

test('an older failure is dropped too, and stop() drops a running call', async () => {
  const { l, seen, last } = record<number>();
  const old = l.run(slowCall(30, 0, new Error('late')));
  await l.run(slowCall(1, 1));
  await old;
  assert.deepEqual(last(), { state: 'ok', value: 1 });
  const n = seen.length;
  const p = l.run(slowCall(10, 2));
  l.stop();
  await p;
  assert.equal(seen.length, n + 1, 'only the loading state; the answer after stop() is ignored');
});

test('preview: an empty answer means "no game data read yet", not "nothing to show"', () => {
  assert.equal(previewView({ state: 'idle' }), 'loading');
  assert.equal(previewView({ state: 'loading', since: 0 }), 'loading');
  assert.equal(previewView({ state: 'ok', value: [] }), 'nodata');
  assert.equal(previewView({ state: 'ok', value: [{ key: 'k' }] }), 'items');
  assert.equal(previewView({ state: 'error', error: 'x' }), 'error');
});

test('elapsed time and the slow hint', () => {
  assert.equal(elapsed(1000, 13_999), 12);
  assert.equal(elapsed(5000, 1000), 0, 'clock skew never goes negative');
  assert.equal(fmtElapsed(12), '12 s');
  assert.equal(fmtElapsed(65), '1 min 05 s');
  assert.ok(SLOW_AFTER_S >= 10 && SLOW_AFTER_S <= 30);
});
