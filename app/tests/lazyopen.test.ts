// Explorer and Files open their workers only on a click, a hand-off or a repeat visit. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Opened, openPlan } from '../src/lib/lazyopen.ts';

test('a ready worker is used as is', () => {
  assert.equal(openPlan('ready', false, false), 'ready');
  assert.equal(openPlan('ready', true, true), 'ready');
});

test('opening the screen alone does not start the worker', () => {
  assert.equal(openPlan('stopped', false, false), 'ask');
  assert.equal(openPlan(null, false, false), 'ask');
  assert.equal(openPlan(undefined, false, false), 'ask');
});

test('a hand-off or an earlier open this session starts it', () => {
  assert.equal(openPlan('stopped', true, false), 'open');
  assert.equal(openPlan('stopped', false, true), 'open');
});

test('a worker another screen is starting is waited for', () => {
  assert.equal(openPlan('starting', false, false), 'open');
});

test('a crashed worker waits for Retry, even on a hand-off', () => {
  assert.equal(openPlan('failed', true, true), 'ask');
});

test('opened channels are kept per screen and forgotten on close', () => {
  const o = new Opened();
  assert.equal(o.has('explorer', 'i/LIVE'), false);
  o.add('explorer', 'i/LIVE');
  assert.equal(o.has('explorer', 'i/LIVE'), true);
  assert.equal(o.has('files', 'i/LIVE'), false);
  assert.equal(o.has('explorer', 'i/PTU'), false);
  o.forget('explorer', 'i/LIVE');
  assert.equal(o.has('explorer', 'i/LIVE'), false);
  o.add('files', '');
  assert.equal(o.has('files', ''), false);
});
