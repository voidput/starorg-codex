// Unit tests for the Developer mode JSON tree, explorer tree and history (addendum 7.4, 7.8).
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ancestors, at, copyText, initialOpen, jsonKey, searchJson, splice, visibleLines, type Json } from '../src/lib/jsontree.ts';
import { History, HISTORY, treeKey, treeRows, typeahead, type Loaded } from '../src/lib/devnav.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const record: Json = JSON.parse(samples.dev_record.json);

test('the first two levels start open and only open nodes show their children', () => {
  const open = initialOpen(record);
  assert.ok(open.has('') && open.has('/_RecordValue_'));
  assert.ok(!open.has('/_RecordValue_/Speed'));
  const lines = visibleLines(record, open);
  assert.equal(lines[0]!.pointer, '');
  assert.ok(lines.some((l) => l.pointer === '/_RecordValue_/Speed' && l.kind === 'object' && !l.open));
  assert.ok(!lines.some((l) => l.pointer.startsWith('/_RecordValue_/Speed/')));
  // References are links, not containers.
  const ref = visibleLines(record, new Set([...open, '/_RecordValue_/Components'])).find((l) => l.kind === 'ref');
  assert.equal(ref?.ref?.type, 'SynComponent');
  assert.equal(ref?.pointer, '/_RecordValue_/Components/0');
});

test('tree keys move, open, close, open siblings and follow references', () => {
  let open = initialOpen(record);
  let lines = visibleLines(record, open);
  assert.deepEqual(jsonKey(lines, '', 'ArrowDown'), { kind: 'focus', pointer: '/_RecordName_' });
  assert.deepEqual(jsonKey(lines, '/_RecordValue_/Speed', 'ArrowRight'), { kind: 'open', pointer: '/_RecordValue_/Speed' });
  open.add('/_RecordValue_/Speed');
  lines = visibleLines(record, open);
  assert.deepEqual(jsonKey(lines, '/_RecordValue_/Speed', 'ArrowRight'), { kind: 'focus', pointer: '/_RecordValue_/Speed/Scm' });
  assert.deepEqual(jsonKey(lines, '/_RecordValue_/Speed/Scm', 'ArrowLeft'), { kind: 'focus', pointer: '/_RecordValue_/Speed' });
  assert.deepEqual(jsonKey(lines, '/_RecordValue_/Speed', 'ArrowLeft'), { kind: 'close', pointer: '/_RecordValue_/Speed' });
  const end = jsonKey(lines, '', 'End');
  assert.equal(end?.kind, 'focus');
  open.add('/_RecordValue_/Components');
  lines = visibleLines(record, open);
  const follow = jsonKey(lines, '/_RecordValue_/Components/0', 'Enter');
  assert.equal(follow?.kind, 'follow');
  open = initialOpen(record);
  lines = visibleLines(record, open);
  const star = jsonKey(lines, '/_RecordValue_/Speed', '*');
  assert.equal(star?.kind, 'open-siblings');
});

test('a cut node loads on expand and is spliced in place', () => {
  const cut: Json = JSON.parse(samples.dev_record_cut.json);
  const lines = visibleLines(cut, initialOpen(cut, 3));
  const stub = lines.find((l) => l.kind === 'cut');
  assert.ok(stub, 'the huge record has cut nodes');
  assert.deepEqual(jsonKey(lines, stub!.pointer, 'ArrowRight'), { kind: 'load', pointer: stub!.pointer });
  const merged = splice(cut, stub!.pointer, [{ Port: 'x' }]);
  assert.deepEqual(at(merged, stub!.pointer), [{ Port: 'x' }]);
  assert.notDeepEqual(at(cut, stub!.pointer), [{ Port: 'x' }], 'splice copies');
});

test('search finds keys and values; ancestors open the path to a hit', () => {
  const hits = searchJson(record, 'boost');
  assert.deepEqual(hits, ['/_RecordValue_/Speed/Boost']);
  assert.deepEqual(ancestors('/_RecordValue_/Speed/Boost'), ['', '/_RecordValue_', '/_RecordValue_/Speed']);
  assert.ok(searchJson(record, 'synship').length >= 1);
  assert.equal(copyText('a'), 'a');
  assert.equal(copyText({ a: 1 }), '{"a":1}');
});

test('history keeps 50 entries and drops forward on a new visit', () => {
  const h = new History();
  for (let i = 0; i < 60; i++) h.visit(`g${i}`);
  assert.equal(h.back.length, HISTORY - 1);
  assert.equal(h.goBack(), 'g58');
  assert.equal(h.goBack(), 'g57');
  assert.equal(h.goForward(), 'g58');
  h.visit('x');
  assert.equal(h.canForward, false);
  assert.equal(h.goBack(), 'g58');
});

test('explorer tree rows, keys, "show more" and typeahead', () => {
  const loaded = new Map<string, Loaded>([
    ['', { nodes: samples.dev_tree.nodes, total: samples.dev_tree.total }],
    ['t:SynComponent', { nodes: samples.dev_tree_type.nodes, total: samples.dev_tree_type.total }],
  ]);
  let rows = treeRows(loaded, new Set());
  assert.deepEqual(rows.map((r) => r.id), ['t:SynComponent', 't:SynShip', 't:SynTag']);
  assert.deepEqual(treeKey(rows, 't:SynComponent', 'ArrowRight'), { kind: 'open', id: 't:SynComponent' });
  rows = treeRows(loaded, new Set(['t:SynComponent']));
  assert.equal(rows[1]!.parent, 't:SynComponent');
  assert.equal(rows[1]!.level, 2);
  const more = rows.find((r) => r.id === 'more:t:SynComponent');
  assert.equal(more?.more, 195);
  assert.deepEqual(treeKey(rows, more!.id, 'Enter'), { kind: 'more', parent: 't:SynComponent' });
  assert.deepEqual(treeKey(rows, rows[1]!.id, 'Enter'), { kind: 'activate', id: rows[1]!.id });
  assert.deepEqual(treeKey(rows, rows[1]!.id, 'ArrowLeft'), { kind: 'focus', id: 't:SynComponent' });
  assert.deepEqual(treeKey(rows, 't:SynComponent', 'ArrowLeft'), { kind: 'close', id: 't:SynComponent' });
  assert.equal(typeahead(rows, 't:SynComponent', 'synt'), 't:SynTag');
});
