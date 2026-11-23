/**
 * Where to get (spec obtain-hints): the Enhancements tab's mode examples, the UEX file age
 * and the mock preview samples. The samples follow `starorg_core::enhance::obtain::block`
 * over the same synthetic sources as `mock::obtain_preview` in Rust (a test compares them
 * with samples.gen.json), so mode, shops, sell prices and line switches show as they will
 * in game.
 */
import type { ObtainMode, ObtainStatus, PreviewItem, Profile, UexTokenStatus } from './contract/types';

/** One example per mode, as the block starts in game (labels without colour). */
export const OBTAIN_MODES: { id: ObtainMode; label: string; example: string }[] = [
  { id: 'short', label: 'Short', example: 'Get: Loot (Rare) · Craft [BP] · Reward' },
  { id: 'normal', label: 'Normal', example: 'WHERE TO GET, then a line per source: Loot: Weapons rack (Rare) at Bunkers, Locker (Common) at Caves +3' },
  { id: 'detailed', label: 'Detailed', example: 'Adds odds per box, systems, craft time and quality spread: Loot: Weapons rack (Rare) at Bunkers · 21%  |  …' },
];

/** `today`, `1 day old`, `12 days old` for an RFC 3339 time; null when unknown. */
export function fileAge(iso: string | null | undefined, now: Date = new Date()): string | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  const days = Math.floor((now.getTime() - t) / 86_400_000);
  if (days <= 0) return 'today';
  return days === 1 ? '1 day old' : `${days} days old`;
}

const UEX_LABEL: Record<string, string> = {
  'uex-item-prices.json': 'Item prices',
  'uex-commodity-prices.json': 'Commodity prices',
  'uex-terminals.json': 'Terminals',
};

/** `Item prices today · Commodity prices missing · Terminals today`: each UEX file's age, for Settings and the Shops card. */
export function uexFilesText(s: ObtainStatus, now: Date = new Date()): string {
  return s.files.map((f) => `${UEX_LABEL[f.name] ?? f.name} ${f.present ? (fileAge(f.modified, now) ?? 'present') : 'missing'}`).join(' · ');
}

/** Where the UEX token is, never its value. */
export function uexTokenText(t: UexTokenStatus | null): string {
  if (!t) return 'Token status unknown';
  if (!t.set) return 'No token saved: the endpoints are public, so none is needed';
  return `Token saved (${t.store ?? 'keyring'})`;
}

/**
 * A price rounded to two significant digits with k / M, as the sell line writes it
 * (`starorg_core::enhance::obtain::rough_price`): `~6.2k`, `~12k`, `~1.5M`, `~850`.
 */
