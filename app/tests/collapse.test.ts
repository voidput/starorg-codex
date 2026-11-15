// Collapsible sections: the stored lists of collapsed cards and finder groups, the arrow keys,
// the finder's paging with collapsed groups, and the blueprint markers summary / preview order.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CARDS_KEY, GROUPS_KEY, arrowToggle, parseCollapsed, readCollapsed, withCollapsed, writeCollapsed } from '../src/lib/collapse.ts';
import { DEFAULT_PREFS, hiddenMatches, pageRows, query, type FinderPrefs } from '../src/lib/bpfinder.ts';
import { emWord, markerLine, markerSummary, previewOrder } from '../src/lib/bpmarkers.ts';
import type { BlueprintFinder, BlueprintRow, PreviewItem, Profile } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const sample: BlueprintFinder = samples.blueprint_finder;
const chars = samples.list_characters.characters;
const nameOf = (id: string) => chars.find((c: { id: string }) => c.id === id)?.name ?? id;
const P = (over: Partial<FinderPrefs> = {}): FinderPrefs => ({ ...DEFAULT_PREFS, ...over });
const view = (p: FinderPrefs) => query(sample, p, { channels: ['LIVE'], characterName: nameOf });

/** A Map-backed localStorage for node. */
function fakeStorage() {
  const m = new Map<string, string>();
  (globalThis as { localStorage?: unknown }).localStorage = {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, String(v)),
    removeItem: (k: string) => void m.delete(k),
  };
  return m;
}

test('stored lists: malformed reads as nothing collapsed, duplicates and non-strings dropped', () => {
  assert.deepEqual(parseCollapsed(null), []);
  assert.deepEqual(parseCollapsed('not json'), []);
  assert.deepEqual(parseCollapsed('{"a":1}'), []);
  assert.deepEqual(parseCollapsed('["a", 3, "b", "a", null]'), ['a', 'b']);
  assert.deepEqual(parseCollapsed(JSON.stringify(['x'.repeat(201), 'ok'])), ['ok']);
});

test('withCollapsed adds once and removes, keeping the rest', () => {
  assert.deepEqual(withCollapsed([], 'a', true), ['a']);
  assert.deepEqual(withCollapsed(['a', 'b'], 'a', true), ['b', 'a']);
  assert.deepEqual(withCollapsed(['a', 'b'], 'a', false), ['b']);
  assert.deepEqual(withCollapsed(['b'], 'a', false), ['b']);
});

test('persistence: a card collapsed survives a reload; expanding the last one removes the key', () => {
  const m = fakeStorage();
  writeCollapsed(CARDS_KEY, withCollapsed(readCollapsed(CARDS_KEY), 'blueprints.finder', true));
  writeCollapsed(CARDS_KEY, withCollapsed(readCollapsed(CARDS_KEY), 'blueprints.starorg', true));
  assert.deepEqual(readCollapsed(CARDS_KEY), ['blueprints.finder', 'blueprints.starorg']);
  writeCollapsed(CARDS_KEY, withCollapsed(readCollapsed(CARDS_KEY), 'blueprints.finder', false));
  assert.deepEqual(readCollapsed(CARDS_KEY), ['blueprints.starorg']);
  writeCollapsed(CARDS_KEY, withCollapsed(readCollapsed(CARDS_KEY), 'blueprints.starorg', false));
  assert.equal(m.has(CARDS_KEY), false);
});

test('persistence: finder groups are kept per channel, scope and group-by', () => {
  fakeStorage();
  const live = GROUPS_KEY('steam', 'LIVE', 'channel', 'kind');
  writeCollapsed(live, ['armor']);
  assert.deepEqual(readCollapsed(live), ['armor']);
  assert.deepEqual(readCollapsed(GROUPS_KEY('steam', 'PTU', 'channel', 'kind')), []);
  assert.deepEqual(readCollapsed(GROUPS_KEY('steam', 'LIVE', 'all', 'kind')), []);
  assert.deepEqual(readCollapsed(GROUPS_KEY('steam', 'LIVE', 'channel', 'manufacturer')), []);
});

