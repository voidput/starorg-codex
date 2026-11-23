// The Commodities screen's pure helpers: group chips, search, the flat row model of the
// windowed list, the visible window, selection, and text helpers, on the Rust mock's samples.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { currentId, explosionText, filterCommodities, flatRows, groupChips, groupPath, numText, scuText, stepId, visibleRange } from '../src/lib/commodities.ts';
import type { CommodityDetail, CommodityDirectory, CommodityInfo } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const list = samples.commodity_list as CommodityDirectory;
const rows = list.commodities;

test('the samples: summaries only, no placeholder groups, details on their own', () => {
  assert.equal(list.state, 'ready');
  assert.equal((samples.commodity_list_not_read as CommodityDirectory).state, 'not-read');
  assert.ok(rows.length > 30);
  assert.deepEqual(Object.keys(rows[0]!).sort(), ['group', 'id', 'name', 'raw', 'subgroup', 'volatile']);
  assert.ok(!rows.some((r) => r.group.toLowerCase().includes('placeholder')));
  const d = (samples.commodity_detail as CommodityInfo).commodity!;
  assert.equal(d.id, 'ResourceType.Raw_Fixtanium');
  assert.ok(d.mined && d.mined.length && d.quality && d.damage.length);
  const all = samples.commodity_details_all as CommodityDetail[];
  assert.equal(all.length, rows.length);
  assert.ok(all.every((c) => !c.market.loaded));
  assert.ok((samples.commodity_details_all_priced as CommodityDetail[]).some((c) => c.market.sell.length && c.market.buy.length));
});

test('group chips count rows per top group, in list order', () => {
  const chips = groupChips(rows);
  assert.equal(
    chips.reduce((n, c) => n + c.count, 0),
    rows.length,
  );
  assert.deepEqual(
    chips.map((c) => c.group),
    [...new Set(rows.map((r) => r.group))],
  );
  assert.deepEqual(groupChips(null), []);
});

test('search matches name, id and group path; the group chip narrows', () => {
  assert.deepEqual(
    filterCommodities(rows, 'synthium', '').map((r) => r.name),
    ['Synthium', 'Synthium (Ore)'],
  );
  assert.deepEqual(
    filterCommodities(rows, 'unrefined', '').map((r) => r.id),
    ['ResourceType.Ore_Mockite', 'ResourceType.Ore_Synthium'],
  );
  assert.equal(filterCommodities(rows, 'raw_fixtanium', '').length, 1);
  assert.ok(filterCommodities(rows, '', 'Gas').every((r) => r.group === 'Gas'));
  assert.equal(filterCommodities(rows, 'synthium', 'Gas').length, 0);
  assert.equal(filterCommodities(undefined, '', '').length, 0);
});

test('flat rows put one header before each group path', () => {
  const flat = flatRows(filterCommodities(rows, '', 'Metal'));
  assert.deepEqual(
    flat.map((r) => (r.kind === 'header' ? `# ${r.label} ${r.count}` : r.item.name)),
    ['# Metal 2', 'Mockite', 'Synthium', '# Metal > Unrefined Ores 2', 'Mockite (Ore)', 'Synthium (Ore)'],
  );
  assert.equal(new Set(flatRows(rows).map((r) => r.key)).size, flatRows(rows).length);
  assert.equal(groupPath({ group: 'Metal', subgroup: null }), 'Metal');
});

test('the window covers the viewport plus overscan, clamped', () => {
  assert.deepEqual(visibleRange(0, 300, 30, 100, 2), { first: 0, last: 12 });
  assert.deepEqual(visibleRange(600, 300, 30, 100, 2), { first: 18, last: 32 });
  assert.deepEqual(visibleRange(10_000, 300, 30, 100, 2), { first: 99, last: 99 });
  assert.deepEqual(visibleRange(0, 300, 30, 0), { first: 0, last: -1 });
  assert.deepEqual(visibleRange(-50, 90, 30, 5, 0), { first: 0, last: 3 });
});

test('selection: the picked row while listed, else the first; arrows stay on the ends', () => {
  const some = filterCommodities(rows, 'synthium', '');
  assert.equal(currentId(some, 'ResourceType.Ore_Synthium'), 'ResourceType.Ore_Synthium');
  assert.equal(currentId(some, 'ResourceType.Hydrogen'), 'ResourceType.Synthium');
  assert.equal(currentId([], 'x'), null);
  assert.equal(stepId(some, 'ResourceType.Synthium', 1), 'ResourceType.Ore_Synthium');
  assert.equal(stepId(some, 'ResourceType.Ore_Synthium', 1), 'ResourceType.Ore_Synthium');
  assert.equal(stepId(some, 'ResourceType.Synthium', -1), 'ResourceType.Synthium');
  assert.equal(stepId(some, 'gone', 1), 'ResourceType.Synthium');
  assert.equal(stepId([], null, 1), null);
});

test('text helpers', () => {
  assert.equal(scuText(0.01), '0.01 SCU');
  assert.equal(scuText(32), '32 SCU');
  assert.equal(scuText(null), '–');
  assert.equal(numText(0.0675), '0.0675');
  assert.equal(numText(1500), '1,500');
  assert.equal(numText(undefined), '–');
  const d = (samples.commodity_detail as CommodityInfo).commodity!.damage[0]!;
  assert.equal(explosionText(d), '375 damage, 0.1–2.1 m');
  assert.equal(explosionText({ ...d, explosion_damage: null, explosion_min_radius: null, explosion_max_radius: null }), null);
});
