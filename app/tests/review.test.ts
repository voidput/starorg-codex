// Review's gate on "Set up and apply" (review of the Windows fix): the dry run must have answered,
// every chosen channel's game data must be read (the real backend answers Ok with a game-data
// warning and zero counts when a read fails), and another tool's file needs the tick, also when
// only the dry run found it. The tick is never saved with the draft. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ackKey, emptyDraft, foreignPicked, loadDraft, reviewBlock, saveDraft, unreadChannels } from '../src/components/wizard/draft.svelte.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const install = structuredClone(samples.overview_first_run.installs[0]);
for (const c of install.channels) Object.assign(c, { status: { state: 'not-applied' }, foreign_file: false, foreign_tool: null });
const plan = structuredClone(samples.plan_setup);

const draft = () => {
  const d = emptyDraft();
  for (const c of install.channels) d.channels[c.channel] = { consent: c.channel === 'LIVE', switch_language: false };
  return d;
};
const planFor = (over: object = {}) => ({
  ...plan,
  install: install.id,
  warnings: [],
  channels: [{ ...plan.channels[0], channel: 'LIVE', replaces_tool: null, ...over }],
});

test('blocked while the dry run has not answered', () => {
  assert.match(reviewBlock(draft(), install, null)!, /Wait until the review has read the game data/);
  assert.equal(reviewBlock(draft(), install, planFor()), null);
});

test('a failed read (Ok + game-data warning, zero counts) blocks and names the channel', () => {
  const p = planFor({ counts: { ...plan.channels[0].counts, changed: 0 } });
  p.warnings = [{ kind: 'game-data', detail: 'LIVE: reading game data: Data.p4k: The process cannot access the file (os error 32)' }];
  assert.deepEqual(unreadChannels(p), [{ channel: 'LIVE', why: 'reading game data: Data.p4k: The process cannot access the file (os error 32)' }]);
  assert.match(reviewBlock(draft(), install, p)!, /Could not read the game data of LIVE/);
  p.warnings = [{ kind: 'language', detail: 'PTU: the game language is German; it will be skipped' }];
  assert.equal(reviewBlock(draft(), install, p), null, 'other warnings do not block');
});

test('a file the dry run found (replaces_tool) needs the tick too', () => {
  const d = draft();
  const p = planFor({ replaces_tool: 'Smart Citizen' });
  const fs = foreignPicked(d, install, p);
  assert.deepEqual(fs, [{ channel: 'LIVE', tool: 'Smart Citizen' }]);
  assert.match(reviewBlock(d, install, p)!, /Back up and replace/);
  d.replace_ack = ackKey(fs);
  assert.equal(reviewBlock(d, install, p), null);
});

test('the tick is not saved with the draft: a resumed setup asks again', () => {
  const store = new Map<string, string>();
  (globalThis as { localStorage?: unknown }).localStorage = {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  };
  const d = draft();
  d.replace_ack = 'LIVE:Smart Citizen';
  saveDraft(d);
  assert.ok(!store.values().next().value!.includes('Smart Citizen'));
  assert.equal(loadDraft()!.replace_ack, null);
  delete (globalThis as { localStorage?: unknown }).localStorage;
});