test('blocked storage: reads as nothing collapsed, writes are dropped', () => {
  (globalThis as { localStorage?: unknown }).localStorage = {
    getItem: () => {
      throw new Error('blocked');
    },
    setItem: () => {
      throw new Error('blocked');
    },
    removeItem: () => {
      throw new Error('blocked');
    },
  };
  assert.deepEqual(readCollapsed(CARDS_KEY), []);
  assert.doesNotThrow(() => writeCollapsed(CARDS_KEY, ['a']));
});

test('arrow keys: Left collapses an open header, Right expands a closed one, else not handled', () => {
  assert.equal(arrowToggle('ArrowLeft', true), false);
  assert.equal(arrowToggle('ArrowRight', false), true);
  assert.equal(arrowToggle('ArrowLeft', false), null);
  assert.equal(arrowToggle('ArrowRight', true), null);
  assert.equal(arrowToggle('Enter', true), null);
});

// Synthetic groups for the paging rules.
const row = (key: string) => ({ key, name: key }) as unknown as BlueprintRow;
const rows = (g: string, n: number) => Array.from({ length: n }, (_, i) => row(`${g}${i}`));
const G = (spec: [string, number][]) => ({ rows: spec.flatMap(([g, n]) => rows(g, n)), groups: spec.map(([id, n]) => ({ id, label: id.toUpperCase(), rows: rows(id, n) })) });
const shape = (lines: ReturnType<typeof pageRows>['lines']) => lines.map((l) => (l.row ? l.row.key : `#${l.group}${l.collapsed ? '-' : ''}`));

test('paging without groups is a plain slice', () => {
  const v = { rows: rows('r', 5), groups: null };
  const p = pageRows(v, 3);
  assert.deepEqual(shape(p.lines), ['r0', 'r1', 'r2']);
  assert.equal(p.total, 5);
});

test('paging with groups: headers then rows, cut at the limit', () => {
  const p = pageRows(G([['a', 2], ['b', 3], ['c', 1]]), 4);
  assert.deepEqual(shape(p.lines), ['#a', 'a0', 'a1', '#b', 'b0', 'b1']);
  assert.equal(p.total, 6);
});

test('a collapsed group keeps its header and leaves the paging (rows and total)', () => {
  const p = pageRows(G([['a', 2], ['b', 300], ['c', 1]]), 200, new Set(['b']));
  assert.deepEqual(shape(p.lines), ['#a', 'a0', 'a1', '#b-', '#c', 'c0']);
  assert.equal(p.total, 3);
});

test('collapsed groups right after a full page still show; groups after a cut wait for Show more', () => {
  // The page ends exactly with a: the collapsed b still shows, c is cut, d waits.
  let p = pageRows(G([['a', 2], ['b', 5], ['c', 2], ['d', 1]]), 2, new Set(['b', 'd']));
  assert.deepEqual(shape(p.lines), ['#a', 'a0', 'a1', '#b-']);
  assert.equal(p.total, 4);
  // Show more: c and the collapsed d appear.
  p = pageRows(G([['a', 2], ['b', 5], ['c', 2], ['d', 1]]), 4, new Set(['b', 'd']));
  assert.deepEqual(shape(p.lines), ['#a', 'a0', 'a1', '#b-', '#c', 'c0', 'c1', '#d-']);
});

test('everything collapsed: only headers, nothing to page', () => {
  const p = pageRows(G([['a', 250], ['b', 300]]), 200, new Set(['a', 'b']));
  assert.deepEqual(shape(p.lines), ['#a-', '#b-']);
  assert.equal(p.total, 0);
});

test('the real finder view pages by group and drops a collapsed category', () => {
  const v = view(P({ owned: 'all', group: 'kind' }));
  assert.ok(v.groups && v.groups.length > 1);
  const first = v.groups[0]!;
  const open = pageRows(v, 10_000);
  const shut = pageRows(v, 10_000, new Set([first.id]));
  assert.equal(open.total - shut.total, first.rows.length);
  assert.equal(shut.lines.filter((l) => l.row && l.group === first.id).length, 0);
  assert.equal(shut.lines.find((l) => l.group === first.id)?.collapsed, true);
});

