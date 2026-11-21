// The Explorer record's Combat card: number text, the loadout sum row and the mock samples it renders. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { numText, statText, sumRows } from '../src/lib/combat.ts';
import type { Combat } from '../src/lib/contract/dev.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

test('numbers read grouped with their unit, missing ones as a dash', () => {
  assert.equal(numText(1537.5, 'm/s'), '1,537.5 m/s');
  assert.equal(numText(12000), '12,000');
  assert.equal(numText(null), '–');
  assert.equal(numText(Number.NaN), '–');
  assert.equal(statText({ key: 'rate', label: 'Fire rate', value: 180, unit: 'rpm' }), '180 rpm');
});

test('a ship sample sums its guns and missiles', () => {
  const ship: Combat = samples.dev_record_combat_ship;
  assert.equal(ship.kind, 'ship');
  const guns = sumRows(ship.weapons);
  const dps = ship.stats.find((s) => s.key === 'dps')!.value;
  assert.equal(guns.total, dps);
  assert.ok(ship.weapons.every((w) => w.item.guid !== null), 'stock guns link to their record');
  const missiles = sumRows(ship.missiles);
  assert.equal(missiles.count, ship.stats.find((s) => s.key === 'missiles')!.value);
  assert.deepEqual(sumRows(null), { count: 0, total: null });
  assert.deepEqual(sumRows([{ item: { item: 'x', name: 'x', guid: null }, size: null, count: 2, mount: null, each: null, total: null }]), { count: 2, total: null });
});

test('weapon and missile samples carry their numbers; other records have no card', () => {
  const w: Combat = samples.dev_record_combat_weapon;
  assert.equal(w.kind, 'weapon');
  assert.ok(w.modes.length >= 2);
  assert.deepEqual(w.stats.slice(0, 3).map((s) => s.key), ['dps', 'alpha', 'rate']);
  const m: Combat = samples.dev_record_combat_missile;
  assert.deepEqual(m.stats.slice(0, 3).map((s) => s.key), ['damage', 'lock', 'lockTime']);
  assert.equal(samples.dev_record_combat_none, null);
});
