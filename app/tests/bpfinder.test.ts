// Blueprint finder (addendum §7.19): the TS mock replays the Rust mock's samples, and the
// filter / facet / sort / group rules over that synthetic data. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { mockFinder, seedOwned, normalize, type FinderChan } from '../src/lib/contract/mock-finder.ts';
import { DEFAULT_PREFS, activeFilters, clearFilters, earnFor, earnId, matchScore, missionOf, missionTitle, parsePrefs, query, toggle, fold, type FinderPrefs } from '../src/lib/bpfinder.ts';
import type { BlueprintFinder } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const sample: BlueprintFinder = samples.blueprint_finder;
const chars = samples.list_characters.characters;
const ALL = ['LIVE', 'HOTFIX', 'PTU'];
const chan = (channel: string): FinderChan => ({ channel, owned: seedOwned(sample, channel), forgotten: new Set() });
const nameOf = (id: string) => chars.find((c: { id: string }) => c.id === id)?.name ?? id;
const P = (over: Partial<FinderPrefs> = {}): FinderPrefs => ({ ...DEFAULT_PREFS, ...over });
const view = (p: FinderPrefs, channels = ['LIVE'], data: BlueprintFinder = sample) => query(data, p, { channels, characterName: nameOf });
const names = (rows: { name: string }[]) => rows.map((r) => r.name);
const count = (v: ReturnType<typeof view>, f: keyof ReturnType<typeof view>['facets'], id: string) => v.facets[f].find((x) => x.id === id)?.count ?? 0;

test('TS mock = Rust mock: every channel, and LIVE alone', () => {
  assert.deepEqual(mockFinder(sample, ALL.map(chan), chars), sample);
  assert.deepEqual(mockFinder(sample, [chan('LIVE')], chars), samples.blueprint_finder_live);
});

test('normalize matches the core: tags, quotes, spaces, case', () => {
  assert.equal(normalize('[CIV-S3-A] [H]  7CA ’Nargun’ '), "7ca 'nargun'");
});

test('ownership is per channel: PTU owners never show on LIVE', () => {
  const live = view(P());
  assert.ok(live.rows.every((r) => r.owners.some((o) => o.channel === 'LIVE')));
  assert.ok(!names(live.rows).includes('PTU Prototype Shield'));
  const ptu = view(P(), ['PTU']);
  assert.deepEqual(names(ptu.rows), ['PTU Prototype Shield', 'Sample Helmet Grey', 'Synthetic Repeater S2', 'Synthetic Rifle Mk1']);
  // All channels: the merged row lists where it is owned.
  const all = view(P(), ALL);
  const rifle = all.rows.find((r) => r.name === 'Synthetic Rifle Mk1')!;
  assert.deepEqual([...new Set(rifle.owners.map((o) => o.channel))], ['LIVE', 'HOTFIX', 'PTU']);
});

test('All / Owned / Missing over the full catalogue, with counts', () => {
  const v = view(P({ owned: 'all' }));
  // 39 blueprints + one name row (Alt_B's log name two blueprints have).
  assert.equal(v.total, 40);
  assert.equal(v.rows.length, 40);
  assert.deepEqual(v.owned, { all: 40, owned: 10, missing: 30 });
  const missing = view(P({ owned: 'missing' }));
  assert.equal(missing.rows.length, 30);
  // The two blueprints that name could be are "maybe owned", not plainly missing.
  assert.equal(missing.maybe, 2);
  assert.ok(missing.rows.filter((r) => r.maybe.length).every((r) => r.name === 'Sample Visor'));
  // The facet numbers follow the owned toggle.
  assert.equal(count(view(P({ owned: 'all' })), 'kind', 'armor'), 13);
  assert.equal(count(view(P({ owned: 'owned' })), 'kind', 'armor'), 4);
});

