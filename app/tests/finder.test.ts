// The item finder's pure parts (addendum 7.17): view state in the history, filters per
// section, presets and recent searches in guarded storage, chips, highlighting, stat columns.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as F from '../src/lib/finder.ts';
import type { ListPage } from '../src/lib/contract/dev.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

function mem(): Storage {
  const m = new Map<string, string>();
  return {
    get length() {
      return m.size;
    },
    clear: () => m.clear(),
    getItem: (k) => m.get(k) ?? null,
    key: (i) => [...m.keys()][i] ?? null,
    removeItem: (k) => void m.delete(k),
    setItem: (k, v) => void m.set(k, v),
  };
}
const blocked = {
  getItem: () => {
    throw new Error('blocked');
  },
  setItem: () => {
    throw new Error('blocked');
  },
} as unknown as Storage;

test('filters are remembered per section and survive blocked or broken storage', () => {
  const s = mem();
  assert.equal(F.loadFilters(s, 'ships'), null);
  F.saveFilters(s, 'ships', { ...F.NO_FILTERS, sizes: [2], player_only: true });
  F.saveFilters(s, 'components', { ...F.NO_FILTERS, classes: ['Military'] });
  assert.deepEqual(F.loadFilters(s, 'ships')!.sizes, [2]);
  assert.equal(F.loadFilters(s, 'ships')!.player_only, true);
  assert.deepEqual(F.loadFilters(s, 'components')!.classes, ['Military']);
  assert.equal(F.loadFilters(blocked, 'ships'), null);
  F.saveFilters(blocked, 'ships', F.NO_FILTERS);
  s.setItem('starorg.dev.finder.filters', '{"ships":{"sizes":["x",3],"model":"yes","grades":["A"]}}');
  assert.deepEqual(F.loadFilters(s, 'ships'), { ...F.NO_FILTERS, sizes: [3], grades: ['A'] });
  s.setItem('starorg.dev.finder.filters', 'not json');
  assert.equal(F.loadFilters(s, 'ships'), null);
});

test('recent searches: newest first, no duplicates, ten at most', () => {
  const s = mem();
  for (let i = 0; i < 12; i++) F.pushRecent(s, `query ${i}`);
  F.pushRecent(s, 'QUERY 5');
  const r = F.loadRecent(s);
  assert.equal(r.length, F.MAX_RECENT);
  assert.equal(r[0], 'QUERY 5');
  assert.equal(r.filter((x) => x.toLowerCase() === 'query 5').length, 1);
  assert.deepEqual(F.pushRecent(s, ' '), r, 'blank is not kept');
  assert.deepEqual(F.loadRecent(blocked), []);
});

test('presets save by name, replace, delete', () => {
  const s = mem();
  const p = { name: 'S1 military coolers', query: 'type:cooler', filters: { ...F.NO_FILTERS, sizes: [1], classes: ['Military'] }, sort: 'size' as const, group: 'manufacturer' as const, node: '', fold: false };
  F.savePreset(s, p);
  F.savePreset(s, { ...p, name: 'other' });
  F.savePreset(s, { ...p, query: 'type:cooler grade:a' });
  const all = F.loadPresets(s);
  assert.deepEqual(all.map((x) => x.name), ['S1 military coolers', 'other']);
  assert.equal(all[0]!.query, 'type:cooler grade:a');
  assert.deepEqual(F.deletePreset(s, 'other').map((x) => x.name), ['S1 military coolers']);
  assert.deepEqual(F.savePreset(s, { ...p, name: '  ' }).length, 1);
});

test('fold variants: kept by presets and history, on for older ones', () => {
  const s = mem();
  assert.equal(F.DEFAULT_VIEW.fold, true);
  s.setItem('starorg.dev.finder.presets', JSON.stringify([{ name: 'old', query: '', filters: F.NO_FILTERS, sort: 'name', group: 'none', node: '' }]));
  assert.equal(F.loadPresets(s)[0]!.fold, true);
  F.savePreset(s, { ...F.loadPresets(s)[0]!, name: 'flat', fold: false });
  assert.equal(F.loadPresets(s)[0]!.fold, false);
  const old = JSON.stringify({ g: 'g-1', v: { mode: 'items', node: '', query: 'x' } });
  assert.equal(F.decodeEntry(old).view!.fold, true);
  assert.equal(F.decodeEntry(F.encodeEntry('g-1', { ...F.DEFAULT_VIEW, fold: false })).view!.fold, false);
});

