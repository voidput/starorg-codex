// Datasets and Diff group trees (addendum 7.15): items, keyboard model, remembered group-by.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ALL, NONE, collapse, dataItemName, dataItems, dataKeyAction, keys, levelsOf, pickDiffBy, pickGroupBy } from '../src/lib/datatree.ts';
import type { DataGroup, GroupPreset } from '../src/lib/contract/dev.ts';

const n = (id: string, label: string, depth: number, parent: string | null, children: number, count: number): DataGroup => ({ id, parent, label, depth, count, children, more: 0, diff: null });
const gun = '["WeaponGun"]';
const gunGun = '["WeaponGun","Gun"]';
const nodes = [n(gun, 'WeaponGun', 1, null, 2, 8), n(gunGun, 'Gun', 2, gun, 2, 5), n('["WeaponGun","Rocket"]', 'Rocket', 2, gun, 1, 3), n('["WeaponMining"]', 'WeaponMining', 1, null, 1, 0)];

test('items: All first, then the nodes as sent, open ones expanded', () => {
  const items = dataItems(nodes, new Set([gun]), 10, 'All rows');
  assert.deepEqual(
    items.map((x) => [x.label, x.depth, x.expanded, x.count]),
    [
      ['All rows', 1, false, 10],
      ['WeaponGun', 1, true, 8],
      ['Gun', 2, false, 5],
      ['Rocket', 2, false, 3],
      ['WeaponMining', 1, false, 0],
    ],
  );
  assert.deepEqual(dataItems(null as unknown as DataGroup[], new Set(), 0).length, 1, 'guards a missing list');
});

test('keys move, open, close, select and clear', () => {
  const items = dataItems(nodes, new Set([gun]), 10);
  assert.deepEqual(dataKeyAction(items, ALL, 'ArrowDown'), { kind: 'focus', id: gun });
  assert.deepEqual(dataKeyAction(items, gun, 'ArrowRight'), { kind: 'focus', id: gunGun }, 'open: to the first child');
  assert.deepEqual(dataKeyAction(items, gunGun, 'ArrowRight'), { kind: 'expand', id: gunGun });
  assert.deepEqual(dataKeyAction(items, gunGun, 'ArrowLeft'), { kind: 'focus', id: gun }, 'closed child: to the parent');
  assert.deepEqual(dataKeyAction(items, gun, 'ArrowLeft'), { kind: 'collapse', id: gun });
  assert.deepEqual(dataKeyAction(items, gun, 'Enter'), { kind: 'select', id: gun });
  assert.deepEqual(dataKeyAction(items, ALL, ' '), { kind: 'clear' });
  assert.deepEqual(dataKeyAction(items, gun, 'Escape'), { kind: 'clear' });
  assert.deepEqual(dataKeyAction(items, gun, 'End'), { kind: 'focus', id: '["WeaponMining"]' });
  assert.deepEqual(dataKeyAction(items, gun, 'r', 'r'), { kind: 'focus', id: '["WeaponGun","Rocket"]' }, 'typeahead');
  assert.deepEqual(dataKeyAction(items, '["WeaponMining"]', 'ArrowRight'), { kind: 'expand', id: '["WeaponMining"]' });
});

test('collapsing closes open descendants', () => {
  assert.deepEqual(collapse([gun, gunGun, '["WeaponMining"]'], gun), ['["WeaponMining"]']);
  assert.deepEqual(keys('["a",null]'), ['a', null]);
  assert.equal(keys('nope'), null);
  assert.equal(keys('[1]'), null);
});

test('accessible names carry the counts', () => {
  const [, x] = dataItems([{ ...nodes[0]!, diff: { added: 1, removed: 0, changed: 2 } }], new Set(), 9);
  assert.equal(dataItemName(x!, 'records'), 'WeaponGun, 8 records, 1 added, 2 changed');
  assert.equal(dataItemName({ ...x!, count: 1 }, 'records'), 'WeaponGun, 1 record, 1 added, 2 changed');
  assert.equal(dataItemName({ ...x!, count: 1, diff: null }), 'WeaponGun, 1 row');
});

test('the remembered group-by falls back to the default when the table no longer offers it', () => {
  const presets: GroupPreset[] = [
    { id: 'chain', label: 'type › subType', levels: [{ column: 'type', folders: false }, { column: 'subType', folders: false }] },
    { id: 'folders:path', label: 'path folders', levels: [{ column: 'path', folders: true }] },
  ];
  assert.equal(pickGroupBy(null, presets), 'chain');
  assert.equal(pickGroupBy('folders:path', presets), 'folders:path');
  assert.equal(pickGroupBy('col:gone', presets), 'chain');
  assert.equal(pickGroupBy(NONE, presets), NONE);
  assert.equal(pickGroupBy(null, []), NONE);
  assert.deepEqual(levelsOf('folders:path', presets), [{ column: 'path', folders: true }]);
  assert.deepEqual(levelsOf(NONE, presets), []);
  assert.equal(pickDiffBy('folders'), 'folders');
  assert.equal(pickDiffBy('junk'), 'auto');
});