test('facet counts apply every other filter but their own', () => {
  const v = view(P({ owned: 'all', kind: ['ship-component'] }));
  assert.equal(v.rows.length, 10);
  // Kind still shows what the other kinds would add.
  assert.equal(count(v, 'kind', 'fps-weapon'), 9);
  assert.equal(count(v, 'kind', 'ship-component'), 10);
  // Makers and sizes are counted within ship components.
  assert.equal(count(v, 'manufacturer', 'SYNA'), 5);
  assert.equal(count(v, 'manufacturer', 'TSTO'), 0);
  assert.equal(count(v, 'size', 's2'), 4);
  // Two values of one facet: OR. Two facets: AND.
  const two = view(P({ owned: 'all', kind: ['ship-component'], size: ['s1', 's3'] }));
  assert.deepEqual(names(two.rows), ['Sample Mining Laser S1', 'Sample Power Plant S1', 'Sample Shield S1', 'Synthetic Cooler S1', 'Synthetic Cooler S3', 'Test Radar S1']);
  // Armor weights are sizes too.
  assert.equal(view(P({ owned: 'all', size: ['heavy'] })).rows.length, 5);
});

test('character filter: owned by, or missing for, the picked characters', () => {
  const alt = '1000000000002';
  const owned = view(P({ character: [alt] }));
  assert.deepEqual(names(owned.rows), ['Sample Shield S1', 'Sample Undersuit', 'Sample Visor', 'Synthetic Rifle Mk1']);
  const v = view(P());
  assert.equal(count(v, 'character', '1000000000001'), 6);
  assert.equal(count(v, 'character', alt), 4);
  assert.equal(count(v, 'character', 'unattributed'), 1);
  assert.equal(count(v, 'character', '1000000000003'), 0);
  const missing = view(P({ owned: 'missing', character: [alt] }));
  assert.equal(missing.rows.length, 40 - 4);
  assert.equal(count(missing, 'character', '1000000000003'), 40, 'Alt_C has nothing on LIVE');
  // With "All", picking a character keeps every row; the toggle counts follow the pick.
  const all = view(P({ owned: 'all', character: [alt] }));
  assert.equal(all.rows.length, 40);
  assert.deepEqual(all.owned, { all: 40, owned: 4, missing: 36 });
});

test('source facet groups StarOrg orgs and follows the character filter', () => {
  const v = view(P());
  assert.deepEqual(v.facets.source.map((s) => [s.id, s.count]), [['log', 7], ['smart-citizen', 2], ['starorg', 1]]);
  assert.deepEqual(names(view(P({ source: ['smart-citizen'] })).rows), ['Sample Shield S1', 'Synthetic Cooler S2']);
  assert.deepEqual(names(view(P({ source: ['smart-citizen'], character: ['1000000000001'] })).rows), ['Synthetic Cooler S2']);
});

test('channel facet narrows "All channels" to where a blueprint is (owned)', () => {
  const v = view(P({ owned: 'all', channel: ['PTU'] }), ALL);
  assert.ok(!names(v.rows).includes('Sample Crate'), 'not in PTU’s build');
  assert.ok(names(v.rows).includes('PTU Prototype Shield'));
  const owned = view(P({ channel: ['HOTFIX'] }), ALL);
  assert.deepEqual(names(owned.rows), ['Synthetic Rifle Mk1']);
  const all = view(P(), ALL);
  assert.deepEqual(all.facets.channel.map((c) => [c.id, c.count]), [['LIVE', 10], ['HOTFIX', 1], ['PTU', 4]]);
});

test('search: words over name, record, type and maker; fuzzy letters in order', () => {
  assert.deepEqual(names(view(P({ owned: 'all', search: 'cooler s2' })).rows), ['Synthetic Cooler S2']);
  assert.equal(view(P({ owned: 'all', search: 'mock dynamics' })).rows.length, 13);
  assert.ok(names(view(P({ owned: 'all', search: 'BP_CRAFT_SYN_Sample_Crate' })).rows).includes('Sample Crate'));
  const fuzzy = view(P({ owned: 'all', search: 'scnn' }));
  assert.deepEqual(names(fuzzy.rows), ['Synthetic Cannon S3']);
  // Letters in order only when nothing matches as text, and only for 4+ letters.
  assert.ok(names(view(P({ owned: 'all', search: 'cool' })).rows).every((n) => n.includes('Cooler')));
  assert.equal(view(P({ owned: 'all', search: 'scn' })).rows.length, 0);
  // Word hits rank before fuzzy ones.
  const row = sample.rows.find((r) => r.name === 'Synthetic Cannon S3')!;
  assert.equal(matchScore(row, [fold('cannon')]), 2);
  assert.equal(matchScore(row, [fold('scnn')]), 1);
  assert.equal(matchScore(row, [fold('zz')]), 0);
});

