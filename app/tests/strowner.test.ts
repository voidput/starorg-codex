// Strings owners in words (the grid's String column). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ownerContext, ownerLabel, prettyKey, sharedLabel } from '../src/lib/strowner.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

test('prettyKey matches the core examples', () => {
  // Same table as `unknown_keys_have_no_owner_and_a_pretty_fallback` in owners_tests.rs.
  const cases: [string, string][] = [
    ['item_DescCOOL_AEGS_S01_Bracer', 'item desc · COOL_AEGS_S01_Bracer'],
    ['hud_law_TrespassArea', 'hud · law trespass area'],
    ['item_Name_987_jacket_03', 'item name · 987 jacket 03'],
    ['item_desc_FLBL_ANVL_Pisces', 'item desc · FLBL_ANVL_Pisces'],
    ['ui_CIDynamicZoomAbsDesc', 'ui · CI dynamic zoom abs desc'],
    ['item_Nameless_thing', 'item · nameless thing'],
    ['Pyro2', 'pyro2'],
    ['@area_name_int_transit,P', 'area name · int transit'],
    ['vehicle_Name', 'vehicle name'],
  ];
  for (const [k, want] of cases) assert.equal(prettyKey(k), want, k);
});

test('fallback rows from the Rust mock are prettyKey of their key', () => {
  const rows = samples.query_strings_by_owner.rows;
  assert.ok(rows.length > 0);
  for (const r of rows) {
    assert.equal(r.owner.kind, 'key');
    assert.equal(r.owner.name, prettyKey(r.key));
    assert.equal(ownerLabel(r.owner, r.key), r.owner.name);
    assert.equal(ownerContext(r.owner), '');
  }
});

test('owner labels name the field and the kind', () => {
  const o = { name: 'Bracer', field: 'description', kind: 'component', detail: 'Aegis Dynamics' } as const;
  assert.equal(ownerLabel(o, 'item_DescCOOL_AEGS_S01_Bracer'), 'Bracer · description');
  assert.equal(ownerContext(o), 'Ship component · Aegis Dynamics');
  assert.equal(ownerContext({ ...o, kind: 'named', detail: null }), '');
  assert.equal(ownerLabel(null, 'item_DescAB_Cd'), 'item desc · AB_Cd');
  assert.equal(sharedLabel(136, { ...o, field: 'career' }), '136 records · career');
  assert.equal(sharedLabel(1200, null), '1,200 records');
});

test('every page row of the samples carries an owner and a count', () => {
  for (const page of [samples.query_strings, samples.query_strings_by_size, samples.query_strings_by_owner]) {
    for (const r of page.rows) {
      assert.ok(r.owner, r.key);
      assert.equal(typeof r.owners, 'number');
    }
  }
  const d = samples.string_detail;
  assert.deepEqual(d.owners[0], d.row.owner);
});
