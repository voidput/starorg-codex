// The Explorer record's Readable view: sections and rows from the record JSON, humanized keys,
// values as a player reads them, empty values hidden, search, and the per-viewer switch. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { SHEET_MODE_KEY, buildSheet, humanize, humanizeType, isNoiseKey, loadSheetMode, numberText, rowCount, saveSheetMode, viewSheet, type SheetGroup } from '../src/lib/recordsheet.ts';
import { splice, type Json } from '../src/lib/jsontree.ts';
import type { RecordView } from '../src/lib/contract/dev.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const mock = JSON.parse(readFileSync(new URL('../src/lib/contract/mock-dev.gen.json', import.meta.url), 'utf8'));
const fixtures: Json[] = JSON.parse(readFileSync(new URL('../../crates/starorg-data/tests/fixtures/records.json', import.meta.url), 'utf8'));

const find = (gs: SheetGroup[], title: string): SheetGroup | undefined => {
  for (const g of gs) {
    if (g.title === title) return g;
    const s = find(g.groups, title);
    if (s) return s;
  }
  return undefined;
};

test('keys and types read as words, acronyms stay upper case', () => {
  assert.equal(humanize('maxHealth'), 'Max health');
  assert.equal(humanize('SCMSpeed'), 'SCM speed');
  assert.equal(humanize('Scm'), 'SCM');
  assert.equal(humanize('damage_drop_min'), 'Damage drop min');
  assert.equal(humanize('DamagePhysical'), 'Damage physical');
  assert.equal(humanize('emSignature'), 'EM signature');
  assert.equal(humanize('maxQtFuel2'), 'Max QT fuel 2');
  assert.equal(humanize('_Type_'), 'Type');
  assert.equal(humanizeType('SAttachableComponentParams'), 'Attachable component');
  assert.equal(humanizeType('EntityClassDefinition'), 'Entity class');
  assert.equal(humanizeType('SCItemPurchasableParams'), 'SC item purchasable');
  assert.equal(humanizeType('Params'), 'Params');
  assert.ok(isNoiseKey('_Type_') && isNoiseKey('_Pointer_') && isNoiseKey('__meta') && !isNoiseKey('_cut_like'));
});

test('numbers lose float noise and gain their unit', () => {
  assert.equal(numberText(0.30000001, 'lifetime'), '0.3');
  assert.equal(numberText(25740, 'health'), '25,740 HP');
  assert.equal(numberText(151, 'Scm'), '151 m/s');
  assert.equal(numberText(1.5, 'speedMultiplier'), '1.5');
  assert.equal(numberText(0.0001, 'x'), '0.0001');
  assert.equal(numberText(1234.56789, 'Mass'), '1,234.568 kg');
});

test('the sample record reads as General, Speed and Components with references as links', () => {
  const r: RecordView = samples.dev_record;
  const s = buildSheet(JSON.parse(r.json), { labels: r.labels, loc: r.loc });
  assert.deepEqual(
    s.map((g) => g.title),
    ['General', 'Speed', 'Components'],
  );
  const general = s[0]!;
  assert.equal(general.subtitle, 'Syn ship');
  assert.ok(!general.rows.some((x) => x.key.startsWith('_')));
  const mass = general.rows.find((x) => x.key === 'Mass')!;
  assert.equal(mass.value.text, '10,250 kg');
  assert.equal(mass.pointer, '/_RecordValue_/Mass');
  const desc = general.rows.find((x) => x.key === 'Description')!;
  assert.equal(desc.value.kind, 'loc');
  const speed = find(s, 'Speed')!;
  assert.equal(speed.rows.find((x) => x.key === 'Scm')!.label, 'SCM');
  const boost = find(s, 'Boost')!;
  assert.deepEqual(boost.trail, ['Speed']);
  assert.equal(boost.depth, 1);
  const comps = find(s, 'Components')!;
  assert.equal(comps.rows.length, 3);
  const first = comps.rows[0]!;
  assert.equal(first.value.kind, 'ref');
  assert.equal(first.value.text, 'Sample Cooler 41');
  assert.equal(first.raw, '52000000-0000-4000-8000-000000000005');
  assert.ok(first.value.kind === 'ref' && first.value.follow?.guid === first.raw);
});

