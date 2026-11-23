// The Loot screen's pure helpers (odds text, search and group filter, grouped list rows, the
// visible window, drop labels) and the TS mock's answers, on the Rust mock's data
// (mock-loot.gen.json). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dropWhere, filterSummaries, groupsOf, listEntries, listed, oddsText, queryWords, windowRange } from '../src/lib/loot.ts';
import { mockLootDetail, mockLootList, type LootMockData } from '../src/lib/contract/mock-loot.ts';
import type { LootSummary } from '../src/lib/contract/types.ts';

const data = JSON.parse(readFileSync(new URL('../src/lib/contract/mock-loot.gen.json', import.meta.url), 'utf8')) as LootMockData;

const row = (id: string, name: string, group: string, best = 0.1, count = 1): LootSummary => ({ id, name, group, best, count });

test('odds read like the WHERE TO GET line', () => {
  assert.equal(oddsText(0.75), '75.0%');
  assert.equal(oddsText(0.1), '10.0%');
  assert.equal(oddsText(0.0123), '1.23%');
  assert.equal(oddsText(0.001), '0.10%');
  assert.equal(oddsText(0.00012345), '0.012%');
  assert.equal(oddsText(0), '0%');
  assert.equal(oddsText(Number.NaN), '0%');
});

test('search needs every word in the name, group or id; the group filter is exact', () => {
  const rows = [row('caves/loot_caves_occupied', 'Occupied', 'Caves'), row('v3slotpreset_ugfs', 'Underground facilities', 'Underground facilities'), row('lmg', 'FS-9 LMG', 'FPS weapons')];
  assert.deepEqual(queryWords('  Caves  OCC '), ['caves', 'occ']);
  assert.deepEqual(
    filterSummaries(rows, 'caves occ', '').map((r) => r.id),
    ['caves/loot_caves_occupied'],
  );
  assert.deepEqual(
    filterSummaries(rows, 'ugfs', '').map((r) => r.name),
    ['Underground facilities'],
  );
  assert.equal(filterSummaries(rows, '', 'FPS weapons').length, 1);
  assert.equal(filterSummaries(rows, 'lmg', 'Caves').length, 0);
  assert.deepEqual(filterSummaries(null, 'x', ''), []);
});

test('groups sort by name with Other last; the areas list gets a header per group', () => {
  const rows = [row('a', 'A', 'Caves'), row('b', 'B', 'Caves'), row('c', 'C', 'Other'), row('d', 'D', 'Asteroid bases')];
  assert.deepEqual(groupsOf(rows), [
    { group: 'Asteroid bases', count: 1 },
    { group: 'Caves', count: 2 },
    { group: 'Other', count: 1 },
  ]);
  const sorted = [rows[0]!, rows[1]!, rows[2]!];
  const e = listEntries(sorted, 'areas');
  assert.deepEqual(
    e.map((x) => (x.kind === 'group' ? `#${x.group}:${x.count}` : x.row.id)),
    ['#Caves:2', 'a', 'b', '#Other:1', 'c'],
  );
  assert.ok(listEntries(sorted, 'items').every((x) => x.kind === 'row'));
});

test('the window covers the visible rows plus overscan, clamped to the list', () => {
  assert.deepEqual(windowRange(0, 400, 40, 1000, 5), { first: 0, end: 15 });
  assert.deepEqual(windowRange(4000, 400, 40, 1000, 5), { first: 95, end: 115 });
  assert.deepEqual(windowRange(39_990, 400, 40, 1000, 5), { first: 994, end: 1000 });
  assert.deepEqual(windowRange(100, 400, 40, 0), { first: 0, end: 0 });
  assert.deepEqual(windowRange(-50, 80, 40, 10, 0), { first: 0, end: 2 });
});

test('a drop says what the spot is, its rarity and how the item is there', () => {
  assert.equal(dropWhere({ container: 'Large weapons crate', rarity: 'Rare', kind: 'loot' }), 'Large weapons crate · Rare');
  assert.equal(dropWhere({ container: 'Weapons rack', rarity: null, kind: 'fixed' }), 'Weapons rack · always in it');
  assert.equal(dropWhere({ container: 'Lying loose', rarity: null, kind: 'loose' }), 'Lying loose');
  const rows = [row('a', 'A', 'G')];
  assert.equal(listed(rows, 'a'), 'a');
  assert.equal(listed(rows, 'b'), null);
  assert.equal(listed(rows, null), null);
});

test('the TS mock serves the Rust mock: missing until read, then lists and likeliest-first details', () => {
  assert.deepEqual(mockLootList(data, false, 'areas'), { state: 'missing', view: 'areas', build: null, rows: [] });
  const areas = mockLootList(data, true, 'areas');
  const items = mockLootList(data, true, 'items');
  assert.equal(areas.state, 'ready');
  assert.ok(areas.rows.length >= 10 && items.rows.length >= 40);
  // Summaries only: no drops in a list row.
  assert.deepEqual(Object.keys(areas.rows[0]!).sort(), ['best', 'count', 'group', 'id', 'name']);
  for (const r of areas.rows) {
    const d = mockLootDetail(data, true, 'areas', r.id.toUpperCase());
    assert.equal(d.entry?.id, r.id);
    assert.equal(d.total, r.count);
    assert.ok(d.rows.every((x, i) => i === 0 || d.rows[i - 1]!.odds >= x.odds));
  }
  const gun = items.rows.find((r) => r.group === 'FPS weapons')!;
  const d = mockLootDetail(data, true, 'items', gun.id);
  assert.ok(d.record?.guid);
  assert.equal(mockLootDetail(data, true, 'items', 'nothing').entry, null);
  assert.equal(mockLootDetail(data, false, 'items', gun.id).state, 'missing');
  // Answers are copies: editing one does not change the next.
  d.rows.length = 0;
  assert.ok(mockLootDetail(data, true, 'items', gun.id).rows.length > 0);
});
