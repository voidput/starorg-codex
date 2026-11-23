// The Loot screen's grouped drops: the Group by choices and their storage, the rows the
// windowed table draws, same-name detection, and the grouping the mock does as Rust does.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  ALL_GROUP,
  MAX_DROPS,
  MAX_GROUP_ROWS,
  defaultGroupBy,
  defaultOpen,
  dropRows,
  dupNames,
  groupByKey,
  groupByOptions,
  groupDrops,
  groupKeyOf,
  loadGroupBy,
  loadedRows,
  missingGroups,
  parseGroupBy,
  saveGroupBy,
} from '../src/lib/lootgroups.ts';
import type { LootDrop, LootGroup } from '../src/lib/contract/types.ts';

const drop = (id: string, name: string, container: string, odds: number, group = 'Caves › Occupied'): LootDrop => ({ id, name, group, odds, container, rarity: null, kind: 'loot' });

test('Group by: areas offer where-in-it (default) and none; items place (default), where-in-it and none', () => {
  assert.deepEqual(
    groupByOptions('areas').map((o) => o.id),
    ['container', 'none'],
  );
  assert.deepEqual(
    groupByOptions('items').map((o) => o.id),
    ['place', 'container', 'none'],
  );
  assert.equal(groupByOptions('areas')[0]!.label, 'Where in it');
  assert.equal(defaultGroupBy('areas'), 'container');
  assert.equal(defaultGroupBy('items'), 'place');
  assert.equal(parseGroupBy('items', 'container'), 'container');
  // Place is not offered for areas; junk and nothing fall back to the default.
  assert.equal(parseGroupBy('areas', 'place'), 'container');
  assert.equal(parseGroupBy('items', 'bogus'), 'place');
  assert.equal(parseGroupBy('areas', null), 'container');
});

test('the choice is remembered per view, and blocked storage never throws', () => {
  const m = new Map<string, string>();
  const store = { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
  saveGroupBy('items', 'none', store);
  assert.equal(m.get(groupByKey('items')), 'none');
  assert.equal(loadGroupBy('items', store), 'none');
  assert.equal(loadGroupBy('areas', store), 'container');
  assert.notEqual(groupByKey('areas'), groupByKey('items'));
  const blocked = {
    getItem: (): string | null => {
      throw new Error('blocked');
    },
    setItem: (): void => {
      throw new Error('blocked');
    },
  };
  assert.equal(loadGroupBy('items', blocked), 'place');
  assert.doesNotThrow(() => saveGroupBy('items', 'none', blocked));
  assert.equal(loadGroupBy('areas', null), 'container');
});

test('same names are found across the rows', () => {
  const rows = [drop('a1', 'Standard Container', 'Large crate', 0.5), drop('a2', 'Standard Container', 'Medium crate', 0.4), drop('b', 'Davlos Shirt', 'Large crate', 0.3)];
  assert.deepEqual([...dupNames(rows)], ['Standard Container']);
  assert.equal(dupNames([]).size, 0);
});

test('groups: by container, place or one group; best first then by name; default and asked-for rows; caps', () => {
  const rows = [
    drop('r1', 'Rifle', 'Weapons rack', 1, 'FPS weapons › Rifles'),
    drop('l1', 'Ration', 'Lying loose', 1, 'Consumables'),
    drop('c1', 'Pistol', 'Large crate', 0.4, 'FPS weapons › Pistols'),
    drop('c2', 'Helmet', 'Large crate', 0.2, 'Armour & clothing › Helmets'),
  ];
  const g = groupDrops(rows, 'container', null, false);
  // Equal best: by name, so Lying loose before Weapons rack.
  assert.deepEqual(
    g.map((x) => [x.label, x.count, x.best]),
    [
      ['Lying loose', 1, 1],
      ['Weapons rack', 1, 1],
      ['Large crate', 2, 0.4],
    ],
  );
  // All small: every group opens.
  assert.ok(g.every((x) => Array.isArray(x.rows)));
  const p = groupDrops(rows, 'place', ['FPS weapons'], false);
  assert.deepEqual(
    p.map((x) => x.key),
    ['Consumables', 'FPS weapons', 'Armour & clothing'],
  );
  assert.deepEqual(
    p.map((x) => x.rows?.map((r) => r.id) ?? null),
    [null, ['r1', 'c1'], null],
  );
  assert.equal(groupKeyOf(rows[0]!, 'place'), 'FPS weapons');
  const flat = groupDrops(rows, 'none', null, false);
  assert.deepEqual(
    flat.map((x) => [x.key, x.count]),
    [[ALL_GROUP, 4]],
  );
  // Big groups: only the first opens, its rows capped; the header still counts every drop.
  const many = Array.from({ length: MAX_DROPS + 10 }, (_, i) => drop(`g${i}`, `Gun ${i}`, i % 2 ? 'Large crate' : 'Lying loose', 1 - i / 1e5));
  const big = groupDrops(many, 'container', null, false);
  assert.deepEqual(
    big.map((x) => x.count),
    [(MAX_DROPS + 10) / 2, (MAX_DROPS + 10) / 2],
  );
  assert.equal(big[0]!.rows!.length, MAX_GROUP_ROWS);
  assert.equal(big[1]!.rows, null);
  assert.equal(groupDrops(many, 'none', null, false)[0]!.rows!.length, MAX_DROPS);
});

test('default open: the first, a few small ones, or a filter’s matches up to the budget', () => {
  assert.deepEqual(defaultOpen([3, 5, 30, 5], 500, false), [true, true, true, false]);
  assert.deepEqual(defaultOpen([900, 1], 500, false), [true, false]);
  assert.deepEqual(defaultOpen([900, 900, 900, 900, 900], 500, true), [true, true, true, true, false]);
});

test('the drawn rows: headers, open groups’ rows, loading and "more" rows; ungrouped draws no header', () => {
  const head = (key: string, count: number, rows: LootDrop[] | null): LootGroup => ({ key, label: key, count, best: rows?.[0]?.odds ?? 0.1, rows });
  const a = [drop('x', 'X', 'Rack', 0.9), drop('y', 'Y', 'Rack', 0.8)];
  const groups = [head('Rack', 5, a), head('Loose', 3, null), head('Crate', 1, null)];
  const loaded = loadedRows(groups);
  assert.deepEqual(Object.keys(loaded), ['Rack']);
  const open = new Set(['Rack', 'Loose']);
  const rows = dropRows(groups, open, loaded, false);
  assert.deepEqual(
    rows.map((r) => r.kind),
    ['head', 'drop', 'drop', 'more', 'head', 'loading', 'head'],
  );
  const more = rows[3]!;
  assert.ok(more.kind === 'more' && more.group.count - more.shown === 3);
  assert.ok(rows[0]!.kind === 'head' && rows[0]!.open && rows[6]!.kind === 'head' && !rows[6]!.open);
  assert.equal(new Set(rows.map((r) => r.id)).size, rows.length);
  // What to load next: open groups without rows, not loading already.
  assert.deepEqual(missingGroups(open, loaded, new Set()), ['Loose']);
  assert.deepEqual(missingGroups(open, loaded, new Set(['Loose'])), []);
  // Ungrouped: the rows only.
  const flat = dropRows([head(ALL_GROUP, 9, a)], new Set(), loadedRows([head(ALL_GROUP, 9, a)]), true);
  assert.deepEqual(
    flat.map((r) => r.kind),
    ['drop', 'drop'],
  );
  assert.deepEqual(dropRows(null, open, {}, false), []);
});
