// Setup wizard draft: the Preferences step (defaults, seeding from Settings, what Apply saves,
// the Review rows), over the Rust mock's Settings sample. Run: npm test (node --test, type stripping).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { STEPS, blocker, defaultPrefs, emptyDraft, prefsFrom, prefsSummary, withPrefs } from '../src/components/wizard/draft.svelte.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const settings = samples.get_settings;

test('Preferences sits after Enhancements and before Review, and never blocks', () => {
  const ids = STEPS.map((s) => s.id);
  assert.deepEqual(ids.slice(ids.indexOf('enhancements'), ids.indexOf('enhancements') + 3), ['enhancements', 'prefs', 'review']);
  assert.equal(blocker('prefs', emptyDraft(), null), null);
  assert.equal(emptyDraft().prefs, null, 'seeded from Settings, not guessed');
});

test('defaults keep every network switch off (docs/privacy.md)', () => {
  assert.deepEqual(defaultPrefs(false), { check_updates: false, theme: 'system', reduce_effects: false, starorg: false });
  assert.equal(defaultPrefs(true).reduce_effects, true, 'Linux draws blur slowly');
});

test('seeds from the saved Settings, so a second setup keeps them', () => {
  const s = { ...settings, network: { ...settings.network, check_updates: true }, appearance: { ...settings.appearance, theme: 'light' } };
  const p = prefsFrom(s);
  assert.equal(p.check_updates, true);
  assert.equal(p.theme, 'light');
  assert.equal(p.starorg, s.network.starorg);
});

test('withPrefs changes only the asked fields', () => {
  const p = { check_updates: true, theme: 'dark' as const, reduce_effects: true, starorg: true };
  const next = withPrefs(settings, p, false);
  assert.equal(next.network.check_updates, true);
  assert.equal(next.network.starorg, true);
  assert.equal(next.network.published_dumps, settings.network.published_dumps);
  assert.equal(next.appearance.theme, 'dark');
  assert.equal(next.appearance.reduce_effects, true);
  assert.equal(next.appearance.accent, settings.appearance.accent);
  assert.equal(next.default_channel, settings.default_channel);
  assert.deepEqual(next.advanced, settings.advanced);
  assert.equal(settings.network.check_updates, false, 'input not mutated');
});

test('the Store build keeps its update switch and does not show the question in Review', () => {
  const p = { ...defaultPrefs(false), check_updates: true };
  assert.equal(withPrefs(settings, p, true).network.check_updates, settings.network.check_updates);
  assert.ok(!prefsSummary(p, true).some((r) => r.label === 'Updates'));
});

test('Review says what the update check sends', () => {
  const on = prefsSummary({ ...defaultPrefs(false), check_updates: true }, false);
  assert.match(on.find((r) => r.label === 'Updates')!.value, /api\.github\.com, at most once a day/);
  const off = prefsSummary(defaultPrefs(false), false);
  assert.equal(off.find((r) => r.label === 'Updates')!.value, 'Do not check (no request)');
  assert.equal(off.find((r) => r.label === 'StarOrg Tools')!.value, 'Off');
  assert.equal(off.find((r) => r.label === 'Appearance')!.value, 'Same as the system');
});
