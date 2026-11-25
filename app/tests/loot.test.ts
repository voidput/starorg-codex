// The Loot screen's pure helpers (odds text, search and group filter, drop labels) and the TS
// mock's answers, on the Rust mock's data (mock-loot.gen.json). The trees: loottree.test.ts.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { GEAR_KIND, GEAR_NOTE, NPC_KIND, NPC_NOTE, branchPer, conditionText, dropWhere, filterSummaries, groupPer, groupsOf, isGear, isNpc, listed, oddsText, perWhat, placesText, queryWords, rowPer, slotRank } from '../src/lib/loot.ts';
import { branchId } from '../src/lib/loottree.ts';
import { mockLootDetail, mockLootList, type LootMockData } from '../src/lib/contract/mock-loot.ts';
import type { LootSummary } from '../src/lib/contract/types.ts';

const data = JSON.parse(readFileSync(new URL('../src/lib/contract/mock-loot.gen.json', import.meta.url), 'utf8')) as LootMockData;

const row = (id: string, name: string, group: string, path: string[] = [group], best = 0.1, count = 1): LootSummary => ({ id, name, group, path, best, count, npcs: 0 });

test('odds read like the WHERE TO GET line', () => {
  assert.equal(oddsText(0.75), '75.0%');
  assert.equal(oddsText(0.1), '10.0%');
  assert.equal(oddsText(0.0123), '1.23%');
  assert.equal(oddsText(0.001), '0.10%');
  assert.equal(oddsText(0.00012345), '0.012%');
  assert.equal(oddsText(0), '0%');
  assert.equal(oddsText(Number.NaN), '0%');
});

test('search needs every word in the name, group or id; the group filter is exact', () => {
  const rows = [row('caves/loot_caves_occupied', 'Occupied', 'Caves'), row('v3slotpreset_ugfs', 'Underground facilities', 'Underground facilities'), row('lmg', 'FS-9 LMG', 'FPS weapons')];
  assert.deepEqual(queryWords('  Caves  OCC '), ['caves', 'occ']);
  assert.deepEqual(
    filterSummaries(rows, 'caves occ', '').map((r) => r.id),
    ['caves/loot_caves_occupied'],
  );
  assert.deepEqual(
    filterSummaries(rows, 'ugfs', '').map((r) => r.name),
    ['Underground facilities'],
  );
  assert.equal(filterSummaries(rows, '', 'FPS weapons').length, 1);
  assert.equal(filterSummaries(rows, 'lmg', 'Caves').length, 0);
  assert.deepEqual(filterSummaries(null, 'x', ''), []);
});

test('groups sort by name with Other last', () => {
  const rows = [row('a', 'A', 'Caves'), row('b', 'B', 'Caves'), row('c', 'C', 'Other'), row('d', 'D', 'Asteroid bases')];
  assert.deepEqual(groupsOf(rows), [
    { group: 'Asteroid bases', count: 1 },
    { group: 'Caves', count: 2 },
    { group: 'Other', count: 1 },
  ]);
});

test('search matches the tree path too', () => {
  const rows = [row('caves/loot_caves_occupied_rock', 'Rock', 'Caves', ['Caves', 'Occupied']), row('cave_aberdeen_rich', 'Rich', 'Caves', ['Caves', 'Aberdeen'])];
  assert.deepEqual(
    filterSummaries(rows, 'occupied', '').map((r) => r.name),
    ['Rock'],
  );
  assert.deepEqual(
    filterSummaries(rows, 'aberdeen rich', 'Caves').map((r) => r.id),
    ['cave_aberdeen_rich'],
  );
});

test('a drop says what the spot is, its rarity and how the item is there', () => {
  assert.equal(dropWhere({ container: 'Large weapons crate', rarity: 'Rare', kind: 'loot' }), 'Large weapons crate · Rare');
  assert.equal(dropWhere({ container: 'Weapons rack', rarity: null, kind: 'fixed' }), 'Weapons rack · always in it');
  assert.equal(dropWhere({ container: 'Lying loose', rarity: null, kind: 'loose' }), 'Lying loose');
  const rows = [row('a', 'A', 'G')];
  assert.equal(listed(rows, 'a'), 'a');
  assert.equal(listed(rows, 'b'), null);
  assert.equal(listed(rows, null), null);
});