test('the variant group of a folded row holds its representative and variants', () => {
  const page = samples.dev_list_folded as ListPage;
  const row = page.items.find((i) => Array.isArray(i.variants) && i.variants.length > 0)!;
  const g = F.variantGroup(page.items, row.guid)!;
  assert.equal(g.label, 'Alpha Kestrel');
  assert.deepEqual(g.members.map((m) => [m.short, m.variant]), [['SYN_Alpha_Kestrel', false], ['SYN_Alpha_Kestrel_PU_AI_CIV', true]]);
  assert.deepEqual(F.variantGroup(page.items, row.variants![0]!.guid), g, 'a variant finds its row');
  const plain = page.items.find((i) => !i.variants)!;
  assert.equal(F.variantGroup(page.items, plain.guid), null);
  assert.equal(F.variantGroup(page.items, null), null);
  assert.equal(F.variantGroup(page.items, 'nope'), null);
});

test('chips list every active filter and remove one at a time', () => {
  const f = { ...F.NO_FILTERS, player_only: true, sizes: [1, 3], manufacturers: ['SYNA'], changes: ['changed'] };
  const chips = F.activeChips(f, (k, v) => (k === 'manufacturers' ? 'Alpha Synthetics' : v));
  assert.deepEqual(chips.map((c) => c.label), ['Player-usable only', 'Size: 1', 'Size: 3', 'Manufacturer: Alpha Synthetics', 'Changes: changed']);
  let g = F.removeChip(f, chips[1]!);
  assert.deepEqual(g.sizes, [3]);
  g = F.removeChip(g, chips[0]!);
  assert.equal(g.player_only, false);
  assert.equal(F.filterCount(g), 3);
  assert.deepEqual(F.toggle(F.toggle(F.NO_FILTERS, 'sizes', '3'), 'sizes', '1').sizes, [1, 3]);
  assert.ok(F.has(F.toggle(F.NO_FILTERS, 'kinds', 'Cooler'), 'kinds', 'Cooler'));
});

test('highlight marks every search word, case-insensitive, prefixes ignored', () => {
  assert.deepEqual(F.highlight('Alpha Kestrel', ['kes', 'type:cooler']), [
    { text: 'Alpha ', hit: false },
    { text: 'Kes', hit: true },
    { text: 'trel', hit: false },
  ]);
  assert.deepEqual(F.highlight('abc', []), [{ text: 'abc', hit: false }]);
  assert.equal(F.highlight('Gladius Gladius', ['glad']).filter((x) => x.hit).length, 2);
});

test('the view rides in the history entry and an old plain entry still reads', () => {
  const v: F.FinderView = { ...F.DEFAULT_VIEW, node: 's:["ships"]', query: 'kestrel', filters: { ...F.NO_FILTERS, sizes: [2] }, group: 'size' };
  const e = F.encodeEntry('g-1', v);
  assert.deepEqual(F.decodeEntry(e), { guid: 'g-1', view: v });
  assert.deepEqual(F.decodeEntry('51000000-0000-4000-8000-000000000001'), { guid: '51000000-0000-4000-8000-000000000001', view: null });
  assert.equal(F.decodeEntry('{broken').view, null);
  assert.equal(F.sectionOf('s:["ships","NOITEM_Vehicle"]'), 'ships');
  assert.equal(F.sectionOf('m:models:["armor"]'), 'armor');
  assert.equal(F.sectionOf('m:models:[]'), '');
  assert.equal(F.sectionOf(''), '');
});

test('stat columns come from the rows, in order, and read with units', () => {
  const page = samples.dev_list_node as ListPage;
  const cols = F.statColumns(page.items);
  assert.deepEqual(cols.map((c) => c.key), ['size', 'crew', 'cargo', 'scm']);
  const kestrel = page.items.find((i) => i.label === 'Alpha Kestrel')!;
  assert.equal(F.statText(kestrel.stats.find((s) => s.key === 'scm')), '180 m/s');
  assert.equal(F.statText(undefined), '–');
  assert.equal(F.statText({ key: 'x', label: 'X', value: 12345.6, unit: null }), '12,346');
});

test('mode is kept per viewer', () => {
  const s = mem();
  assert.equal(F.loadMode(s), 'items');
  F.saveMode(s, 'raw');
  assert.equal(F.loadMode(s), 'raw');
  assert.equal(F.loadMode(blocked), 'items');
});
