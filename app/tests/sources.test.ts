// The Explorer record's Sources card: which records get one, percent and price text, place names,
// belt bars, the summary and the mock samples. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  beltBars,
  beltPlace,
  hasSources,
  isEmpty,
  pctText,
  priceText,
  qualityText,
  sourcesSummary,
  spotPlace,
} from '../src/lib/sources.ts';
import type { MinedBelt, RecordSources } from '../src/lib/contract/dev.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

const belt = (o: Partial<MinedBelt>): MinedBelt => ({
  location: 'Stanton1',
  system: 'Stanton',
  body: 'Hurston',
  rock: 'R',
  kind: 'ship',
  tier: null,
  spawn_share: 0.1,
  min_percent: null,
  max_percent: null,
  ...o,
});

test('items, commodities and ores get the card; other records do not', () => {
  assert.ok(hasSources('EntityClassDefinition'));
  assert.ok(hasSources('ResourceType'));
  assert.ok(hasSources('MineableElement'));
  assert.ok(!hasSources('MissionBrokerEntry'));
});

test('percent text keeps small odds readable', () => {
  assert.equal(pctText(0.21), '21 %');
  assert.equal(pctText(0.0842), '8.4 %');
  assert.equal(pctText(0.0084), '0.84 %');
  assert.equal(pctText(0.000123), '0.012 %');
  assert.equal(pctText(0.0000005), '< 0.001 %');
  assert.equal(pctText(0), '0 %');
  assert.equal(pctText(null), '–');
});

test('places read as body and system, else the provider key or area', () => {
  assert.equal(beltPlace(belt({})), 'Hurston · Stanton');
  assert.equal(beltPlace(belt({ body: null, location: 'Pyro5a', system: 'Pyro' })), 'Pyro5a · Pyro');
  assert.equal(beltPlace(belt({ body: 'Aaron Halo', system: 'Aaron Halo' })), 'Aaron Halo');
  assert.equal(spotPlace({ area: 'v3slotpreset rockcracker', place: 'Rock Cracker, Nyx', configs: [], rock: 'R', slot_share: 1 }), 'Rock Cracker, Nyx');
  assert.equal(spotPlace({ area: 'cave prison', place: null, configs: [], rock: 'R', slot_share: 1 }), 'cave prison');
});

test('belt bars compare rows of one kind only', () => {
  const bars = beltBars([belt({ spawn_share: 0.2 }), belt({ spawn_share: 0.1 }), belt({ kind: 'hand', spawn_share: 15 })]);
  assert.deepEqual(bars, [1, 0.5, 1]);
  assert.deepEqual(beltBars([belt({ spawn_share: null })]), [0]);
});

test('mock samples: a gun in loot, an ore only at the Rock Cracker, a ship with nothing', () => {
  const gun: RecordSources = samples.dev_record_sources_loot;
  assert.ok(gun.loot && gun.loot.containers.length > 0);
  assert.ok(gun.loot.containers.every((c) => c.odds_per_box > 0 && c.odds_per_box <= 1));
  assert.ok(gun.buy?.loaded);
  assert.deepEqual(gun.buy.offers.map((o) => o.price_buy), [6900, 7029]);
  assert.equal(sourcesSummary(gun), `2 shops · ${gun.loot.container_total} loot areas`);
  const ore: RecordSources = samples.dev_record_sources_ore;
  assert.equal(ore.loot, null);
  assert.equal(ore.buy, null);
  assert.equal(ore.quality[0]!.steps[0]!.shown, 348);
  assert.equal(samples.dev_record_sources_loot.quality.length, 0);
  assert.equal(ore.mined[0]!.name, 'Sadaryx');
  assert.equal(ore.mined[0]!.belts.length, 0);
  assert.equal(spotPlace(ore.mined[0]!.spots[0]!), 'Rock Cracker, Nyx');
  assert.equal(sourcesSummary(ore), '1 mining place');
  const none: RecordSources = samples.dev_record_sources_none;
  assert.ok(isEmpty(none));
  assert.equal(sourcesSummary(none), null);
  assert.equal(sourcesSummary(null), null);
});

test('prices read as aUEC; missing UEX files still show the Buy note', () => {
  assert.equal(priceText(6900), '6,900 aUEC');
  assert.equal(priceText(78261.6), '78,262 aUEC');
  const none: RecordSources = { buy: { loaded: false, folder: '/c/uex', offers: [], total: 0 }, loot: null, mined: [], quality: [] };
  assert.ok(!isEmpty(none));
  assert.ok(isEmpty({ ...none, buy: { ...none.buy!, loaded: true } }));
});

test('quality reads as the range and where most rolls land', () => {
  assert.equal(qualityText({ min: 201, max: 1000, mean: 201, stddev: 298 }), '201–1000, most near 201 (± 298)');
  assert.equal(qualityText({ min: 0, max: 500, mean: null, stddev: null }), '0–500');
  assert.equal(qualityText({ min: 501, max: 1000, mean: 500, stddev: 203 }), '501–1000, most near 501 (± 203)');
  assert.equal(qualityText({ min: null, max: null, mean: null, stddev: null }), '–');
});
