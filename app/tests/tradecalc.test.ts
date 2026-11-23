// The Commodities buy/sell calculator: the math (rounding, zero and empty amounts, a missing
// kiosk side, caps), the amount field, kiosk picks, the fleet's ships and the remembered prefs.
// Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fleetShips, kioskKey, kioskLabel, marginText, parseCalcPrefs, parseScu, pickKiosk, shipLabel, signedAuec, tradeCalc } from '../src/lib/tradecalc.ts';
import type { CommodityDetail, FleetView, KioskPrice } from '../src/lib/contract/types.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));

test('totals, profit per SCU and margin, rounded', () => {
  const r = tradeCalc({ scu: 96, buy: 1234.4, sell: 1500.55 });
  assert.equal(r.scu, 96);
  assert.equal(r.cost, 118502); // 118,502.4
  assert.equal(r.revenue, 144053); // 144,052.8
  assert.equal(r.profit, 25551);
  assert.equal(r.perScu, 266.15);
  assert.equal(r.margin, 21.6);
  assert.equal(r.cappedBy, null);
  // A loss reads negative.
  const loss = tradeCalc({ scu: 10, buy: 200, sell: 150 });
  assert.equal(loss.profit, -500);
  assert.equal(loss.perScu, -50);
  assert.equal(loss.margin, -25);
  // Fractional SCU (small boxes).
  assert.equal(tradeCalc({ scu: 0.125, buy: 100, sell: 120 }).revenue, 15);
});

test('zero, empty and bad amounts give no totals', () => {
  for (const scu of [null, 0, -5, Number.NaN, Number.POSITIVE_INFINITY]) {
    const r = tradeCalc({ scu, buy: 100, sell: 120 });
    assert.equal(r.scu, 0, String(scu));
    assert.equal(r.requested, 0);
    assert.deepEqual([r.cost, r.revenue, r.profit, r.perScu, r.margin], [null, null, null, null, null]);
  }
});

test('a missing kiosk side leaves its totals and the profit empty', () => {
  const noBuy = tradeCalc({ scu: 10, buy: null, sell: 120 });
  assert.equal(noBuy.cost, null);
  assert.equal(noBuy.revenue, 1200);
  assert.deepEqual([noBuy.profit, noBuy.perScu, noBuy.margin], [null, null, null]);
  const noSell = tradeCalc({ scu: 10, buy: 100, sell: 0 });
  assert.equal(noSell.cost, 1000);
  assert.equal(noSell.revenue, null);
  assert.equal(noSell.profit, null);
  const none = tradeCalc({ scu: 10, buy: null, sell: null });
  assert.deepEqual([none.cost, none.revenue], [null, null]);
});

test('stock and demand cap the amount and say which', () => {
  const s = tradeCalc({ scu: 100, buy: 10, sell: 12, stock: 40 });
  assert.deepEqual([s.scu, s.requested, s.cappedBy, s.cost, s.revenue], [40, 100, 'stock', 400, 480]);
  const d = tradeCalc({ scu: 100, buy: 10, sell: 12, stock: 80, demand: 25 });
  assert.deepEqual([d.scu, d.cappedBy], [25, 'demand']);
  // Under the limits, or no limit known: no cap.
  assert.equal(tradeCalc({ scu: 10, buy: 10, sell: 12, stock: 40, demand: 40 }).cappedBy, null);
  assert.equal(tradeCalc({ scu: 10, buy: 10, sell: 12, stock: null, demand: Number.NaN }).cappedBy, null);
  // Out of stock: capped to nothing.
  const out = tradeCalc({ scu: 10, buy: 10, sell: 12, stock: 0 });
  assert.deepEqual([out.scu, out.cappedBy, out.cost], [0, 'stock', null]);
  // A limit on a side without a price does not count.
  assert.equal(tradeCalc({ scu: 10, buy: null, sell: 12, stock: 1 }).cappedBy, null);
});

test('the amount field', () => {
  assert.equal(parseScu('96'), 96);
  assert.equal(parseScu(' 1,000 '), 1000);
  assert.equal(parseScu('0.5'), 0.5);
  assert.equal(parseScu('.25'), 0.25);
  assert.equal(parseScu('2.'), 2);
  assert.equal(parseScu('1.23456'), 1.235);
  for (const t of ['', '  ', '0', '-3', 'abc', '1e3', '1.2.3', null, undefined]) assert.equal(parseScu(t), null, String(t));
});

