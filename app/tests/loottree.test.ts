// The Loot screen's trees: built from the rows' paths, flattened for the windowed list, the
// search (auto-expands), the category filter, the stored expanded state and keyboard moves
// (keyAction of grouptree.ts), on hand-made rows and the Rust mock's data. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ancestorsOf, branchId, buildTree, flatten, leafId, openKey, parseOpen, searching, toggled, visibleTree, whereText, type LootNode } from '../src/lib/loottree.ts';
import { keyAction } from '../src/lib/grouptree.ts';
import type { LootSummary } from '../src/lib/contract/types.ts';
import type { LootMockData } from '../src/lib/contract/mock-loot.ts';

const data = JSON.parse(readFileSync(new URL('../src/lib/contract/mock-loot.gen.json', import.meta.url), 'utf8')) as LootMockData;

const row = (id: string, name: string, path: string[], best = 0.1): LootSummary => ({ id, name, group: path[0] ?? name, path, best, count: 1 });

const ROWS: LootSummary[] = [
  row('v3slotpreset_battaglia', 'Battaglia', [], 0.2),
  row('caves/loot_caves_occupied_rock', 'Rock', ['Caves', 'Occupied'], 0.3),
  row('caves/loot_caves_occupied', 'General', ['Caves', 'Occupied'], 0.1),
  row('lootgeneration_slotpreset_caves', 'General', ['Caves'], 0.05),
  row('cave_prison_harvestables', 'Prison harvestables', ['Caves'], 0.5),
  row('lootgeneration_slotpreset_generic', 'Generic (classic)', ['Other']),
  row('asddelving_multiconfig', 'General (multi)', ['ASD facilities']),
  row('v3slotpreset_asddelving', 'General (v3)', ['ASD facilities']),
  row('v3slotpreset_asddelving_sciencewing', 'Science wing', ['ASD facilities']),
];

const labels = (nodes: LootNode[]): string[] => nodes.map((n) => (n.kind === 'branch' ? `[${n.label}]` : n.label));

test('the tree: branches first by name, Other last, General rows first, counts and best odds', () => {
  const t = buildTree(ROWS);
  assert.deepEqual(labels(t), ['[ASD facilities]', '[Caves]', '[Other]', 'Battaglia']);
  const caves = t[1]!;
  assert.equal(caves.kind, 'branch');
  if (caves.kind !== 'branch') return;
  assert.deepEqual(labels(caves.children), ['[Occupied]', 'General', 'Prison harvestables']);
  assert.equal(caves.count, 4);
  assert.equal(caves.best, 0.5);
  assert.equal(caves.id, branchId(['Caves']));
  const occ = caves.children[0]!;
  assert.ok(occ.kind === 'branch' && occ.count === 2 && occ.best === 0.3);
  if (occ.kind === 'branch') assert.deepEqual(labels(occ.children), ['General', 'Rock']);
  const asd = t[0]!;
  if (asd.kind === 'branch') assert.deepEqual(labels(asd.children), ['General (multi)', 'General (v3)', 'Science wing']);
  assert.deepEqual(buildTree(null), []);
});

test('collapsed by default; expanded branches show their children with aria positions', () => {
  const t = buildTree(ROWS);
  const closed = flatten(t, new Set());
  assert.deepEqual(
    closed.map((r) => r.label),
    ['ASD facilities', 'Caves', 'Other', 'Battaglia'],
  );
  assert.ok(closed.every((r) => r.level === 1 && r.size === 4));
  assert.deepEqual(
    closed.map((r) => r.pos),
    [1, 2, 3, 4],
  );
  const open = flatten(t, new Set([branchId(['Caves']), branchId(['Caves', 'Occupied'])]));
  const ids = open.map((r) => `${r.level}:${r.label}`);
  assert.deepEqual(ids, ['1:ASD facilities', '1:Caves', '2:Occupied', '3:General', '3:Rock', '2:General', '2:Prison harvestables', '1:Other', '1:Battaglia']);
  const rock = open.find((r) => r.label === 'Rock')!;
  assert.equal(rock.parent, branchId(['Caves', 'Occupied']));
  assert.equal(rock.id, leafId('caves/loot_caves_occupied_rock'));
  // A child level opens only when its parent is open.
  assert.equal(flatten(t, new Set([branchId(['Caves', 'Occupied'])])).length, 4);
});

