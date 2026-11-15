// Readable ids in Developer mode (addendum 7.14): the "Show IDs" switch, the label-first
// display with the raw id beside it, and the raw fallback. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { SHOW_IDS_KEY, changeTexts, groupChanges, idLike, loadShowIds, saveShowIds, shown, titled, valueText } from '../src/lib/readable.ts';
import { type Lookup, pathLabelIn } from '../src/lib/contract/mock-dev.ts';
import type { DevFieldDiff, Readable, RecordView, RowsPage } from '../src/lib/contract/dev.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

class MemStore {
  m = new Map<string, string>();
  getItem(k: string) {
    return this.m.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.m.set(k, v);
  }
}
const blocked = {
  getItem(): string | null {
    throw new Error('blocked');
  },
  setItem(): void {
    throw new Error('blocked');
  },
};

test('the Show IDs switch is kept per viewer and survives blocked storage', () => {
  const s = new MemStore();
  assert.equal(loadShowIds(s), false, 'off by default');
  saveShowIds(s, true);
  assert.equal(s.getItem(SHOW_IDS_KEY), '1');
  assert.equal(loadShowIds(s), true);
  saveShowIds(s, false);
  assert.equal(loadShowIds(s), false);
  assert.equal(loadShowIds(blocked), false);
  assert.doesNotThrow(() => saveShowIds(blocked, true));
  assert.equal(loadShowIds(null), false);
});

const bracer: Readable = { kind: 'record', text: 'Bracer', detail: 'COOL_AEGS_S01_Bracer_SCItem', enhanced: null };
const guid = '4ec3037b-4b6f-4837-b00d-cd9b4836a82a';

test('a label comes first with the raw id beside it; Show IDs switches back', () => {
  const s = shown(guid, bracer, false);
  assert.equal(s.primary, 'Bracer');
  assert.equal(s.secondary, guid);
  assert.equal(s.raw, false);
  assert.match(s.title, /COOL_AEGS_S01_Bracer_SCItem/);
  const raw = shown(guid, bracer, true);
  assert.equal(raw.primary, guid);
  assert.equal(raw.secondary, null);
  assert.equal(raw.raw, true);
  assert.match(raw.title, /= Bracer/, 'the name is still in the tooltip');
});

test('ids nothing reads fall back to the raw id, marked, never blank', () => {
  for (const raw of ['aaaaaaaa-0000-4000-8000-0000000000ff', '@item_NameNOPE']) {
    const s = shown(raw, null, false);
    assert.equal(s.primary, raw);
    assert.equal(s.unresolved, true);
    assert.match(s.title, /no readable name/);
  }
  // Plain text is not an id: no mark.
  assert.equal(shown('Sample Cooler 00', null, false).unresolved, false);
  assert.equal(shown('x', { ...bracer, text: '' }, false).primary, 'x', 'an empty label never blanks the cell');
  assert.ok(idLike(guid) && idLike('@LOC_EMPTY') && !idLike('AEGS'));
});

test('enhanced text is marked and in the tooltip', () => {
  const s = shown('@item_DescX', { kind: 'loc', text: 'A cooler.', detail: null, enhanced: 'A cooler.\\nGrade: C' }, false);
  assert.equal(s.enhanced, 'A cooler.\\nGrade: C');
  assert.match(s.title, /Enhanced: /);
});

test('field values: labels, numbers with units, JSON text, missing', () => {
  assert.equal(valueText(guid, bracer, null, false).primary, 'Bracer');
  assert.equal(valueText(guid, bracer, null, true).primary, JSON.stringify(guid));
  assert.equal(valueText(1500, null, 'm/s', false).primary, '1,500 m/s');
  assert.equal(valueText(1500, null, 'm/s', true).primary, '1500');
  assert.equal(valueText(2.12345, null, null, false).primary, '2.123');
  assert.equal(valueText('plain', null, null, false).primary, '"plain"');
  assert.equal(valueText(null, null, null, false).primary, '—');
  assert.equal(valueText(true, null, null, false).primary, 'true');
});

test('records are titled by display name; changes group by item', () => {
  assert.deepEqual(titled('COOL_AEGS_S01_Bracer_SCItem', 'Bracer', false), { primary: 'Bracer', secondary: 'COOL_AEGS_S01_Bracer_SCItem' });
  assert.deepEqual(titled('#3', null, false), { primary: '#3', secondary: null });
  assert.deepEqual(titled('X', 'Bracer', true), { primary: 'X', secondary: null });
  const g = groupChanges([{ group: null }, { group: 'loadout › Bracer' }, { group: 'loadout › Bracer' }, { group: 'stats' }]);
  assert.deepEqual(
    g.map((x) => [x.group, x.items.length]),
    [
      [null, 1],
      ['loadout › Bracer', 2],
      ['stats', 1],
    ],
  );
});

test('the contract samples carry readable fields the screens use', () => {
  const rec: RecordView = samples.dev_record;
  const own = rec.labels[rec.header.guid];
  assert.ok(own && own.kind === 'record' && own.text.length > 0);
  const rows: RowsPage = samples.dev_rows_readable;
  const at = rows.columns.indexOf('manufacturer');
  assert.equal(rows.rows[0]!.labels[at]!.kind, 'manufacturer');
  assert.equal(rows.rows[0]!.cells[at], 'SYNA', 'the raw cell stays');
  assert.ok(rows.rows.every((r) => r.title !== null));
  const ship: DevFieldDiff = samples.dev_diff_record_ship;
  assert.ok(ship.label);
  assert.equal(ship.changes[0]!.unit, 'm/s');
  assert.equal(samples.dev_diff_records_query.records[0].label.toLowerCase().includes('sample cooler 4'), true);
});

test('a key swapped for one with the same text shows the keys, the text secondary', () => {
  const calico: Readable = { kind: 'loc', text: 'Calico Core Desert', detail: null, enhanced: null };
  const c = { old: '@item_Name_calico_a', new: '@item_Name_calico_b', old_label: calico, new_label: calico, unit: null };
  const [o, n] = changeTexts(c, false);
  assert.equal(o.primary, '@item_Name_calico_a');
  assert.equal(n.primary, '@item_Name_calico_b');
  assert.equal(n.secondary, 'same text: Calico Core Desert');
  // Different text: the labels lead as usual.
  const [o2, n2] = changeTexts({ ...c, new_label: { ...calico, text: 'Calico Core Arctic' } }, false);
  assert.equal(o2.primary, 'Calico Core Desert');
  assert.equal(n2.primary, 'Calico Core Arctic');
});

test('positional paths name the element they point at (TS mock mirror)', () => {
  const none: Lookup = { byGuid: () => null, byName: () => null, stock: () => null, enhanced: () => null, bare: false };
  const row = { loadout: { weapons: [{ name: 'Left Gun', dps: 1 }, { name: 'Right Gun', dps: 2 }] } };
  assert.equal(pathLabelIn(none, '/loadout/weapons/1/dps', row), 'loadout › weapons › Right Gun › dps');
  assert.equal(pathLabelIn(none, '/a/0/b', { a: [{ b: 1 }] }), null);
});
