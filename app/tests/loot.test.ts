// The Loot screen's pure helpers (odds text, search and group filter, drop labels) and the TS
// mock's answers, on the Rust mock's data (mock-loot.gen.json). The trees: loottree.test.ts.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dropWhere, filterSummaries, groupsOf, listed, oddsText, queryWords } from '../src/lib/loot.ts';
import { mockLootDetail, mockLootList, type LootMockData } from '../src/lib/contract/mock-loot.ts';
import type { LootSummary } from '../src/lib/contract/types.ts';

const data = JSON.parse(readFileSync(new URL('../src/lib/contract/mock-loot.gen.json', import.meta.url), 'utf8')) as LootMockData;

const row = (id: string, name: string, group: string, path: string[] = [group], best = 0.1, count = 1): LootSummary => ({ id, name, group, path, best, count });

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

test('groups sort by name with Other last', () => {
  const rows = [row('a', 'A', 'Caves'), row('b', 'B', 'Caves'), row('c', 'C', 'Other'), row('d', 'D', 'Asteroid bases')];
  assert.deepEqual(groupsOf(rows), [
    { group: 'Asteroid bases', count: 1 },
    { group: 'Caves', count: 2 },
    { group: 'Other', count: 1 },
  ]);
});

test('search matches the tree path too', () => {
  const rows = [row('caves/loot_caves_occupied_rock', 'Rock', 'Caves', ['Caves', 'Occupied']), row('cave_aberdeen_rich', 'Rich', 'Caves', ['Caves', 'Aberdeen'])];
  assert.deepEqual(
    filterSummaries(rows, 'occupied', '').map((r) => r.name),
    ['Rock'],
  );
  assert.deepEqual(
    filterSummaries(rows, 'aberdeen rich', 'Caves').map((r) => r.id),
    ['cave_aberdeen_rich'],
  );
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

test('the TS mock serves the Rust mock: missing until read, then lists and grouped, likeliest-first details', () => {
  assert.deepEqual(mockLootList(data, false, 'areas'), { state: 'missing', view: 'areas', build: null, rows: [] });
  const areas = mockLootList(data, true, 'areas');
  const items = mockLootList(data, true, 'items');
  assert.equal(areas.state, 'ready');
  assert.ok(areas.rows.length >= 10 && items.rows.length >= 40);
  // Summaries only (with their tree path): no drops in a list row.
  assert.deepEqual(Object.keys(areas.rows[0]!).sort(), ['best', 'count', 'group', 'id', 'name', 'path']);
  const sorted = (rows: { odds: number }[]) => rows.every((x, i) => i === 0 || rows[i - 1]!.odds >= x.odds);
  for (const r of areas.rows) {
    const d = mockLootDetail(data, true, 'areas', r.id.toUpperCase());
    assert.equal(d.entry?.id, r.id);
    assert.equal(d.group_by, 'container');
    assert.equal(d.total, r.count);
    // Headers cover every drop, best first; open groups hold their container's rows.
    assert.equal(
      d.groups.reduce((n, g) => n + g.count, 0),
      r.count,
    );
    assert.ok(d.groups.every((g, i) => i === 0 || d.groups[i - 1]!.best >= g.best));
    assert.ok(Array.isArray(d.groups[0]!.rows));
    for (const g of d.groups) if (g.rows) assert.ok(sorted(g.rows) && g.rows.every((x) => x.container === g.label));
    // Ungrouped: one group of every drop.
    const flat = mockLootDetail(data, true, 'areas', r.id, null, 'none');
    assert.equal(flat.groups.length, 1);
    assert.equal(flat.groups[0]!.rows!.length, r.count);
    assert.ok(sorted(flat.groups[0]!.rows!));
  }
  const gun = items.rows.find((r) => r.group === 'FPS weapons')!;
  const d = mockLootDetail(data, true, 'items', gun.id);
  assert.ok(d.record?.guid);
  assert.equal(d.group_by, 'place');
  assert.ok(d.groups.every((g) => !g.rows || g.rows.every((x) => x.group.split(' › ')[0] === g.label)));
  // Asking for one group answers its rows only.
  const last = d.groups.at(-1)!;
  const one = mockLootDetail(data, true, 'items', gun.id, null, null, [last.key]);
  assert.ok(one.groups.every((g) => (g.key === last.key) === Array.isArray(g.rows)));
  assert.equal(mockLootDetail(data, true, 'items', 'nothing').entry, null);
  assert.equal(mockLootDetail(data, false, 'items', gun.id).state, 'missing');
  // The filter: every word, `all` still counts every drop; the matching groups open.
  const f = mockLootDetail(data, true, 'items', gun.id, '  CAVES ', 'none');
  const fr = f.groups[0]!.rows!;
  assert.ok(fr.length > 0 && fr.every((r) => `${r.name} ${r.group} ${r.id}`.toLowerCase().includes('caves')));
  assert.equal(f.total, fr.length);
  assert.equal(f.all, d.all);
  const fg = mockLootDetail(data, true, 'items', gun.id, 'caves');
  assert.ok(fg.groups.length > 0 && fg.groups.every((g) => Array.isArray(g.rows)));
  assert.equal(mockLootDetail(data, true, 'items', gun.id, 'zzz').groups.length, 0);
  assert.equal(mockLootDetail(data, true, 'items', gun.id, 'zzz').entry?.id, gun.id);
  // Answers are copies: editing one does not change the next.
  d.groups.length = 0;
  assert.ok(mockLootDetail(data, true, 'items', gun.id).groups.length > 0);
});