test('the TS mock serves the Rust mock: missing until read, then lists and grouped, likeliest-first details', () => {
  assert.deepEqual(mockLootList(data, false, 'areas'), { state: 'missing', view: 'areas', build: null, gear: 'missing', rows: [] });
  const areas = mockLootList(data, true, 'areas');
  const items = mockLootList(data, true, 'items');
  assert.equal(areas.state, 'ready');
  assert.ok(areas.rows.length >= 10 && items.rows.length >= 40);
  // Summaries only (with their tree path): no drops in a list row.
  assert.deepEqual(Object.keys(areas.rows[0]!).sort(), ['best', 'count', 'group', 'id', 'name', 'npcs', 'path', 'worn']);
  const sorted = (rows: { odds: number }[]) => rows.every((x, i) => i === 0 || rows[i - 1]!.odds >= x.odds);
  for (const r of areas.rows) {
    const d = mockLootDetail(data, true, 'areas', r.id.toUpperCase());
    assert.equal(d.entry?.id, r.id);
    assert.equal(d.group_by, 'container');
    assert.equal(d.total, r.count);
    // Headers cover every drop, best first; open groups hold their container's rows.
    assert.equal(
      d.groups.reduce((n, g) => n + g.count, 0),
      r.count,
    );
    // (A worn-gear row's slots come in body order instead.)
    const ranked = isGear(r.id) ? (g: { label: string }) => slotRank(g.label) : (g: { best: number }) => -g.best;
    assert.ok(d.groups.every((g, i) => i === 0 || ranked(d.groups[i - 1]!) <= ranked(g)));
    assert.ok(Array.isArray(d.groups[0]!.rows));
    for (const g of d.groups) if (g.rows) assert.ok(sorted(g.rows) && g.rows.every((x) => x.container === g.label));
    // Ungrouped: one group of every drop.
    const flat = mockLootDetail(data, true, 'areas', r.id, null, 'none');
    assert.equal(flat.groups.length, 1);
    assert.equal(flat.groups[0]!.rows!.length, r.count);
    assert.ok(sorted(flat.groups[0]!.rows!));
  }
  const gun = items.rows.find((r) => r.group === 'FPS weapons')!;
  const d = mockLootDetail(data, true, 'items', gun.id);
  assert.ok(d.record?.guid);
  assert.equal(d.group_by, 'place');
  assert.ok(d.groups.every((g) => !g.rows || g.rows.every((x) => x.group.split(' › ')[0] === g.label)));
  // Asking for one group answers its rows only.
  const last = d.groups.at(-1)!;
  const one = mockLootDetail(data, true, 'items', gun.id, null, null, [last.key]);
  assert.ok(one.groups.every((g) => (g.key === last.key) === Array.isArray(g.rows)));
  assert.equal(mockLootDetail(data, true, 'items', 'nothing').entry, null);
  assert.equal(mockLootDetail(data, false, 'items', gun.id).state, 'missing');
  // The filter: every word, `all` still counts every drop; the matching groups open.
  const f = mockLootDetail(data, true, 'items', gun.id, '  CAVES ', 'none');
  const fr = f.groups[0]!.rows!;
  assert.ok(fr.length > 0 && fr.every((r) => `${r.name} ${r.group} ${r.id}`.toLowerCase().includes('caves')));
  assert.equal(f.total, fr.length);
  assert.equal(f.all, d.all);
  const fg = mockLootDetail(data, true, 'items', gun.id, 'caves');
  assert.ok(fg.groups.length > 0 && fg.groups.every((g) => Array.isArray(g.rows)));
  assert.equal(mockLootDetail(data, true, 'items', gun.id, 'zzz').groups.length, 0);
  assert.equal(mockLootDetail(data, true, 'items', gun.id, 'zzz').entry?.id, gun.id);
  // Answers are copies: editing one does not change the next.
  d.groups.length = 0;
  assert.ok(mockLootDetail(data, true, 'items', gun.id).groups.length > 0);
});