test('hidden matches: rows only in collapsed groups, and those groups', () => {
  const v = G([['a', 2], ['b', 3], ['c', 1]]);
  assert.deepEqual(hiddenMatches(v, new Set()), { count: 0, groups: [] });
  assert.deepEqual(hiddenMatches(v, new Set(['b', 'c'])), { count: 4, groups: ['b', 'c'] });
  assert.deepEqual(hiddenMatches({ groups: null }, new Set(['b'])), { count: 0, groups: [] });
  // Character groups share rows: a row also in an expanded group is in view.
  const shared = { groups: [{ id: 'x', label: 'X', rows: [row('r1'), row('r2')] }, { id: 'y', label: 'Y', rows: [row('r1')] }] };
  assert.deepEqual(hiddenMatches(shared, new Set(['x'])), { count: 1, groups: ['x'] });
  assert.deepEqual(hiddenMatches(shared, new Set(['y'])), { count: 0, groups: [] });
});

test('a search that matches only a collapsed category says how many', () => {
  const v = view(P({ owned: 'all', group: 'kind', search: 'armor' }));
  const shut = new Set((v.groups ?? []).map((g) => g.id));
  const h = hiddenMatches(v, shut);
  assert.ok(h.count > 0);
  assert.equal(h.count, v.rows.length);
  assert.equal(pageRows(v, 200, shut).total, 0);
});

const profile = (): Profile => structuredClone(samples.get_profile);

test('marker summary of the sample profile: one line, every marker', () => {
  const p = profile();
  assert.equal(emWord(4), 'blue');
  assert.equal(emWord(0), 'plain');
  assert.equal(emWord(2), 'EM2');
  const s = markerSummary(p);
  assert.deepEqual(s.map((x) => x.label), ['[Owned]', '[Missing]', '[BP Owned]']);
  // samples: blueprint list on, [Owned] on and plain, [Missing] off, [BP Owned] off.
  assert.equal(markerLine(p), '[Owned] plain · [Missing] off · [BP Owned] off');
});

test('marker summary follows the switches and says why a marker is off', () => {
  const p = profile();
  p.missions = true;
  p.mission.blueprints = true;
  p.mission.owned_marker = true;
  p.mission.owned_em = 4;
  p.mission.owned_line = false;
  p.mission.missing_marker = true;
  p.mission.missing_em = 0;
  p.mission.title_blueprint = true;
  p.mission.title_owned = true;
  p.mission.title_em = true;
  p.mission.title_bp_em = 3;
  assert.equal(markerLine(p), '[Owned] blue · [Missing] plain · [BP Owned] underline');
  p.mission.owned_line = true;
  p.mission.title_em = false;
  assert.equal(markerLine(p), '[Owned] blue, whole line · [Missing] plain · [BP Owned] plain');
  p.mission.blueprints = false;
  assert.equal(markerLine(p), '[Owned] off (no blueprint list) · [Missing] off (no blueprint list) · [BP Owned] plain');
  p.missions = false;
  assert.equal(markerLine(p), '[Owned] off (contracts off) · [Missing] off (contracts off) · [BP Owned] off (contracts off)');
});

test('the Blueprints tab preview puts the contract with a blueprint list first', () => {
  const items: PreviewItem[] = samples.build_preview;
  const first = previewOrder(items, 'blueprints')[0]!;
  assert.equal(first.category, 'missions');
  assert.match(first.result, /BLUEPRINTS/);
  assert.match(first.result, /\[Owned\]/);
  // Other tabs keep their own category first; nothing is lost.
  assert.equal(previewOrder(items, 'ships')[0]!.category, 'ships');
  assert.equal(previewOrder(items, 'blueprints').length, items.length);
  assert.deepEqual(previewOrder(null, 'ships'), []);
});