test('arrays of typed objects become a group per element, titled by their type or name', () => {
  const root: Json = {
    _RecordName_: 'X',
    _RecordValue_: {
      _Type_: 'EntityClassDefinition',
      Components: [
        { _Type_: 'SAttachableComponentParams', AttachDef: { _Type_: 'SItemDefinition', Size: 1, Manufacturer: { _ref: '00000000-0000-0000-0000-000000000000', name: 'x', type: null, path: null } } },
        { name: 'Port A', Health: 0.30000001 },
        { other: true },
      ],
      Tags: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'],
      Empty: [],
      Nothing: null,
      Blank: '',
      NoText: '@LOC_EMPTY',
      Zero: '00000000-0000-0000-0000-000000000000',
    },
  };
  const s = buildSheet(root);
  assert.deepEqual(
    s.map((g) => g.title),
    ['General', 'Attachable component', 'Port A', 'Components #3'],
  );
  assert.equal(s[1]!.subtitle, 'Components #1');
  assert.equal(s[1]!.id, '/_RecordValue_/Components/0');
  const attach = find(s, 'Attach def')!;
  assert.equal(attach.subtitle, 'Item');
  assert.equal(attach.rows.find((x) => x.key === 'Manufacturer')!.empty, true);
  assert.equal(s[2]!.rows[0]!.value.text, 'Port A');
  assert.equal(s[2]!.rows[1]!.value.text, '0.3 HP');
  assert.equal(s[3]!.rows[0]!.value.text, 'Yes');
  const tags = s[0]!.rows.find((x) => x.key === 'Tags')!;
  assert.ok(tags.value.kind === 'list' && tags.value.items.length === 8 && tags.value.more === 2);
  // Empty values hide by default and are counted.
  const v = viewSheet(s, '', false);
  const g = v.groups[0]!;
  assert.deepEqual(
    g.rows.map((x) => x.key),
    ['Tags'],
  );
  assert.equal(v.hidden, 6);
  assert.ok(!find(v.groups, 'Attach def')!.rows.some((x) => x.key === 'Manufacturer'));
  assert.equal(viewSheet(s, '', true).hidden, 0);
  // Nothing dropped: the groups come back as built.
  assert.equal(viewSheet(s, '', true).groups[1], s[1]);
});

test('find keeps rows whose label, key, value or titles above them match every word', () => {
  const r: RecordView = samples.dev_record;
  const s = buildSheet(JSON.parse(r.json), { labels: r.labels, loc: r.loc });
  const v = viewSheet(s, 'boost forward', false);
  assert.deepEqual(
    v.groups.map((g) => g.title),
    ['Speed'],
  );
  assert.equal(v.groups[0]!.rows.length, 0);
  assert.deepEqual(
    v.groups[0]!.groups[0]!.rows.map((x) => x.key),
    ['Forward'],
  );
  assert.deepEqual(
    viewSheet(s, 'cooler', false).groups.map((g) => g.title),
    ['Components'],
  );
  assert.equal(viewSheet(s, 'nothing-matches-this', false).groups.length, 0);
  assert.equal(rowCount(s[1]!), 4);
});

test('a cut part is a group that loads, and splices in like the raw tree', () => {
  const r: RecordView = samples.dev_record_cut;
  let root: Json = JSON.parse(r.json);
  const s = buildSheet(root, { labels: r.labels, loc: r.loc });
  const speed = find(s, 'Speed')!;
  assert.deepEqual(speed.cut, { kind: 'object', len: 3 });
  assert.equal(speed.id, '/_RecordValue_/Speed');
  assert.equal(find(s, 'Loadout')!.cut?.len, 3400);
  root = splice(root, speed.id, { Scm: 150, Max: 900, Boost: { Forward: 1.5 } });
  const after = find(buildSheet(root, { labels: r.labels, loc: r.loc }), 'Speed')!;
  assert.equal(after.cut, null);
  assert.equal(after.rows.length, 2);
  // A record whose JSON is only a reference (a part opened by pointer) still reads.
  const p: RecordView = samples.dev_record_pointer;
  const ps = buildSheet(JSON.parse(p.json), { labels: p.labels, loc: p.loc });
  assert.equal(ps[0]!.rows[0]!.value.text, 'Synthetic hull 4');
});

test('every sample, mock and fixture record builds without throwing', () => {
  const all: Json[] = [...fixtures, ...(Array.isArray(mock.values) ? mock.values : [])];
  for (const v of Object.values(samples) as unknown[]) {
    if (v && typeof v === 'object' && 'json' in v && typeof (v as { json: unknown }).json === 'string') all.push(JSON.parse((v as { json: string }).json));
  }
  assert.ok(all.length > 10);
  for (const j of all) {
    const s = buildSheet(j);
    viewSheet(s, 'a', false);
    viewSheet(s, '', true);
  }
  for (const j of [null, 1, 'x', [], {}, [1, [2]], { _RecordValue_: null }] as Json[]) buildSheet(j);
});

test('the view switch is kept per viewer and survives blocked storage', () => {
  const m = new Map<string, string>();
  const s = { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
  assert.equal(loadSheetMode(s), 'readable');
  saveSheetMode(s, 'raw');
  assert.equal(m.get(SHEET_MODE_KEY), 'raw');
  assert.equal(loadSheetMode(s), 'raw');
  const blocked = {
    getItem(): string | null {
      throw new Error('blocked');
    },
    setItem(): void {
      throw new Error('blocked');
    },
  };
  assert.equal(loadSheetMode(blocked), 'readable');
  saveSheetMode(blocked, 'raw');
  assert.equal(loadSheetMode(null), 'readable');
});