test('NPC bodies: rows of their own kind, per body, grouped by table slot, and drop places of items', () => {
  assert.ok(isNpc('npc:V3LootTable_Actor_Grunt'));
  assert.ok(!isNpc('caves/loot_caves_occupied') && !isNpc(null));
  assert.equal(perWhat('npc:V3LootTable_Actor_Grunt'), 'per body');
  assert.equal(perWhat('caves/loot_caves_occupied'), 'per box');
  assert.ok(NPC_NOTE.includes('worn-gear row') && NPC_NOTE.endsWith('The data gives no roll count; these odds may be a floor.') && !NPC_NOTE.includes('not in this list'));
  const npcs = mockLootList(data, true, 'areas').rows.filter((r) => isNpc(r.id));
  assert.ok(npcs.length >= 4);
  assert.ok(npcs.every((r) => r.group === NPC_KIND && r.path[0] === NPC_KIND));
  // Templates are no NPC; names are readable; a council-scrip variant is its own row.
  assert.ok(npcs.every((r) => !/template/i.test(r.id) && !r.name.includes('_')));
  assert.deepEqual(
    npcs.filter((r) => r.path[1] === 'Ground combat').map((r) => r.name),
    ['Grunt', 'Soldier', 'Soldier (with council scrip)'],
  );
  assert.ok(groupsOf(npcs).some((g) => g.group === NPC_KIND));
  // An NPC's drops group by the slot of its table, likeliest first.
  const d = mockLootDetail(data, true, 'areas', 'npc:V3LootTable_Actor_Soldier_CouncilScrip');
  assert.deepEqual(
    d.groups.map((g) => g.label),
    ['Unique item', 'Ammo', 'Sidearm'],
  );
  assert.equal(d.groups[0]!.rows![0]!.id, 'syn_food_ration_mint');
  // By item: the NPCs under their kind, so an item search answers which NPC drops it.
  const it = mockLootDetail(data, true, 'items', 'syn_food_ration_mint');
  const npcGroup = it.groups.find((g) => g.label === NPC_KIND);
  assert.ok(npcGroup, JSON.stringify(it.groups.map((g) => g.label)));
  assert.ok((npcGroup.rows ?? []).some((r) => r.name === 'Soldier (with council scrip)'));
  const mint = mockLootList(data, true, 'items').rows.find((r) => r.id === 'syn_food_ration_mint')!;
  assert.ok(mint.npcs > 0 && mint.npcs <= mint.count);
});

test('places and NPCs are counted apart, and odds say what they are per', () => {
  const r = (count: number, npcs: number) => ({ id: 'x', count, npcs });
  assert.equal(placesText(r(3, 0)), '3 areas');
  assert.equal(placesText(r(2, 2)), '2 NPCs');
  assert.equal(placesText(r(4, 1)), '3 areas, 1 NPC');
  assert.equal(placesText(r(1, 0)), '1 area');
  assert.equal(rowPer('items', r(3, 0)), 'per box');
  assert.equal(rowPer('items', r(2, 2)), 'per body');
  assert.equal(rowPer('items', r(4, 1)), 'per box or body');
  assert.equal(rowPer('areas', { id: 'npc:V3LootTable_Actor_Grunt', count: 3, npcs: 0 }), 'per body');
  assert.equal(branchPer('areas', branchId([NPC_KIND, 'Ground combat'])), 'per body');
  assert.equal(branchPer('areas', branchId(['Caves'])), 'per box');
  assert.equal(groupPer('items', r(4, 1), 'place', NPC_KIND), 'per body');
  assert.equal(groupPer('items', r(4, 1), 'place', 'Caves'), 'per box');
  assert.equal(groupPer('items', r(4, 1), 'none', 'All drops'), 'per box or body');
  assert.equal(groupPer('areas', { id: 'npc:V3LootTable_Actor_Grunt', count: 3, npcs: 0 }, 'container', 'Ammo'), 'per body');
});

const ACE_GEAR = 'gear:PU_Human_Enemy_GroundCombat_NPC_GenericCriminal_AcePilot';

