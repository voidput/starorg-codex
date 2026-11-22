/**
 * Where to get (spec obtain-hints): the Enhancements tab's mode examples, the UEX file age
 * and the mock preview samples. The samples follow `starorg_core::enhance::obtain::block`
 * over the same synthetic sources as `mock::obtain_preview` in Rust (a test compares them
 * with samples.gen.json), so mode, shops and line switches show as they will in game.
 */
import type { ObtainMode, PreviewItem, Profile } from './contract/types';

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

  // Ore: four ship-mined places and a quality spread with one place override.
  const spots: [string, string, number, number][] = [
    ['Lyria', 'Stanton', 12, 30],
    ['Aaron Halo', 'Stanton', 8, 22],
    ['Pyro I', 'Pyro', 5, 18],
    ['Wala', 'Stanton', 4, 10],
  ];
  const oreParts: string[] = [];
  const oreLines: string[] = [];
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