test('sorts: name, category, maker, size/grade, acquired, source, owners; unknowns last', () => {
  const first = (p: Partial<FinderPrefs>, n = 3) => names(view(P({ owned: 'all', ...p })).rows.slice(0, n));
  assert.deepEqual(first({ sort: 'name' }), ['Sample Armor Arms', 'Sample Armor Legs', 'Sample Backpack']);
  assert.deepEqual(first({ sort: 'name', desc: true }), ['Test Sniper', 'Test Scattergun S1', 'Test Rifle Magazine']);
  assert.equal(view(P({ owned: 'all', sort: 'kind' })).rows[0]!.kind, 'ship-component');
  assert.equal(view(P({ owned: 'all', sort: 'kind', desc: true })).rows[0]!.kind, 'mission-item');
  const maker = view(P({ owned: 'all', sort: 'manufacturer' })).rows;
  assert.equal(maker[0]!.manufacturer_name, 'Mock Dynamics');
  assert.equal(maker.at(-1)!.manufacturer, null, 'unknown maker last');
  assert.equal(view(P({ owned: 'all', sort: 'manufacturer', desc: true })).rows.at(-1)!.manufacturer, null, 'last in both directions');
  const size = view(P({ owned: 'all', sort: 'size' })).rows;
  assert.deepEqual([size[0]!.size, size[0]!.grade], [1, 'A']);
  assert.equal(size.at(-1)!.size, null);
  assert.deepEqual(first({ sort: 'acquired' }, 2), ['Test Scattergun S1', 'Synthetic Cooler S2']);
  assert.deepEqual(first({ sort: 'acquired', desc: true }, 1), ['Synthetic Rifle Mk1']);
  assert.equal(first({ sort: 'acquired' }, 9).length, 9);
  assert.deepEqual(first({ owned: 'owned', sort: 'source', desc: true }, 1), ['Synthetic Cannon S3']);
  assert.deepEqual(first({ sort: 'owners', desc: true }, 1), ['Synthetic Rifle Mk1']);
});

test('group by category, maker or character (a row per owning character, plus Not owned)', () => {
  const k = view(P({ owned: 'all', group: 'kind' })).groups!;
  assert.deepEqual(k.map((g) => [g.label, g.rows.length]), [['Ship component', 10], ['Ship weapon', 5], ['FPS weapon', 9], ['Armor', 13], ['FPS ammo', 2], ['Mission item', 1]]);
  const m = view(P({ owned: 'all', group: 'manufacturer' })).groups!;
  assert.equal(m.at(-1)!.label, 'Unknown maker');
  const c = view(P({ owned: 'all', group: 'character' })).groups!;
  assert.deepEqual(c.map((g) => [g.label, g.rows.length]), [['Alt_B', 4], ['Pilot_A', 6], ['unattributed', 1], ['Not owned', 30]]);
});

