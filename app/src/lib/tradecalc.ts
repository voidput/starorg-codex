/**
 * The Commodities detail's buy/sell calculator: the math (cost, revenue, profit, profit per
 * SCU, margin), the amount field, the kiosk pickers' keys, the fleet's ships with their cargo
 * SCU, and the remembered amount and ship. Pure, so `npm test` covers it (tests/tradecalc.test.ts).
 *
 * Prices are UEX kiosk prices (aUEC per SCU, player-reported). Totals are rounded to whole
 * aUEC, profit per SCU to 2 decimals and the margin to 1 decimal.
 */
import type { FleetView, KioskPrice } from './contract/types';
import { fold, ordered } from './shipnames.ts';

export interface TradeInput {
  /** SCU wanted; null or not above 0 means no amount. */
  scu: number | null;
  /** aUEC per SCU the buy kiosk asks; null when no kiosk sells it. */
  buy: number | null;
  /** aUEC per SCU the sell kiosk pays; null when no kiosk buys it. */
  sell: number | null;
  /** SCU the buy kiosk has in stock, when the data says. */
  stock?: number | null;
  /** SCU the sell kiosk will still take, when the data says. */
  demand?: number | null;
}

export interface TradeResult {
  /** SCU the totals are for (after a cap); 0 when there is no amount. */
  scu: number;
  /** The amount asked for, before a cap. */
  requested: number;
  /** What cut the amount down, or null. */
  cappedBy: 'stock' | 'demand' | null;
  cost: number | null;
  revenue: number | null;
  profit: number | null;
  perScu: number | null;
  /** Profit as a share of the cost, percent. */
  margin: number | null;
}

const price = (v: number | null | undefined): number | null => (typeof v === 'number' && Number.isFinite(v) && v > 0 ? v : null);
const limit = (v: number | null | undefined): number | null => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : null);
const round = (v: number, digits: number) => {
  const f = 10 ** digits;
  return Math.round(v * f) / f;
};

/** The totals for an amount at one buy and one sell price; a side without a price leaves its totals null. */
export function tradeCalc(i: TradeInput): TradeResult {
  const requested = typeof i.scu === 'number' && Number.isFinite(i.scu) && i.scu > 0 ? i.scu : 0;
  const buy = price(i.buy);
  const sell = price(i.sell);
  let scu = requested;
  let cappedBy: TradeResult['cappedBy'] = null;
  const stock = buy !== null ? limit(i.stock) : null;
  const demand = sell !== null ? limit(i.demand) : null;
  if (stock !== null && scu > stock) {
    scu = stock;
    cappedBy = 'stock';
  }
  if (demand !== null && scu > demand) {
    scu = demand;
    cappedBy = 'demand';
  }
  const none: TradeResult = { scu, requested, cappedBy, cost: null, revenue: null, profit: null, perScu: null, margin: null };
  if (scu <= 0) return none;
  const cost = buy !== null ? Math.round(scu * buy) : null;
  const revenue = sell !== null ? Math.round(scu * sell) : null;
  const profit = cost !== null && revenue !== null ? revenue - cost : null;
  return {
    ...none,
    cost,
    revenue,
    profit,
    perScu: buy !== null && sell !== null ? round(sell - buy, 2) : null,
    margin: profit !== null && cost ? round((profit / cost) * 100, 1) : null,
  };
}

/** The amount field: "96", "1,000", "0.5" -> SCU (3 decimals at most); null when empty, not a number or not above 0. */
export function parseScu(text: string | null | undefined): number | null {
  const t = (text ?? '').replace(/[,\s_]/g, '');
  if (!t || !/^\d*\.?\d+$|^\d+\.$/.test(t)) return null;
  const v = round(Number(t), 3);
  return Number.isFinite(v) && v > 0 ? v : null;
}

/** "+12,345 aUEC", "-120 aUEC", "0 aUEC" (`digits` decimals at most); a dash when missing. */
export function signedAuec(v: number | null | undefined, digits = 0): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  const n = round(v, digits);
  return `${n > 0 ? '+' : n < 0 ? '-' : ''}${Math.abs(n).toLocaleString('en-US', { maximumFractionDigits: digits })} aUEC`;
}

