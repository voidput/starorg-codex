import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fillStates, needsStates } from '../src/lib/patchstates.ts';
import type { PatchView } from '../src/lib/contract/types.ts';

const row = (id: string, state: PatchView['state'], enabled = true): PatchView => ({
  id,
  title: id,
  description: '',
  keys: 1,
  origin: 'built-in',
  state,
  enabled,
});

test('only a checking row asks for states', () => {
  assert.equal(needsStates([row('a', 'applies'), row('b', 'disabled', false)]), false);
  assert.equal(needsStates([row('a', 'applies'), row('b', 'checking')]), true);
});

test('late states fill checking rows only', () => {
  const rows = [row('a', 'checking'), row('b', 'disabled', false), row('c', 'applies'), row('d', 'checking')];
  const states = [row('a', 'target-changed'), row('b', 'disabled', false), row('c', 'not-needed'), row('d', 'applies', false)];
  const out = fillStates(rows, states);
  assert.deepEqual(
    out.map((p) => p.state),
    // c was answered already (a toggle); d was switched meanwhile, so the late state is not its own.
    ['target-changed', 'disabled', 'applies', 'checking'],
  );
  // A patch added meanwhile has no late state and stays as it is.
  assert.deepEqual(fillStates([row('new', 'checking')], states)[0].state, 'checking');
});
