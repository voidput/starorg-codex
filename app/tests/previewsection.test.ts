// The in-game preview shows the open Enhancements section's examples only
// (src/lib/previewsection.ts), against the Rust mock's samples. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isNameKey, sectionPreview, type Section } from '../src/lib/previewsection.ts';
import type { PreviewItem } from '../src/lib/contract/types.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };

const items = samples.build_preview as PreviewItem[];
const labels = (s: Section) => sectionPreview(items, s).map((i) => i.label);

test('every section maps to examples the mock gives', () => {
  assert.deepEqual(labels('ships'), ['Ship description']);
  assert.deepEqual(labels('components'), ['Ship component']);
  assert.deepEqual(labels('ship-weapons'), ['Ship weapon']);
  assert.deepEqual(labels('fps-weapons'), ['Personal weapon']);
  assert.deepEqual(labels('missiles'), ['Missile']);
  assert.deepEqual(labels('missions'), ['Mission title', 'Contract description']);
  assert.deepEqual(labels('name-tags'), ['Component name']);
  assert.deepEqual(labels('quest-items'), ['Quest item']);
  assert.deepEqual(labels('obtain'), ['Weapon (where to get)', 'Ore (where to get)']);
  // General: one stats-block example (its header line), a fixed missing name, plus the
  // colour test when on.
  assert.deepEqual(labels('general'), ['Ship description', 'Item name (missing in stock)']);
});

test('Blueprints puts the contract with a blueprint list first', () => {
  const [first, ...rest] = sectionPreview(items, 'blueprints');
  assert.match(first!.result, /BLUEPRINTS/);
  assert.match(first!.result, /\[Owned\]/);
  assert.ok(rest.every((i) => i.category === 'missions'));
});

test('a section with nothing to show is empty; null shows all', () => {
  assert.deepEqual(sectionPreview(items.filter((i) => i.category !== 'missiles'), 'missiles'), []);
  assert.deepEqual(sectionPreview(null, 'ships'), []);
  assert.equal(sectionPreview(items, null).length, items.length);
  const ui: PreviewItem = { key: 'mG_ContractsMgr_Select', label: 'Colour test', category: 'ui', original: 'a', result: 'b' };
  assert.deepEqual(sectionPreview([...items, ui], 'general').map((i) => i.label), ['Ship description', 'Item name (missing in stock)', 'Colour test']);
});

test('name keys as the Rust side reads them', () => {
  assert.ok(isNameKey('item_NameSYN_Cooler_S2'));
  assert.ok(isNameKey('item_NameA,P'));
  assert.ok(!isNameKey('item_DescSYN_Cooler_S2'));
  assert.ok(!isNameKey('items_commodities_ore_desc'));
});