test('text helpers', () => {
  assert.equal(signedAuec(25551), '+25,551 aUEC');
  assert.equal(signedAuec(-500), '-500 aUEC');
  assert.equal(signedAuec(0), '0 aUEC');
  assert.equal(signedAuec(266.15, 2), '+266.15 aUEC');
  assert.equal(signedAuec(null), '–');
  assert.equal(marginText(21.6), '21.6%');
  assert.equal(marginText(-25), '-25%');
  assert.equal(marginText(null), '–');
});

const kiosk = (terminal: string, location: string, price: number): KioskPrice => ({ terminal, location, system: null, price, scu_stock: null, updated: null, stale: false });

test('kiosk picks default to the first row (cheapest buy, best sell)', () => {
  const rows = [kiosk('A', 'Here', 100), kiosk('B', 'There', 110), kiosk('A', 'There', 120)];
  assert.equal(pickKiosk(rows, null), rows[0]);
  assert.equal(pickKiosk(rows, kioskKey(rows[2]!)), rows[2]);
  assert.equal(pickKiosk(rows, 'gone'), rows[0]);
  assert.equal(pickKiosk([], null), null);
  assert.equal(pickKiosk(null, 'x'), null);
  assert.notEqual(kioskKey(rows[0]!), kioskKey(rows[2]!));
  assert.equal(kioskLabel(rows[1]!), 'B · There · 110 aUEC/SCU');
  // The mock's priced details come best first on both sides.
  const priced = samples.commodity_details_all_priced as CommodityDetail[];
  const buy = priced.find((d) => d.market.buy.length > 1);
  const sell = priced.find((d) => d.market.sell.length > 1);
  assert.ok(buy && sell, 'priced samples with several kiosks');
  assert.ok(buy.market.buy.every((k) => pickKiosk(buy.market.buy, null)!.price <= k.price));
  assert.ok(sell.market.sell.every((k) => pickKiosk(sell.market.sell, null)!.price >= k.price));
});

test('the fleet: My ships in order, then owned ones, each once, cargo when known', () => {
  const fv = samples.fleet_view as FleetView;
  assert.ok(fv.catalog.some((h) => h.cargo_scu !== null), 'the mock catalog has cargo');
  assert.ok(fv.catalog.some((h) => h.cargo_scu === null), 'and hulls without');
  const hauler = fv.catalog.find((h) => h.cargo_scu !== null)!;
  const plain = fv.catalog.find((h) => h.cargo_scu === null)!;
  const view: FleetView = {
    ...fv,
    settings: { ...fv.settings, groups: [], ships: [{ hull: plain.hull, group: null, name: 'My one', short: null }, { hull: plain.hull.toUpperCase(), group: null, name: null, short: null }] },
    owned: [
      { hull: hauler.hull, count: 2, names: [] },
      { hull: plain.hull, count: 1, names: [] },
      { hull: 'vehicle_NameNOT_InCatalog', count: 1, names: [] },
    ],
  };
  const ships = fleetShips(view);
  assert.deepEqual(
    ships.map((s) => [s.hull, s.name, s.scu]),
    [
      [plain.hull, 'My one', null],
      [hauler.hull, hauler.name.trim(), hauler.cargo_scu],
      ['vehicle_NameNOT_InCatalog', 'vehicle_NameNOT_InCatalog', null],
    ],
  );
  assert.equal(shipLabel(ships[0]!), 'My one');
  assert.equal(shipLabel(ships[1]!), `${hauler.name.trim()} · ${hauler.cargo_scu!.toLocaleString('en-US')} SCU`);
  assert.deepEqual(fleetShips(null), []);
});

test('remembered prefs survive broken storage', () => {
  assert.deepEqual(parseCalcPrefs(null), { amount: '', hull: null });
  assert.deepEqual(parseCalcPrefs('{oops'), { amount: '', hull: null });
  assert.deepEqual(parseCalcPrefs('null'), { amount: '', hull: null });
  assert.deepEqual(parseCalcPrefs(JSON.stringify({ amount: '96', hull: 'vehicle_NameRDGE_Hauler' })), { amount: '96', hull: 'vehicle_NameRDGE_Hauler' });
  assert.deepEqual(parseCalcPrefs(JSON.stringify({ amount: 96, hull: '' })), { amount: '', hull: null });
});