test('worn gear: its own kind, by slot in body order, with chances, notes and links', () => {
  assert.ok(isGear(ACE_GEAR) && !isGear('npc:LootTable_Actor_AcePilot') && !isGear(null));
  assert.equal(perWhat(ACE_GEAR), 'per NPC (est.)');
  assert.ok(GEAR_NOTE.startsWith('What this NPC type can spawn wearing') && GEAR_NOTE.includes('assumed even'));
  assert.ok(!/drops/i.test(GEAR_NOTE));
  assert.equal(dropWhere({ container: 'Helmet', rarity: null, kind: 'worn', note: 'if AI/NpcClass/Heavy' }), 'Helmet · can spawn wearing · if AI/NpcClass/Heavy');
  assert.equal(slotRank('Undersuit'), 0);
  assert.ok(slotRank('Helmet') < slotRank('Primary weapon') && slotRank('Nope') > slotRank('Carried'));
  assert.equal(conditionText({ min: 0, max: 0.5 }), 'Wear range 0–0.5');
  assert.equal(conditionText({ min: 0.33, max: 0.33 }), 'Wear range 0.33');
  assert.equal(conditionText(null), 'Condition not known');
  assert.equal(placesText({ count: 3, npcs: 1, worn: 1 }), '1 area, 1 NPC, worn by 1 NPC type');
  assert.equal(placesText({ count: 2, npcs: 0, worn: 2 }), 'worn by 2 NPC types');
  assert.equal(rowPer('items', { id: 'x', count: 2, npcs: 0, worn: 2 }), 'per NPC (est.)');
  assert.equal(rowPer('items', { id: 'x', count: 3, npcs: 1, worn: 1 }), 'per box, body or NPC');
  assert.equal(rowPer('areas', { id: ACE_GEAR, count: 2, npcs: 0, worn: 0 }), 'per NPC (est.)');
  assert.equal(branchPer('areas', 'p:NPCs (worn gear)'), 'per NPC (est.)');
  assert.equal(groupPer('items', { id: 'x', count: 2, npcs: 0, worn: 2 }, 'place', GEAR_KIND), 'per NPC (est.)');
  const list = mockLootList(data, true, 'areas');
  assert.equal(list.gear, 'ready');
  assert.equal(mockLootList(data, false, 'areas').gear, 'missing');
  const gear = list.rows.filter((r) => isGear(r.id));
  assert.ok(gear.length >= 4 && gear.every((r) => r.group === GEAR_KIND && r.path[0] === GEAR_KIND));
  assert.ok(gear.every((r) => !r.id.includes('FeatureTest') && !r.name.includes('_')));
  // The TS mock orders a gear row's slots in body order, as Rust does.
  const d = mockLootDetail(data, true, 'areas', ACE_GEAR);
  assert.deepEqual(
    d.groups.map((g) => g.label),
    ['Undersuit', 'Helmet', 'Torso', 'Primary weapon'],
  );
  assert.ok(d.groups.flatMap((g) => g.rows ?? []).every((r) => r.kind === 'worn'));
  assert.equal(d.npc?.body_loot?.id, 'npc:LootTable_Actor_AcePilot');
  assert.equal(d.npc?.faction, 'Criminals');
  // The body-loot row links back to the worn gear.
  const body = mockLootDetail(data, true, 'areas', 'npc:LootTable_Actor_AcePilot');
  assert.deepEqual(
    body.npc?.worn_gear.map((l) => l.id),
    [ACE_GEAR],
  );
  // A filter word of a note reaches the gated row.
  const grunt = gear.find((r) => r.name === 'Grunt 01')!;
  const f = mockLootDetail(data, true, 'areas', grunt.id, 'npcclass');
  assert.equal(f.total, 1);
  // By item: worn by, under the worn-gear kind.
  const helmet = mockLootDetail(data, true, 'items', 'syn_armour_helmet_night');
  const worn = helmet.groups.find((g) => g.label === GEAR_KIND);
  assert.ok(worn, JSON.stringify(helmet.groups.map((g) => g.label)));
  assert.equal(worn.rows?.[0]?.name, 'Ace pilot');
});
