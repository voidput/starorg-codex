// The Commodities screen's Mining tab: the samples' shapes, the list search, kind filter and
// sort, the place sort (most common or best quality first), labels, and the deep link
// (`?screen=mining` is the same screen on its second tab), on the Rust mock's samples.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { curveSource, difficultyInputs, filterMinerals, kindLabel, placeName, powerText, rangeText, scaleNote, shareText, sortMinerals, sortPlaces, trusted } from '../src/lib/mining.ts';
import { findNav, isCommoditiesScreen, isScreen, navItems } from '../src/lib/nav.ts';
import { currentId, stepId } from '../src/lib/commodities.ts';
import type { MineralDetail, MiningDirectory, MiningInfo, MiningPlace } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const list = samples.mining_list as MiningDirectory;
const rows = list.minerals;
const all = samples.mining_details_all as MineralDetail[];
const fix = (samples.mining_detail as MiningInfo).mineral!;

test('the samples: summaries in the list, every detail on its own, physics and lasers read', () => {
  assert.equal(list.state, 'ready');
  assert.equal((samples.mining_list_not_read as MiningDirectory).state, 'not-read');
  assert.ok(list.physics && list.lasers_read);
  assert.deepEqual(Object.keys(rows[0]!).sort(), ['best_high', 'best_high_uncertain', 'difficulty', 'has_override', 'id', 'kinds', 'name', 'places', 'refined', 'spots']);
  assert.ok(list.spawns_read && list.spots_read);
  assert.deepEqual(
    all.map((m) => m.id),
    rows.map((r) => r.id),
  );
  assert.equal(fix.id, 'ResourceType.Raw_Fixtanium');
  assert.equal(fix.commodity?.id, fix.id, 'links back to the Commodities row');
  assert.equal(fix.refined?.name, 'Fixtanium');
  assert.ok(fix.places.every((p) => p.rock_mass === null), 'rock mass is never made up');
  assert.deepEqual(
    list.lasers.map((l) => [l.size, l.max]),
    [
      [1, 3600],
      [2, 4200],
    ],
  );
});

test('search matches name, id and refined form; the kind chip narrows', () => {
  assert.deepEqual(
    filterMinerals(rows, 'fixtanium', '').map((r) => r.name),
    ['Fixtanium (Raw)'],
  );
  assert.deepEqual(
    filterMinerals(rows, '', 'vehicle').map((r) => r.name),
    ['Mockite (Ore)'],
  );
  assert.deepEqual(
    filterMinerals(rows, '', 'hand').map((r) => r.name),
    ['Stubbite'],
  );
  assert.equal(filterMinerals(rows, 'synthium', 'hand').length, 0);
  assert.equal(filterMinerals(rows, '', 'ship').length, 2);
  assert.equal(filterMinerals(null, '', '').length, 0);
});

test('minerals sort by name, or best quality first with unknown last', () => {
  assert.deepEqual(sortMinerals(rows, 'name'), rows);
  const q = sortMinerals(rows, 'quality');
  assert.equal(q[0]!.name, 'Fixtanium (Raw)');
  const highs = q.map((r) => r.best_high ?? -1);
  assert.deepEqual(
    highs,
    [...highs].sort((a, b) => b - a),
  );
  const unknown = { ...rows[0]!, id: 'x', name: 'Aaa', best_high: null };
  assert.equal(sortMinerals([unknown, ...rows], 'quality').at(-1)!.id, 'x');
  // An uncertain best share never outranks a certain one, however high.
  const shaky = { ...rows[0]!, id: 'y', name: 'Zzz', best_high: 0.99, best_high_uncertain: true };
  const ranked = sortMinerals([shaky, ...rows], 'quality');
  assert.ok(ranked.findIndex((r) => r.id === 'y') > ranked.findIndex((r) => r.best_high !== null && !r.best_high_uncertain));
  // The list helpers take minerals too.
  assert.equal(currentId(rows, null), rows[0]!.id);
  assert.equal(stepId(rows, rows[0]!.id, 1), rows[1]!.id);
});