/** "12.5%", "-3%"; a dash when missing. */
export function marginText(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '–';
  return `${Number(v.toFixed(1)).toLocaleString('en-US', { maximumFractionDigits: 1 })}%`;
}

/** A kiosk row's picker key: terminal and place (the backend keeps one row per pair). */
export const kioskKey = (k: Pick<KioskPrice, 'terminal' | 'location'>): string => `${k.terminal}\u0001${k.location}`;

/** The row `key` names, else the first (cheapest buy, best sell: the lists come best first); null when empty. */
export function pickKiosk(rows: KioskPrice[] | null | undefined, key: string | null): KioskPrice | null {
  const list = Array.isArray(rows) ? rows : [];
  return (key !== null ? list.find((k) => kioskKey(k) === key) : undefined) ?? list[0] ?? null;
}

/** "TDD Area 18 · Area 18, ArcCorp · 9,612 aUEC/SCU" for a picker option. */
export function kioskLabel(k: KioskPrice): string {
  return `${k.terminal} · ${k.location} · ${Math.round(k.price).toLocaleString('en-US')} aUEC/SCU${k.stale ? ' (old)' : ''}`;
}

export interface FleetShip {
  hull: string;
  name: string;
  /** Cargo grid SCU; null when the game data does not say. */
  scu: number | null;
}

/** The ships the Ships tab lists (My ships, in order), then owned ones it does not list; each once. */
export function fleetShips(v: FleetView | null | undefined): FleetShip[] {
  if (!v) return [];
  const byHull = new Map((Array.isArray(v.catalog) ? v.catalog : []).map((h) => [fold(h.hull), h]));
  const seen = new Set<string>();
  const out: FleetShip[] = [];
  const add = (hull: string, rename: string | null) => {
    const f = fold(hull);
    if (seen.has(f)) return;
    seen.add(f);
    const h = byHull.get(f);
    const scu = h?.cargo_scu;
    out.push({ hull, name: rename?.trim() || h?.name.trim() || hull, scu: typeof scu === 'number' && Number.isFinite(scu) && scu > 0 ? scu : null });
  };
  for (const p of v.settings ? ordered(v.settings) : []) add(p.entry.hull, p.entry.name);
  for (const o of Array.isArray(v.owned) ? v.owned : []) add(o.hull, null);
  return out;
}

/** "Hauler · 96 SCU", or the name alone when the cargo is not known. */
export const shipLabel = (s: FleetShip): string => (s.scu !== null ? `${s.name} · ${s.scu.toLocaleString('en-US')} SCU` : s.name);

/** What the calculator remembers per viewer (localStorage): the amount text and the ship. */
export interface CalcPrefs {
  amount: string;
  hull: string | null;
}

export const CALC_PREFS_KEY = 'commodity-calc';

/** Stored prefs, or the defaults for anything missing or broken. */
export function parseCalcPrefs(raw: string | null | undefined): CalcPrefs {
  const out: CalcPrefs = { amount: '', hull: null };
  if (!raw) return out;
  try {
    const v = JSON.parse(raw) as Partial<CalcPrefs> | null;
    if (v && typeof v.amount === 'string') out.amount = v.amount.slice(0, 32);
    if (v && typeof v.hull === 'string' && v.hull) out.hull = v.hull.slice(0, 200);
  } catch {
    // Broken JSON: the defaults.
  }
  return out;
}

export function loadCalcPrefs(): CalcPrefs {
  try {
    return parseCalcPrefs(localStorage.getItem(CALC_PREFS_KEY));
  } catch {
    return parseCalcPrefs(null);
  }
}

export function saveCalcPrefs(p: CalcPrefs): void {
  try {
    localStorage.setItem(CALC_PREFS_KEY, JSON.stringify(p));
  } catch {
    // Storage blocked or full: nothing is remembered.
  }
}
