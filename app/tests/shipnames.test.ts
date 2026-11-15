// Ship-name rules and list edits (addendum §6.6, 6.9). The prefix and sort rules are checked
// against the vectors the Rust generator test writes (fixtures/ship-names-vectors.json).
// Run: npm test (node --test, type stripping).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  addGroup,
  addShip,
  announce,
  compareNames,
  deleteGroup,
  editValue,
  moveBy,
  moveGroup,
  moveToBlock,
  moveToEnd,
  nameProblem,
  ordered,
  placeIn,
  plan,
  preview,
  removeShip,
  rename,
} from '../src/lib/shipnames.ts';

const vectors = JSON.parse(readFileSync(new URL('../../fixtures/ship-names-vectors.json', import.meta.url), 'utf8'));

test('prefixes and preview order match the Rust vectors', () => {
  assert.ok(vectors.cases.length >= 5);
  for (const c of vectors.cases) {
    const p = plan(c.settings, vectors.catalog);
    const byId = new Map(vectors.catalog.map((h: { hull: string }) => [h.hull, h]));
    const edits = p.edits.map((e) => {
      const h = byId.get(e.hull) as { name: string; short: string };
      return { key: e.key, value: editValue(e, e.short ? h.short : h.name) };
    });
    assert.deepEqual(edits, c.expected.edits, c.name);
    assert.deepEqual(p.notInBuild, c.expected.not_in_build, c.name);
    assert.deepEqual(
      preview(c.settings, vectors.catalog).map((r) => r.value),
      c.expected.preview,
      c.name,
    );
  }
});

test('sort is case-insensitive and character by character', () => {
  const names = ['b', 'A', '10 x', '2 x', '02 x', '* y', 'a'];
  assert.deepEqual([...names].sort(compareNames), ['* y', '02 x', '10 x', '2 x', 'A', 'a', 'b']);
});

test('rename rules match the core', () => {
  for (const bad of ['', '   ', '@x', 'a<b', '50%', 'x~y', 'a\\n', 'x'.repeat(49), 'a\u0007']) assert.ok(nameProblem(bad), JSON.stringify(bad));
  assert.equal(nameProblem('  Needle  '), null);
  assert.equal(nameProblem('é'.repeat(48)), null);
});

type S = Parameters<typeof plan>[0];
const base = (): S => ({
  enabled: true,
  style: 'numeric',
  separator: ' ',
  symbol: '*',
  rest: 'stock',
  prefix_short: false,
  groups: [
    { id: 'combat', name: 'Combat' },
    { id: 'cargo', name: 'Cargo' },
  ],
  ships: [
    { hull: 'a', group: null, name: null, short: null },
    { hull: 'b', group: null, name: null, short: null },
    { hull: 'c', group: 'combat', name: null, short: null },
    { hull: 'd', group: 'combat', name: null, short: null },
    { hull: 'e', group: 'cargo', name: null, short: null },
  ],
});
const order = (s: S) => ordered(s).map((p) => `${p.entry.hull}${p.group ? ':' + p.group.id : ''}`).join(' ');

test('Alt+Up/Down moves one place and crosses group borders', () => {
  let s = base();
  assert.equal(order(s), 'a b c:combat d:combat e:cargo');
  s = moveBy(s, 'b', -1);
  assert.equal(order(s), 'b a c:combat d:combat e:cargo');
  s = moveBy(s, 'a', 1);
  assert.equal(order(s), 'b a:combat c:combat d:combat e:cargo', 'joins Combat at its top');
  s = moveBy(s, 'a', -1);
  assert.equal(order(s), 'b a c:combat d:combat e:cargo', 'and back');
  s = moveBy(s, 'e', 1);
  assert.equal(order(s), 'b a c:combat d:combat e:cargo', 'bottom stays');
  s = moveBy(s, 'b', -1);
  assert.equal(order(s), 'b a c:combat d:combat e:cargo', 'top stays');
  s = moveBy(s, 'e', -1);
  assert.equal(order(s), 'b a c:combat d:combat e:combat', 'joins Combat at its end');
  s = moveBy(s, 'e', 1);
  assert.equal(order(s), 'b a c:combat d:combat e:cargo', 'steps into the empty Cargo block');
});

test('Alt+Home/End and Alt+Shift+Up/Down', () => {
  let s = moveToEnd(base(), 'd', 'top');
  assert.equal(order(s), 'd a b c:combat e:cargo');
  s = moveToEnd(s, 'a', 'bottom');
  assert.equal(order(s), 'd b c:combat e:cargo a:cargo');
  s = moveToBlock(base(), 'a', 1);
  assert.equal(order(s), 'b a:combat c:combat d:combat e:cargo', 'next group: first in it');
  s = moveToBlock(s, 'e', -1);
  assert.equal(order(s), 'b a:combat c:combat d:combat e:combat', 'previous group: last in it');
  assert.equal(order(moveToBlock(base(), 'a', -1)), order(base()), 'no block before ungrouped');
  s = placeIn(base(), 'a', 'cargo', 'first');
  assert.equal(order(s), 'b c:combat d:combat a:cargo e:cargo', 'drop on a group header');
  s = placeIn(base(), 'e', null, { before: 'b' });
  assert.equal(order(s), 'a e b c:combat d:combat');
});

test('add, remove, rename, groups', () => {
  let s = addShip(base(), 'f');
  assert.equal(order(s), 'a b f c:combat d:combat e:cargo', 'added ships go to the end of the ungrouped block');
  assert.equal(addShip(s, 'F'), s, 'no duplicates (folded ids)');
  s = removeShip(s, 'c');
  assert.equal(order(s), 'a b f d:combat e:cargo');
  s = rename(s, 'a', '  Needle ');
  assert.equal(s.ships[0]!.name, 'Needle');
  s = rename(s, 'a', '');
  assert.equal(s.ships[0]!.name, null, 'empty = reset to stock');
  let id: string;
  [s, id] = addGroup(s, 'Combat');
  assert.equal(id, 'combat-2');
  s = moveGroup(s, 'cargo', -1);
  assert.deepEqual(s.groups.map((g) => g.id), ['cargo', 'combat', 'combat-2']);
  assert.equal(order(s), 'a b f e:cargo d:combat', 'a group moves as a block');
  s = deleteGroup(s, 'cargo');
  assert.equal(order(s), 'a b f e d:combat', 'its ships become ungrouped');
  assert.equal(announce(s, 'd', 'Delta'), 'Delta, position 5 of 5, group Combat');
  assert.equal(announce(s, 'a', 'Alpha'), 'Alpha, position 1 of 5, no group');
});

test('the preview uses overrides and marks listed ships', () => {
  const cat = [
    { hull: 'a', name: 'Zed', short: null },
    { hull: 'b', name: 'Alpha', short: null },
    { hull: 'x', name: 'Beta', short: null },
  ];
  const s = { ...base(), groups: [], ships: [{ hull: 'a', group: null, name: null, short: null }, { hull: 'b', group: null, name: null, short: null }] };
  const rows = preview(s, cat, [{ hull: 'b', long: 'My Alpha', short: null }]);
  assert.deepEqual(rows.map((r) => r.value), ['01 Zed', 'Beta', 'My Alpha']);
  assert.equal(rows[2]!.overridden, true);
  assert.equal(rows[1]!.listed, false);
});