test('prefs: stored, validated, cleared', () => {
  assert.deepEqual(parsePrefs(null), DEFAULT_PREFS);
  assert.deepEqual(parsePrefs('{bad json'), DEFAULT_PREFS);
  const p = parsePrefs(JSON.stringify({ scope: 'all', sort: 'size', desc: true, owned: 'missing', kind: ['armor', 3], group: 'nope', search: 'x' }));
  assert.deepEqual([p.scope, p.sort, p.desc, p.owned, p.kind, p.group, p.search], ['all', 'size', true, 'missing', ['armor'], 'none', 'x']);
  assert.equal(activeFilters(p), 3);
  // Channel picks count only in the All channels view.
  assert.equal(activeFilters({ ...p, channel: ['PTU'] }, false), 3);
  assert.equal(activeFilters({ ...p, channel: ['PTU'] }, true), 4);
  const c = clearFilters(toggle(p, 'size', 's2'));
  assert.equal(activeFilters(c), 0);
  assert.deepEqual([c.scope, c.sort, c.desc], ['all', 'size', true], 'sort and scope stay');
  assert.deepEqual(toggle(toggle(P(), 'kind', 'armor'), 'kind', 'armor').kind, []);
});

test('a pull or import shows up for that channel and character only', () => {
  const live = chan('LIVE');
  live.owned.get('1000000000003')!.push({ name: 'Sample Armor Legs', ambiguous: false, favorite: false, sources: ['import:smart-citizen'], acquired: '2026-11-01T10:00:00Z' });
  live.owned.get('1000000000003')!.push({ name: '[H] Retired Sample Helmet', ambiguous: false, favorite: false, sources: ['log'], acquired: null });
  const f = mockFinder(sample, [live, chan('PTU')], chars);
  const legs = f.rows.find((r) => r.name === 'Sample Armor Legs')!;
  assert.deepEqual(legs.owners.map((o) => [o.channel, o.character]), [['LIVE', '1000000000003']]);
  const retired = f.rows.find((r) => r.key === 'name:retired sample helmet')!;
  assert.deepEqual([retired.kind, retired.channels], ['other', []]);
  assert.ok(names(view(P(), ['LIVE'], f).rows).includes('[H] Retired Sample Helmet'));
});

test('missing for: owned by one character, missing for another, with counts', () => {
  const pilot = '1000000000001';
  const alt = '1000000000002';
  const altC = '1000000000003';
  // What Alt_B has that Pilot_A lacks (the rifle both own drops out).
  const v = view(P({ character: [alt], missing: [pilot] }));
  assert.deepEqual(names(v.rows), ['Sample Shield S1', 'Sample Undersuit', 'Sample Visor']);
  assert.deepEqual(v.owned, { all: 34, owned: 3, missing: 31 });
  // Missing-for numbers: the view with that character added (ANDed).
  assert.equal(count(v, 'missing', pilot), 3);
  assert.equal(count(v, 'missing', altC), 3, 'Alt_C has nothing on LIVE');
  assert.equal(count(v, 'missing', alt), 0);
  // The other facets count within it.
  assert.equal(count(v, 'character', alt), 3);
  assert.equal(count(v, 'kind', 'armor'), 2);
  const before = view(P({ character: [alt] }));
  assert.equal(count(before, 'missing', pilot), 3);
  assert.equal(count(before, 'missing', altC), 4);
  assert.equal(view(P({ character: [pilot], missing: [pilot] })).rows.length, 0);
  // Missing means no source at all: the source filter does not narrow it.
  assert.deepEqual(names(view(P({ character: [alt], missing: [pilot], source: ['smart-citizen'] })).rows), ['Sample Shield S1']);
  // Two picks: missing for both.
  assert.equal(view(P({ owned: 'all', missing: [pilot, alt] })).rows.length, 40 - 9);
  assert.equal(activeFilters(P({ missing: [pilot] })), 1);
  assert.deepEqual(clearFilters(P({ missing: [pilot], earn: ['chance'], faction: ['x'] })), P());
});