export function roughPrice(v: number): string {
  if (!Number.isFinite(v) || v <= 0) return '~0';
  const step = 10 ** (Math.floor(Math.log10(v)) - 1);
  const r = Math.round(v / step) * step;
  const [n, unit] = r >= 1e6 ? [r / 1e6, 'M'] : r >= 1e3 ? [r / 1e3, 'k'] : [r, ''];
  const digits = n >= 10 ? String(Math.round(n)) : n >= 1 ? String(Number(n.toFixed(1))) : String(Number(n.toPrecision(2)));
  return `~${digits}${unit}`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** `21 Nov` for `2026-11-21T09:00:00Z` (the date as written, no time zone); null when unreadable. */
export function shortDay(iso: string | null | undefined): string | null {
  const m = /^\d{4}-(\d{2})-(\d{2})/.exec(iso ?? '');
  if (!m) return null;
  const month = MONTHS[Number(m[1]) - 1];
  const day = Number(m[2]);
  return month && day >= 1 && day <= 31 ? `${day} ${month}` : null;
}

/** When the mock's "Fetch now" wrote the UEX files (`mock::MOCK_UEX_FETCHED`). */
const MOCK_FETCHED = '2026-11-21T09:00:00Z';

const TOP = 3;
const SEP = '  |  ';
const em = (n: number, t: string) => (n === 0 ? t : `<EM${n}>${t}</EM${n}>`);
const int = (v: number) => Math.round(v).toLocaleString('en-US');

/** The first TOP joined with `sep`, then `+N` for the rest and `extra`. */
function top(items: string[], extra: number, sep: string): string {
  let s = items.slice(0, TOP).join(sep);
  const more = Math.max(0, items.length - TOP) + extra;
  if (more > 0) s += `${sep === ', ' ? ' ' : sep}+${more}`;
  return s;
}

type Stats = (prose: string, lines: string[]) => string;

/** The synthetic rifle (after its stats block) and ore, `[]` when `obtain` is off. */
export function obtainPreview(p: Profile, stats: Stats): PreviewItem[] {
  if (!p.obtain) return [];
  const on = (line: string) => !p.lines_off.includes(`obtain.${line}`);
  const detailed = p.obtain_mode === 'detailed';
  const short = p.obtain_mode === 'short';
  const label = (t: string) => em(p.mission.label_em, `${t}:`);
  const sep = detailed ? SEP : ', ';
  const finish = (parts: string[], lines: string[]) => {
    if (short) return parts.length ? `${label('Get')} ${parts.join(' · ')}` : null;
    return lines.length ? `${em(p.mission.header_em, 'WHERE TO GET')}\\n${lines.join('\\n')}` : null;
  };
  const out: PreviewItem[] = [];

  // Rifle: two shops, four of six loot containers, a blueprint and a contract reward.
  const shops = p.obtain_shops && on('shops') ? [['Center Mass', 'Area18'], ['Skutters', 'GrimHEX']].map(([t, place]) => `${t} (${place}${detailed ? ', Stanton' : ''})`) : [];
  const boxes: [string, string, string, number][] = [
    ['Weapons rack', 'Rare', 'Bunkers', 21],
    ['Weapon crate', 'Uncommon', 'Derelict outposts', 6],
    ['Locker', 'Common', 'Caves', 2],
    ['Footlocker', 'Common', 'Distribution centres', 1],
  ];
  const loot = on('loot') ? boxes.map(([c, r, at, o]) => `${c} (${r}) at ${at}${detailed ? ` · ${o}%` : ''}`) : [];
  const craft = on('craft');
  const rewards = on('rewards') ? ['Synthetic Recovery [BP]'] : [];
  const parts: string[] = [];
  const lines: string[] = [];
  if (shops.length) {
    parts.push('Buy');
    lines.push(`${label('Buy')} ${top(shops, 0, sep)}`);
  }
  if (loot.length) {
    parts.push('Loot (Rare)');
    lines.push(`${label('Loot')} ${top(loot, 2, sep)}`);
  }
  if (craft) {
    parts.push('Craft [BP]');
    lines.push(`${label('Craft')} from blueprint`);
  }
  if (rewards.length) {
    parts.push('Reward');
    lines.push(`${label('Reward')} ${top(rewards, 0, ', ')}`);
  }
  const gun = finish(parts, lines);
  if (gun) {
    const prose = 'A synthetic laser rifle used for testing.';
    const stat: [string, string][] = [
      ['fps_weapons.damage', 'Damage: 18 energy  |  DPS: 150'],
      ['fps_weapons.rate', 'Fire rate: 500 rpm'],
      ['fps_weapons.magazine', 'Magazine: 45'],
    ];
    const sl = p.fps_weapons ? stat.filter(([k]) => !p.lines_off.includes(k)).map(([, l]) => l) : [];
    out.push({ key: 'item_DescSYN_Rifle', label: 'Weapon (where to get)', category: 'obtain-hints', original: prose, result: `${stats(prose, sl)}\\n\\n${gun}` });
  }

  // Ore: four kiosks that buy it, four ship-mined places and a quality spread with one
  // place override.
  const kiosks: [string, string, string, number][] = [
    ['HUR-L2', 'HUR-L2 Faithful Dream Station', 'Stanton', 9612],
    ["Rod's Fuel", "Rod's Fuel 'N Supplies", 'Pyro', 9180],
    ['TDD Area 18', 'Area 18', 'Stanton', 9000],
    ['Ruin Station', 'Pyro', 'Pyro', 8700],
  ];
  const spots: [string, string, number, number][] = [
    ['Lyria', 'Stanton', 12, 30],
    ['Aaron Halo', 'Stanton', 8, 22],
    ['Pyro I', 'Pyro', 5, 18],
    ['Wala', 'Stanton', 4, 10],
  ];
  const oreParts: string[] = [];
  const oreLines: string[] = [];
  if (p.obtain_prices && on('sell')) {
    const key = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, '');
    const where = ([t, place, sys]: [string, string, string, number]) => {
      const inner = [...new Set([place, sys].filter((x) => !key(t).includes(key(x))))];
      return detailed ? `${t} (${inner.join(', ')})` : t;
    };
    const entries = kiosks.map((k, i) => `${roughPrice(k[3])}${i === 0 ? ' aUEC/SCU' : ''} at ${where(k)}`);
    const stamp = `(UEX, ${shortDay(MOCK_FETCHED)})`;
    oreParts.push(`Sell ${roughPrice(kiosks[0]![3])} aUEC/SCU at ${kiosks[0]![0]} ${stamp}`);
    oreLines.push(`${label('Sell')} ${top(entries, 0, sep)} ${stamp}`);
  }
  if (on('mined')) {
    const places = spots.map(([pl]) => pl);
    oreParts.push(`Mined (ship) at ${top(places, 0, ', ')}`);
    const s = detailed ? top(spots.map(([pl, sys, lo, hi]) => `${pl}, ${sys} (ship) ${lo}–${hi}%`), 0, sep) : `${top(places, 0, ', ')} (ship)`;
    oreLines.push(`${label('Mined')} ${s}`);
  }
  if (on('quality')) {
    oreParts.push('Quality most near 500');
    oreLines.push(`${label('Quality')} 1–${int(1000)}, most near 500${detailed ? ` ± 150; Aaron Halo 300–${int(1000)}` : ''}`);
  }
  const ore = finish(oreParts, oreLines);
  if (ore) {
    const prose = 'A synthetic unstable ore.';
    out.push({ key: 'items_commodities_SYN_ore_desc', label: 'Ore (where to get)', category: 'obtain-hints', original: prose, result: `${prose}\\n\\n${ore}` });
  }
  return out;
}
