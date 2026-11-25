// Sidebar groups: Game is for reading the game, App holds what the app did (Backups, Activity)
// and Settings. Developer mode puts Dev before App and moves only Settings to Alt+0.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { findNav, isScreen, navGroups, navItems } from '../src/lib/nav.ts';

const layout = (dev: boolean, telemetry = false) => navGroups(dev, telemetry).map((g) => [g.label, g.items.map((i) => `${i.id}:${i.key}`)]);

test('Game reads the game; Backups and Activity sit with Settings', () => {
  assert.deepEqual(layout(false), [
    [null, ['home:1']],
    ['Customize', ['enhancements:2', 'strings:3', 'patches:4', 'blueprints:5', 'ships:6']],
    ['Game', ['shops:', 'commodities:', 'mining:', 'loot:']],
    ['App', ['backups:7', 'activity:8', 'settings:9']],
  ]);
});

test('Developer mode adds Dev before App and moves only Settings to Alt+0', () => {
  const groups = navGroups(true);
  assert.deepEqual(
    groups.map((g) => g.label),
    [null, 'Customize', 'Game', 'Dev', 'App'],
  );
  assert.deepEqual(layout(true).at(-1), ['App', ['backups:7', 'activity:8', 'settings:0']]);
  const keys = navItems(true)
    .map((i) => i.key)
    .filter(Boolean);
  assert.equal(new Set(keys).size, keys.length, 'no Alt key used twice');
});

test('Live telemetry joins Game only while the feature is on, labelled experimental and without a key', () => {
  assert.ok(!navItems(false).some((i) => i.id === 'telemetry'));
  assert.ok(!navItems(true).some((i) => i.id === 'telemetry'));
  assert.deepEqual(layout(false, true)[2], ['Game', ['shops:', 'commodities:', 'mining:', 'loot:', 'telemetry:']]);
  const withDev = layout(true, true);
  assert.deepEqual(withDev[2], ['Game', ['shops:', 'commodities:', 'mining:', 'loot:', 'telemetry:']]);
  assert.deepEqual(withDev.at(-1), ['App', ['backups:7', 'activity:8', 'settings:0']]);
  const item = navItems(false, true).find((i) => i.id === 'telemetry')!;
  assert.equal(item.label, 'Live telemetry');
  assert.equal(item.badge, 'Experimental');
  // Deep links and the palette know the screen even while it is hidden.
  assert.ok(isScreen('telemetry'));
  assert.equal(findNav('telemetry').label, 'Live telemetry');
});
