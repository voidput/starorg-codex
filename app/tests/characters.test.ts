// Character labels and the auto fallback hint (src/lib/characters.ts). Synthetic names.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { characterLabel, fallbackHint, largerHint, newestId } from '../src/lib/characters.ts';

const c = (id: string, name: string, last_seen: string | null) => ({ id, name, last_seen, owned: 0 });

test('same-name characters are told apart; others keep the plain name', () => {
  const all = [c('1', 'Test_Pilot', '2026-01-01T00:00:00Z'), c('2', 'test_pilot', '2026-03-01T00:00:00Z'), c('3', 'Alt_Pilot', null)];
  assert.equal(characterLabel(all[1]!, all), 'test_pilot · newest');
  assert.match(characterLabel(all[0]!, all), /^Test_Pilot · last seen \S/);
  assert.equal(characterLabel(all[2]!, all), 'Alt_Pilot');
});

test('fallback hint names both characters, only when auto fell back', () => {
  const all = [c('1', 'Old_Pilot', null), c('2', 'New_Pilot', null)];
  assert.equal(fallbackHint(all, '1', null), null);
  assert.match(fallbackHint(all, '1', '2')!, /^Using Old_Pilot's blueprints: the newest character, New_Pilot, has none/);
  // Same names: the labels tell them apart.
  const twins = [c('1', 'Test_Pilot', '2026-01-01T00:00:00Z'), c('2', 'Test_Pilot', '2026-03-01T00:00:00Z')];
  assert.match(fallbackHint(twins, '1', '2')!, /^Using Test_Pilot · last seen .+'s blueprints: the newest character, Test_Pilot · newest, has none/);
});

test('larger hint and newest character', () => {
  const all = [c('1', 'Big_Pilot', '2026-01-01T00:00:00Z'), c('2', 'New_Pilot', '2026-03-01T00:00:00Z')];
  assert.equal(largerHint(all, '2', null), null);
  assert.match(largerHint(all, '2', '1')!, /^Big_Pilot owns far more blueprints than New_Pilot.*Any of my characters/);
  assert.equal(newestId(all), '2');
  assert.equal(newestId([]), null);
});