test('earn it: missions facet, reputation facet, and the shared mission table', () => {
  const v = view(P({ owned: 'all' }));
  // A sure pool of several is not "guaranteed" for each blueprint in it.
  assert.deepEqual(v.facets.earn.map((x) => [x.id, x.label, x.count]), [
    ['guaranteed', 'Guaranteed', 1],
    ['pool', 'From a guaranteed pool', 4],
    ['chance', 'By chance', 5],
    ['none', 'Not from missions', 29],
    ['unknown', 'Not known', 1],
  ]);
  // The game's faction name when it has one; else the record without its lawful marker.
  assert.deepEqual(v.facets.faction.map((x) => [x.label, x.count]), [['Synthetic Security Bureau', 4], ['Test CDS', 4], ['No reputation', 3]]);
  const g = view(P({ owned: 'all', earn: ['pool'], faction: ['factionreputation_unlawful_test_cds'] }));
  assert.deepEqual(names(g.rows), ['Synthetic Rifle Mk2']);
  // The reputation numbers follow the missions pick, and the other way round.
  assert.equal(count(view(P({ owned: 'all', earn: ['chance'] })), 'faction', 'factionreputation_lawful_synthetic_security'), 0);
  assert.equal(count(view(P({ owned: 'all', faction: ['none'] })), 'earn', 'chance'), 2);
  // A name row's missions are not known (never "none").
  const visor = sample.rows.find((r) => r.record === null)!;
  assert.equal(earnId(earnFor(visor, new Set(['LIVE']))), 'unknown');
  // Rows point into one table; the odds keep the in-game MISSION DETAILS wording.
  const mk2 = sample.rows.find((r) => r.name === 'Synthetic Rifle Mk2')!;
  const ms = mk2.earn.map((e) => [missionTitle(missionOf(sample, e)!), e.award, e.odds, e.pool, e.pick]);
  assert.deepEqual(ms, [
    ['Eliminate the Sample Warlord', 'guaranteed-pool', 'Guaranteed', '5', '17%'],
    ['Recover the Cargo near [Location]', 'chance', '30%', '4', null],
  ]);
  const arms = sample.rows.find((r) => r.name === 'Sample Armor Arms')!;
  assert.equal(arms.earn[0]!.odds, 'Some variants only');
  const knife = sample.rows.find((r) => r.name === 'Test Knife')!;
  assert.equal(knife.earn[0]!.award, 'guaranteed', 'a sure pool of one');
  assert.equal(missionTitle(missionOf(sample, knife.earn[0]!)!), 'SYN_Debug_Reward', 'untitled: its record');
  assert.equal(missionOf(sample, { ...knife.earn[0]!, mission: 99 }), null);
});

test('earn it over channels: every build’s missions, narrowed by the channel filter', () => {
  // LIVE's missions do not award the Mk1; PTU's trial does.
  const mk1 = (p: Partial<FinderPrefs>) => view(P({ owned: 'all', search: 'Synthetic Rifle Mk1', ...p }), ALL);
  assert.equal(mk1({}).facets.earn.find((x) => x.count)!.id, 'guaranteed');
  assert.equal(mk1({ channel: ['LIVE'] }).facets.earn.find((x) => x.count)!.id, 'none');
  assert.equal(mk1({ channel: ['PTU'] }).facets.earn.find((x) => x.count)!.id, 'guaranteed');
  assert.equal(view(P({ owned: 'all', search: 'Synthetic Rifle Mk1' })).facets.earn.find((x) => x.count)!.id, 'none', 'LIVE alone');
  const row = sample.rows.find((r) => r.name === 'Synthetic Rifle Mk1')!;
  assert.deepEqual(row.earn_read, ['LIVE', 'PTU']);
  assert.deepEqual(earnFor(row, new Set(['HOTFIX'])), null, 'HOTFIX is not read: not known');
  assert.deepEqual(earnFor(row, new Set(['LIVE'])), []);
  assert.deepEqual(earnFor(row, new Set(['PTU']))!.map((e) => e.channels), [['PTU']]);
});

test('prefs: missing, missions and reputation are validated', () => {
  const p = parsePrefs(JSON.stringify({ missing: ['1000000000001', 5], earn: ['pool', 'nope'], faction: 'x' }));
  assert.deepEqual([p.missing, p.earn, p.faction], [['1000000000001'], ['pool'], []]);
  assert.equal(activeFilters(p), 2);
  // Prefs stored before these filters existed read with them empty.
  assert.deepEqual(parsePrefs(JSON.stringify({ owned: 'missing' })), { ...DEFAULT_PREFS, owned: 'missing' });
});
