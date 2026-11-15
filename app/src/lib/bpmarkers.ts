/**
 * Blueprint markers: the profile options that mark blueprints in game ([Owned] / [Missing] on
 * the blueprint lines of a contract, [BP] / [BP Owned] after its title), summed up for the
 * Blueprints screen, and the order of the Enhancements preview on its Blueprints tab. Pure.
 */
import type { PreviewItem, Profile } from './contract/types';

/** An EM tag number as a word: 0 = plain text. EM3/EM4 are the looks verified in game. */
export function emWord(n: number): string {
  if (n === 0) return 'plain';
  if (n === 3) return 'underline';
  if (n === 4) return 'blue';
  return `EM${n}`;
}

export interface MarkerState {
  label: '[Owned]' | '[Missing]' | '[BP Owned]';
  on: boolean;
  /** "blue", "plain", "off", "off (contracts off)". */
  text: string;
}

/** What the game shows now for each marker, from a saved profile. */
export function markerSummary(p: Pick<Profile, 'missions' | 'mission'>): MarkerState[] {
  const m = p.mission;
  const why = !p.missions ? 'off (contracts off)' : 'off';
  const pool = p.missions && m.blueprints;
  const owned = pool && m.owned_marker;
  const missing = pool && m.missing_marker;
  const bpOwned = p.missions && m.title_blueprint && m.title_owned;
  const poolWhy = !p.missions ? why : !m.blueprints ? 'off (no blueprint list)' : 'off';
  return [
    { label: '[Owned]', on: owned, text: owned ? emWord(m.owned_em) + (m.owned_line && m.owned_em !== 0 ? ', whole line' : '') : poolWhy },
    { label: '[Missing]', on: missing, text: missing ? emWord(m.missing_em) : poolWhy },
    { label: '[BP Owned]', on: bpOwned, text: bpOwned ? emWord(m.title_em ? m.title_bp_em : 0) : why },
  ];
}

/** One line: "[Owned] blue · [Missing] off · [BP Owned] off". */
export const markerLine = (p: Pick<Profile, 'missions' | 'mission'>) =>
  markerSummary(p)
    .map((s) => `${s.label} ${s.text}`)
    .join(' · ');

const hasList = (it: PreviewItem) => it.category === 'missions' && /\[(Owned|Missing)\]|BLUEPRINTS/.test(it.result);

/**
 * Preview examples, the open tab's first. `blueprints` puts a contract with a blueprint list
 * first (its [Owned] / [Missing] lines), then the other contract examples.
 */
export function previewOrder(items: PreviewItem[] | null | undefined, focus: string | null): PreviewItem[] {
  if (!Array.isArray(items)) return [];
  const rank = (it: PreviewItem) => (focus === 'blueprints' ? (hasList(it) ? 2 : it.category === 'missions' ? 1 : 0) : Number(it.category === focus));
  return [...items].sort((a, b) => rank(b) - rank(a));
}