test('search filters the tree and opens every branch with a match; the filter keeps a category', () => {
  const s = visibleTree(ROWS, 'rock', '', new Set());
  assert.equal(s.matched, 1);
  assert.deepEqual(
    s.rows.map((r) => r.label),
    ['Caves', 'Occupied', 'Rock'],
  );
  assert.ok(s.rows.filter((r) => r.hasChildren).every((r) => r.expanded));
  // The path matches too: every occupied cave.
  assert.equal(visibleTree(ROWS, 'occupied', '', new Set()).matched, 2);
  // Closed during this search: stays closed while it lasts.
  const c = visibleTree(ROWS, 'caves', '', new Set(), new Set([branchId(['Caves', 'Occupied'])]));
  assert.ok(!c.rows.some((r) => r.label === 'Rock'));
  assert.ok(c.rows.some((r) => r.label === 'Occupied' && !r.expanded));
  // No search: the stored expanded state.
  const g = visibleTree(ROWS, '  ', 'Caves', new Set());
  assert.deepEqual(
    g.rows.map((r) => r.label),
    ['Caves'],
  );
  assert.equal(g.matched, 4);
  assert.ok(searching(' a ') && !searching('  '));
});

test('keyboard: down/up move, right opens then enters, left closes then goes to the parent', () => {
  let open = new Set<string>();
  let rows = flatten(buildTree(ROWS), open);
  const caves = branchId(['Caves']);
  assert.deepEqual(keyAction(rows, caves, 'ArrowDown', false), { kind: 'focus', id: branchId(['Other']) });
  assert.deepEqual(keyAction(rows, caves, 'ArrowUp', false), { kind: 'focus', id: branchId(['ASD facilities']) });
  assert.deepEqual(keyAction(rows, caves, 'ArrowRight', false), { kind: 'expand', id: caves });
  open = toggled(open, caves, true);
  rows = flatten(buildTree(ROWS), open);
  assert.deepEqual(keyAction(rows, caves, 'ArrowRight', false), { kind: 'focus', id: branchId(['Caves', 'Occupied']) });
  const general = leafId('lootgeneration_slotpreset_caves');
  assert.deepEqual(keyAction(rows, general, 'ArrowLeft', false), { kind: 'focus', id: caves });
  assert.deepEqual(keyAction(rows, caves, 'ArrowLeft', false), { kind: 'collapse', id: caves });
  // Enter on a row picks it (the screen opens or closes a branch instead).
  assert.deepEqual(keyAction(rows, general, 'Enter', false), { kind: 'select-only', id: general });
  assert.deepEqual(keyAction(rows, caves, 'End', false), { kind: 'focus', id: leafId('v3slotpreset_battaglia') });
  // Typeahead by label.
  assert.deepEqual(keyAction(rows, caves, 'p', false, 'p'), { kind: 'focus', id: leafId('cave_prison_harvestables') });
  assert.equal(toggled(open, caves, false).has(caves), false);
});

test('a row opens through its ancestors; where it sits reads as a line', () => {
  const r = ROWS[1]!;
  assert.deepEqual(ancestorsOf(r), [branchId(['Caves']), branchId(['Caves', 'Occupied'])]);
  assert.deepEqual(ancestorsOf(ROWS[0]!), []);
  assert.equal(whereText(r), 'Caves › Occupied');
  assert.equal(whereText(ROWS[0]!), 'Battaglia');
});

test('stored expanded state: per view, malformed values dropped', () => {
  assert.equal(openKey('areas'), 'starorg.loot.open.areas');
  assert.notEqual(openKey('areas'), openKey('items'));
  assert.deepEqual(parseOpen(JSON.stringify(['p:Caves', 'x', 3, 'p:Caves\u001fOccupied'])), ['p:Caves', 'p:Caves\u001fOccupied']);
  assert.deepEqual(parseOpen('{not json'), []);
  assert.deepEqual(parseOpen(null), []);
  assert.deepEqual(parseOpen('{"a":1}'), []);
});

test('the mock lists make trees with unique sibling names and every row reachable', () => {
  for (const list of [data.areas, data.items]) {
    const tree = buildTree(list.rows);
    const all = flatten(tree, 'all');
    const leaves = all.filter((r) => r.node.kind === 'leaf');
    assert.equal(leaves.length, list.rows.length);
    // Area names are unique among siblings.
    if (list.view === 'areas') {
      const seen = new Set<string>();
      for (const r of leaves) {
        const k = `${r.parent}|${r.label}`;
        assert.ok(!seen.has(k), k);
        seen.add(k);
      }
    }
    assert.ok(all.some((r) => r.level === 2));
  }
});
