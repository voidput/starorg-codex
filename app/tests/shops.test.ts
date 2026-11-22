// The Shops screen's pure helpers: category order and labels, the file-age line, the shop list
// search and system filter, the item filter, chips and grouping, on the Rust mock's samples.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  CATEGORY_LABEL,
  CATEGORY_ORDER,
  categoryChips,
  filterShops,
  firstShop,
  groupItems,
  hasShop,
  matchesWords,
  textFiltered,
  topCategories,
  uexAgeDays,
  uexAgeText,
} from '../src/lib/shops.ts';
import type { ShopDirectory, ShopItemRow, ShopItems } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const dir = samples.shop_inventory as ShopDirectory;
const all = samples.shop_items_all as ShopItems[];

const row = (o: Partial<ShopItemRow>): ShopItemRow => ({
  name: 'X',
  uex_name: null,
  category: 'other',
  price_buy: 1,
  price_sell: null,
  updated: null,
  stale: false,
  record: null,
  ...o,
});

test('every category has a label, in the Rust order', () => {
  assert.equal(CATEGORY_ORDER.length, 9);
  assert.deepEqual(Object.keys(CATEGORY_LABEL).sort(), [...CATEGORY_ORDER].sort());
  assert.equal(CATEGORY_LABEL.missiles, 'Missiles & torpedoes');
});

test('the file age reads as a day and month', () => {
  const now = new Date('2026-11-22T10:00:00Z');
  assert.equal(uexAgeText('2026-11-21T05:30:43.250Z', now), 'UEX data from 21 Nov');
  assert.equal(uexAgeText('2025-12-31T23:00:00Z', now), 'UEX data from 31 Dec 2025');
  assert.equal(uexAgeText(null, now), null);
  assert.equal(uexAgeText('not a date', now), null);
  assert.equal(uexAgeDays('2026-11-01T10:00:00Z', now), 21);
  assert.equal(uexAgeDays('2026-11-23T10:00:00Z', now), 0);
  assert.equal(uexAgeDays(undefined, now), null);
});

test('words match at word starts, or inside for 3+ letters', () => {
  assert.ok(matchesWords("Dumper's GrimHEX Grim HEX, Yela", 'grim'));
  assert.ok(matchesWords("Dumper's GrimHEX", 'dump grim'));
  assert.ok(matchesWords('CenterMass Area 18', 'mass'));
  assert.ok(!matchesWords('CenterMass Area 18', 'ma'));
  assert.ok(!matchesWords('Cafe Lorville', 'orison'));
  assert.ok(matchesWords('anything', '  '));
});

test('the mock directory filters by search and system', () => {
  const g = filterShops(dir, '', '');
  assert.deepEqual(
    g.map((x) => x.system),
    ['Nyx', 'Pyro', 'Stanton'],
  );
  assert.equal(
    g.reduce((n, x) => n + x.count, 0),
    dir.shop_count,
  );
  const stanton = filterShops(dir, '', 'Stanton');
  assert.deepEqual(
    stanton.map((x) => x.system),
    ['Stanton'],
  );
  const grim = filterShops(dir, 'grim hex', '');
  assert.deepEqual(
    grim.flatMap((x) => x.places.flatMap((p) => p.shops.map((s) => s.name))),
    ["Dumper's GrimHEX", 'Skutters Grim'],
  );
  // A system name finds its shops.
  assert.equal(filterShops(dir, 'pyro', '')[0]?.count, 2);
  assert.deepEqual(filterShops(dir, 'zzz', ''), []);
  assert.deepEqual(filterShops(null, '', ''), []);
  assert.equal(firstShop(g)?.name, 'Levski Admin');
  assert.ok(hasShop(g, firstShop(g)!.id));
  assert.ok(!hasShop(grim, firstShop(g)!.id));
  assert.ok(!hasShop(g, null));
});

test('a mock shop groups its items by category and filters them', () => {
  const dumper = all.find((s) => s.shop?.name === "Dumper's GrimHEX")!;
  const rows = dumper.items;
  const chips = categoryChips(rows);
  assert.deepEqual(
    chips.map((c) => c.category),
    ['ship-weapons', 'missiles', 'ship-components', 'mining'],
  );
  assert.equal(
    chips.reduce((n, c) => n + c.count, 0),
    rows.length,
  );
  const groups = groupItems(rows, null);
  assert.deepEqual(
    groups.map((g) => g.label),
    chips.map((c) => c.label),
  );
  const missiles = groupItems(rows, 'missiles');
  assert.equal(missiles.length, 1);
  assert.ok(missiles[0]!.items.every((i) => i.category === 'missiles'));
  // UEX names a missile without its kind word; the filter finds either name.
  const m = missiles[0]!.items.find((i) => i.uex_name)!;
  assert.equal(textFiltered(rows, m.uex_name!).some((i) => i.name === m.name), true);
  assert.deepEqual(
    textFiltered(rows, 'syn-9').map((i) => [i.name, i.record]),
    [['SYN-9 Repeater', null]],
  );
  assert.deepEqual(textFiltered(rows, 'zzzz'), []);
  assert.equal(textFiltered(null, '').length, 0);
  // Biggest first: two named, the rest counted.
  assert.equal(topCategories(dumper.shop!), 'Ship components · Ship weapons +2');
});

test('groups sort by name and skip empty categories', () => {
  const g = groupItems([row({ name: 'b', category: 'armour' }), row({ name: 'a', category: 'armour' }), row({ name: 'z', category: 'fps-weapons' })], null);
  assert.deepEqual(
    g.map((x) => [x.category, x.items.map((i) => i.name)]),
    [
      ['fps-weapons', ['z']],
      ['armour', ['a', 'b']],
    ],
  );
  assert.deepEqual(groupItems([], 'armour'), []);
});

test('the missing sample says where the files go', () => {
  const m = samples.shop_inventory_missing as ShopDirectory;
  assert.equal(m.state, 'missing');
  assert.ok(m.folder?.endsWith('/uex'));
  assert.deepEqual(m.systems, []);
});
