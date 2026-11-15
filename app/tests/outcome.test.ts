// Unit tests for the apply-outcome mapping and the message catalogue.
// Run: npm test (node --test, type stripping; no bundler, no browser).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describeOutcome } from '../src/lib/outcome.ts';
import { RULE_TEXT, logText, staleWords } from '../src/lib/messages.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const base = samples.apply;

test('skipped is not reported as applied and carries the reason', () => {
  const r = { ...base, result: 'skipped', reason: 'Star Citizen is running on LIVE. Close the game, then apply.' };
  const o = describeOutcome(r, 'apply');
  assert.equal(o.tone, 'warning');
  assert.equal(o.done, false);
  assert.equal(o.wrote, false);
  assert.match(o.title, /^Not applied to LIVE/);
  assert.equal(o.detail, r.reason);
});

test('dry run with warnings says nothing was written', () => {
  const r = { ...base, result: 'dry-run', warnings: [{ kind: 'patch-target-changed', detail: 'x' }] };
  const o = describeOutcome(r, 'apply');
  assert.match(o.title, /^Dry run for LIVE: nothing written/);
  assert.doesNotMatch(o.title, /Applied/);
  assert.equal(o.wrote, false);
});

test('every result has its own wording', () => {
  for (const result of ['applied', 'up-to-date', 'skipped', 'failed', 'dry-run', 'needs-setup', 'needs-decision', 'removed', 'restored', 'nothing-to-do']) {
    const o = describeOutcome({ ...base, result, reason: 'why' }, 'remove');
    assert.ok(o.title.length > 0, result);
  }
  assert.equal(describeOutcome({ ...base, result: 'failed', reason: 'disk full' }, 'apply').tone, 'error');
  assert.equal(describeOutcome({ ...base, result: 'up-to-date' }, 'apply').wrote, false);
  assert.match(describeOutcome({ ...base, result: 'needs-decision', reason: 'Smart Citizen' }, 'remove').detail ?? '', /Smart Citizen/);
});

test('recovered journal message is shown after an apply', () => {
  const o = describeOutcome({ ...base, result: 'applied', warnings: [], recovered: 'An earlier apply was interrupted.' }, 'apply');
  assert.match(o.detail ?? '', /interrupted/);
});

test('rule labels cover the rule names of the contract sample', () => {
  const rule = samples.set_override_rejected.rule;
  assert.ok(RULE_TEXT[rule], rule);
  assert.notEqual(RULE_TEXT[rule].label, rule);
});

test('stale reasons and hook log lines read as plain words', () => {
  assert.equal(staleWords(['profile', 'overrides', 'app version']), 'your settings, your edits, app update');
  const l = logText('prelaunch: LIVE is up to date');
  assert.equal(l.text, 'Before launch: LIVE is up to date');
  assert.equal(l.raw, 'prelaunch: LIVE is up to date');
  assert.match(logText('Applied: 1 warning(s): syn-moved-key target changed').text, /no longer matches/);
});