test('places: most common as the backend sends them, best quality puts the override first', () => {
  assert.deepEqual(
    sortPlaces(fix.places, 'common').map((p) => p.location),
    ['StantonBelt_Aaron', 'Pyro1', 'Nyx1', 'Lagrange_B'],
  );
  const best = sortPlaces(fix.places, 'quality');
  assert.equal(best[0]!.location, 'Nyx1');
  assert.equal(best[0]!.quality.override_location, 'Nyx System');
  // Ties keep their common order; the Lagrange point (curve not known, scale ×0.79) goes after
  // every trusted place.
  assert.deepEqual(
    best.slice(1).map((p) => p.location),
    ['StantonBelt_Aaron', 'Pyro1', 'Lagrange_B'],
  );
  const lag = best.at(-1)!;
  assert.ok(!trusted(lag.quality));
  assert.equal(scaleNote(lag), 'quality scale ×0.79: effect not known, share may be lower');
  assert.equal(scaleNote(best[0]!), null);
  assert.match(curveSource(lag.quality), /^not known/);
  // Even a far higher share does not lift an untrusted row above a trusted one.
  const lifted = { ...lag, quality: { ...lag.quality, high: 0.99 } };
  assert.equal(sortPlaces([lifted, ...fix.places.filter((p) => p !== lag)], 'quality').at(-1)!.location, 'Lagrange_B');
  const blank: MiningPlace = { ...fix.places[0]!, location: 'Blank', quality: { range: { min: null, max: null, mean: null, stddev: null }, override_location: null, high: null, top: [], top_share: null } };
  assert.equal(sortPlaces([blank, ...fix.places], 'quality').at(-1)!.location, 'Blank');
  assert.deepEqual(sortPlaces(undefined, 'quality'), []);
});

test('labels: place, kind, curve source, difficulty inputs and numbers', () => {
  const nyx = fix.places.find((p) => p.location === 'Nyx1')!;
  assert.equal(placeName(nyx), 'Nyx I · Nyx');
  assert.equal(placeName({ place: 'Lagrange B', location: 'Lagrange_B', system: null }), 'Lagrange B');
  assert.equal(placeName({ place: '', location: 'Pyro5a', system: 'Pyro' }), 'Pyro5a · Pyro');
  assert.equal(curveSource(nyx.quality), 'Nyx System override');
  assert.equal(curveSource(fix.places[0]!.quality), 'same as base');
  assert.equal(rangeText(nyx.quality.range), '501–1000');
  assert.equal(rangeText(null), '–');
  assert.equal(kindLabel('vehicle'), 'ROC');
  assert.equal(kindLabel(null), '–');
  const d = nyx.difficulty!;
  assert.equal(d.grade, 'hard');
  assert.equal(difficultyInputs(d), 'resistance 0.9 · instability 900 · window thinness 2 → 6 of 6 points');
  assert.match(difficultyInputs({ ...d, window_thinness: null, missing: ['window thinness'], points: 4, max_points: 4 }), /not known: window thinness/);
  assert.equal(powerText(14790.4), '14,790');
  assert.equal(powerText(null), '–');
  assert.equal(shareText(10, 30), '10–30 %');
  assert.equal(shareText(null, null), '–');
});

test('the ship estimate says what it lacks; ROC and hand rocks follow from their kind', () => {
  const r = fix.places[0]!.recommendation;
  assert.equal(r.tool, 'ship');
  assert.ok(r.estimate);
  assert.match(r.reason, /Rock mass not known/);
  assert.match(r.formula ?? '', /1 − resistance/);
  assert.deepEqual(
    r.options.map((o) => [o.ship, o.laser_count, o.power]),
    [
      ['Prospector', 1, 3600],
      ['MOLE', 3, 12600],
    ],
  );
  const roc = all.find((m) => m.name === 'Mockite (Ore)')!.places[0]!.recommendation;
  assert.equal(roc.tool, 'roc');
  assert.ok(!roc.estimate && !roc.options.length);
});

test('Mining is its own sidebar entry and deep link onto the Commodities screen', () => {
  assert.ok(isScreen('mining'));
  assert.equal(findNav('mining').label, 'Mining');
  assert.ok(isCommoditiesScreen('mining') && isCommoditiesScreen('commodities'));
  assert.ok(!isCommoditiesScreen('shops'));
  const ids = navItems(false).map((i) => i.id);
  assert.equal(ids.indexOf('mining'), ids.indexOf('commodities') + 1, 'right after Commodities');
});
