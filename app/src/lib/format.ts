import type { ApplyState, Build } from './contract/types';

/** Short build label: "4.10.0 · 13165946". */
export function buildLabel(b: Build): string {
  const v = b.game_version ?? 'unknown version';
  return b.change ? `${v} · ${b.change}` : v;
}

export function fmtDateTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

export const int = (n: number) => n.toLocaleString('en-US');

export type StatusTone = 'success' | 'warning' | 'info' | 'muted' | 'destructive';

/** Words and tone for a channel status (spec 9.4 Home): never colour alone (spec 9.6). */
export function statusView(s: ApplyState, setUp: boolean): { label: string; tone: StatusTone; hint: string } {
  if (!setUp && s.state !== 'changed-by-another-tool') return { label: 'Not set up', tone: 'muted', hint: 'Run setup to choose this channel.' };
  switch (s.state) {
    case 'applied':
      return { label: 'Applied', tone: 'success', hint: 'The game file matches your settings.' };
    case 'stale':
      return s.reasons.includes('build')
        ? { label: 'Game updated', tone: 'warning', hint: 'The game was patched; re-apply to match the new build.' }
        : { label: 'Re-apply needed', tone: 'warning', hint: `Changed since the last apply: ${s.reasons.join(', ')}.` };
    case 'changed-by-another-tool':
      return { label: s.tool ? `Changed by ${s.tool}` : 'Changed by another tool', tone: 'info', hint: 'A text file from another tool is in place.' };
    case 'not-applied':
      return { label: 'Not applied', tone: 'muted', hint: 'The game shows its stock text.' };
  }
}
