import type { ApplyState, BackupReason, Build, JobKind, PatchState, StringSource } from './contract/types';
import { staleWords } from './messages';

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
        : { label: 'Re-apply needed', tone: 'warning', hint: `Changed since the last apply: ${staleWords(s.reasons)}.` };
    case 'changed-by-another-tool':
      return { label: s.tool ? `Changed by ${s.tool}` : 'Changed by another tool', tone: 'info', hint: 'A text file from another tool is in place.' };
    case 'not-applied':
      return { label: 'Not applied', tone: 'muted', hint: 'The game shows its stock text.' };
  }
}

export function fmtBytes(n: number | null | undefined): string {
  if (n == null) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(1)} MiB`;
}

export const fmtMs = (ms: number) => (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`);

/** Same build = same change number and branch (addendum §1), never by ordering versions. */
export function sameBuild(a: Build | null | undefined, b: Build | null | undefined): boolean {
  if (!a || !b) return false;
  return a.change === b.change && a.branch === b.branch;
}

export interface ToneView {
  label: string;
  tone: StatusTone;
  hint: string;
  /** On, but not written on this build (the switch reads "On, skipped"). */
  skipped?: boolean;
}

/** Patch states in words (spec 4.5); mismatch states are warnings, never colour alone. */
export function patchStateView(s: PatchState, enabled: boolean): ToneView {
  if (!enabled && s !== 'retired') return { label: 'Off', tone: 'muted', hint: 'You turned this patch off.' };
  switch (s) {
    case 'applies':
      return { label: 'Applied', tone: 'success', hint: 'The stock text matches; the fix is written on apply.' };
    case 'not-needed':
      return { label: 'Not needed', tone: 'muted', hint: 'The game text already reads the way the patch would make it.', skipped: true };
    case 'nothing-to-add':
      return { label: 'Nothing to add', tone: 'muted', hint: 'The text the patch adds is already there.', skipped: true };
    case 'target-changed':
      return { label: 'Mismatch', tone: 'warning', hint: 'The stock text changed in this build; the patch is skipped and reported.', skipped: true };
    case 'missing':
      return { label: 'Key missing', tone: 'warning', hint: 'The key is not in this build.', skipped: true };
    case 'out-of-range':
      return { label: 'Other build', tone: 'muted', hint: 'Written for a different game version range.', skipped: true };
    case 'conflict':
      return { label: 'Conflict', tone: 'destructive', hint: 'Another patch changes the same key.', skipped: true };
    case 'disabled':
      return { label: 'Off', tone: 'muted', hint: 'You turned this patch off.' };
    case 'no-data':
      return { label: 'No data', tone: 'muted', hint: 'This game data source has no records of this kind, so nothing could be checked.', skipped: true };
    case 'retired':
      return { label: 'Retired', tone: 'muted', hint: 'Fixed upstream; kept for older builds.', skipped: true };
  }
}

export const SOURCE_LABEL: Record<StringSource, string> = { unmodified: 'Unmodified', enhanced: 'Enhanced', patched: 'Patched', edited: 'Edited' };
export const SOURCE_TONE: Record<StringSource, StatusTone> = { unmodified: 'muted', enhanced: 'success', patched: 'info', edited: 'info' };

export const CATEGORY_LABEL: Record<string, string> = {
  ships: 'Ships',
  components: 'Components',
  'ship-weapons': 'Ship weapons',
  missiles: 'Missiles',
  'fps-weapons': 'FPS weapons',
  missions: 'Missions',
  items: 'Items',
  ui: 'Interface',
};

export const JOB_LABEL: Record<JobKind, string> = {
  discover: 'Discover',
  extract: 'Game data',
  apply: 'Apply',
  remove: 'Remove',
  restore: 'Restore',
  import: 'Import',
  sync: 'StarOrg sync',
  dev: 'Developer',
  hook: 'Launch hook',
  mcp: 'MCP server',
  uex: 'UEX market data',
};

export const BACKUP_REASON: Record<BackupReason, string> = {
  'pre-existing': 'From before StarOrg Codex',
  'before-apply': 'Before an apply',
  'before-restore': 'Before a restore',
  manual: 'Backed up by you',
};
