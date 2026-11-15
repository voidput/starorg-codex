// Unit tests for the Strings group tree (addendum 8.5). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ALL, applySelection, chips, isSelected, itemName, keyAction, parsePrefs, visibleItems } from '../src/lib/grouptree.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const nodes = samples.query_strings.groups;

test('collapsed tree shows All plus the top-level groups only', () => {
  const items = visibleItems(nodes, new Set(), 100);
  assert.equal(items[0].id, ALL);
  assert.ok(items.slice(1).every((x) => x.level === 1 && x.parent === null));
  assert.equal(items.length, 1 + nodes.filter((n) => !n.parent).length);
  // Zero-count groups stay (the UI dims them).
  assert.ok(items.some((x) => x.id === 'blueprints' && x.count === 0));
});

test('expanding shows children under their parent', () => {
  const items = visibleItems(nodes, new Set(['components']), 100);
  const i = items.findIndex((x) => x.id === 'components');
  assert.equal(items[i].expanded, true);
  assert.equal(items[i + 1].parent, 'components');
  assert.equal(items[i + 1].level, 2);
});

test('arrow keys, Home/End, Right/Left follow the tree pattern', () => {
  let open = new Set<string>();
  let items = visibleItems(nodes, open, 100);
  assert.deepEqual(keyAction(items, ALL, 'ArrowDown', false), { kind: 'focus', id: 'ships' });
  assert.deepEqual(keyAction(items, ALL, 'ArrowUp', false), { kind: 'focus', id: ALL });
  assert.deepEqual(keyAction(items, 'ships', 'End', false), { kind: 'focus', id: 'other' });
  assert.deepEqual(keyAction(items, 'other', 'Home', false), { kind: 'focus', id: ALL });
  assert.deepEqual(keyAction(items, 'components', 'ArrowRight', false), { kind: 'expand', id: 'components' });
  open = new Set(['components']);
  items = visibleItems(nodes, open, 100);
  assert.deepEqual(keyAction(items, 'components', 'ArrowRight', false), { kind: 'focus', id: 'components/coolers' });
  assert.deepEqual(keyAction(items, 'components/coolers', 'ArrowLeft', false), { kind: 'focus', id: 'components' });
  assert.deepEqual(keyAction(items, 'components', 'ArrowLeft', false), { kind: 'collapse', id: 'components' });
  // A leaf without children: Right does nothing.
  assert.deepEqual(keyAction(items, 'missiles', 'ArrowRight', false), { kind: 'none' });
});

test('typeahead jumps to the next label with the typed prefix', () => {
  const items = visibleItems(nodes, new Set(), 100);
  assert.deepEqual(keyAction(items, ALL, 'm', false, 'm'), { kind: 'focus', id: 'missiles' });
  assert.deepEqual(keyAction(items, 'missiles', 'm', false, 'm'), { kind: 'focus', id: 'missions' });
  assert.deepEqual(keyAction(items, 'missiles', 'a', false, 'ma'), { kind: 'focus', id: 'manufacturers' });
});

test('Enter selects only this group, Ctrl+Enter toggles, Escape clears', () => {
  const items = visibleItems(nodes, new Set(), 100);
  let sel: string[] = [];
  sel = applySelection(sel, keyAction(items, 'ships', 'Enter', false));
  assert.deepEqual(sel, ['ships']);
  sel = applySelection(sel, keyAction(items, 'armor', 'Enter', true));
  assert.deepEqual(sel, ['ships', 'armor']);
  sel = applySelection(sel, keyAction(items, 'ships', ' ', true));
  assert.deepEqual(sel, ['armor']);
  // Enter on the only selected group deselects it.
  assert.deepEqual(applySelection(sel, keyAction(items, 'armor', 'Enter', false)), []);
  assert.deepEqual(applySelection(['ships'], keyAction(items, 'ships', 'Escape', false)), []);
  assert.deepEqual(applySelection(['ships'], keyAction(items, ALL, 'Enter', false)), []);
});

test('a selected parent marks its children; All is selected when nothing is', () => {
  assert.ok(isSelected({ id: 'components/coolers', parent: 'components' }, ['components']));
  assert.ok(!isSelected({ id: 'components/coolers', parent: 'components' }, ['ships']));
  assert.ok(isSelected({ id: ALL, parent: null }, []));
});

test('accessible names carry the counts', () => {
  assert.equal(itemName({ id: 'x', label: 'Coolers', level: 2, parent: 'c', hasChildren: false, expanded: false, count: 1412, enhanced: 96, edited: 0 }), 'Coolers, 1,412 keys, 96 enhanced');
});

test('stored prefs are validated', () => {
  assert.deepEqual(parsePrefs('garbage'), { groupBy: 'type', groups: [], manufacturers: [], sizes: [], open: [] });
  const p = parsePrefs(JSON.stringify({ groupBy: 'size', groups: ['ships', 3], sizes: [2, 40, -1, 0], open: ['ships'] }));
  assert.deepEqual(p, { groupBy: 'size', groups: ['ships'], manufacturers: [], sizes: [2, 0], open: ['ships'] });
  assert.equal(parsePrefs(JSON.stringify({ groupBy: 'weird' })).groupBy, 'type');
});

test('chips list every selection with known labels', () => {
  const known = new Map([['groups:ships', 'Ships & vehicles'], ['manufacturers:SYNA', 'Alpha Synthetics']]);
  const c = chips({ groupBy: 'type', groups: ['ships'], manufacturers: ['SYNA', 'none'], sizes: [0, 3], open: [] }, known);
  assert.deepEqual(c.map((x) => x.label), ['Ships & vehicles', 'Alpha Synthetics', 'No manufacturer', 'No size', 'Size 3']);
});

test('fallback labels and compact badge counts', async () => {
  const { idLabel, compact } = await import('../src/lib/grouptree.ts');
  assert.equal(idLabel('components/power-plants'), 'Power plants');
  assert.equal(idLabel('missiles'), 'Missiles');
  assert.equal(compact(999), '999');
  assert.equal(compact(1251), '1.3k');
  assert.equal(compact(1000), '1k');
  assert.equal(compact(33498), '33k');
});
