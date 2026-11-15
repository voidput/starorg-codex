// Character labels and the auto fallback hint (src/lib/characters.ts), and where Smart Citizen
// saves its export per OS (src/lib/platform.ts). Synthetic names. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { characterLabel, fallbackHint } from '../src/lib/characters.ts';
import { smartCitizenDir } from '../src/lib/platform.ts';

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
  assert.match(fallbackHint(all, '1', '2')!, /^Using Old_Pilot's blueprints: the newest character \(New_Pilot\) has none/);
});

test('Smart Citizen export folder per OS', () => {
  assert.equal(smartCitizenDir(true), '%USERPROFILE%\\SmartCitizen\\');
  assert.doesNotMatch(smartCitizenDir(true), /Wine|drive_c/);
  assert.match(smartCitizenDir(false), /Wine prefix, drive_c\/users\/<you>\/SmartCitizen\//);
});
