/**
 * What a returned ApplyReport means for the player. M1 returns guards and refusals as a report
 * (`skipped` + `reason`, `needs-decision`, ...), not as an error, so every caller of apply,
 * remove, undo and restore goes through here instead of toasting "done" for any report.
 * Imports types only, so `node --test` can load it without a bundler (tests/outcome.test.ts).
 */
import type { ApplyReport } from './contract/types';

export type Action = 'apply' | 'remove' | 'undo' | 'restore';
export type OutcomeTone = 'info' | 'success' | 'warning' | 'error';

export interface Outcome {
  tone: OutcomeTone;
  title: string;
  detail: string | null;
  /** The game file was written (or removed). */
  wrote: boolean;
  /** The action did what was asked (dry run and up-to-date count as done). */
  done: boolean;
}

const int = (n: number) => n.toLocaleString('en-US');
const plural = (n: number, w: string) => `${int(n)} ${w}${n === 1 ? '' : 's'}`;

const NOT_DONE: Record<Action, (c: string) => string> = {
  apply: (c) => `Not applied to ${c}`,
  remove: (c) => `Changes not removed from ${c}`,
  undo: (c) => `Last apply on ${c} not undone`,
  restore: (c) => `Backup not restored on ${c}`,
};

const FAILED: Record<Action, (c: string) => string> = {
  apply: (c) => `Apply to ${c} failed`,
  remove: (c) => `Remove on ${c} failed`,
  undo: (c) => `Undo on ${c} failed`,
  restore: (c) => `Restore on ${c} failed`,
};

function join(...parts: (string | null | undefined)[]): string | null {
  const s = parts.filter(Boolean).join(' ');
  return s || null;
}

export function describeOutcome(r: ApplyReport, action: Action): Outcome {
  const c = r.channel;
  const warn = r.warnings.length;
  const recovered = r.recovered ? `${r.recovered}` : null;
  switch (r.result) {
    case 'dry-run':
      return {
        tone: 'info',
        title: `Dry run for ${c}: nothing written`,
        detail: join(`${plural(r.counts.changed, 'string')} would change${warn ? `, ${plural(warn, 'warning')}` : ''}.`, warn ? r.warnings[0]?.detail : null),
        wrote: false,
        done: true,
      };
    case 'applied':
      return warn
        ? { tone: 'warning', title: `Applied to ${c} with ${plural(warn, 'warning')}`, detail: join(r.warnings[0]?.detail, recovered), wrote: true, done: true }
        : { tone: 'success', title: `Applied to ${c}`, detail: join(`${plural(r.counts.changed, 'string')} changed.`, recovered), wrote: true, done: true };
    case 'up-to-date':
      return { tone: 'info', title: `${c} is already up to date`, detail: join('Nothing was written.', recovered), wrote: false, done: true };
    case 'removed':
      return { tone: 'success', title: `Removed StarOrg changes from ${c}`, detail: 'The game shows its stock text again.', wrote: true, done: true };
    case 'restored':
      return { tone: 'success', title: action === 'undo' ? `Undid the last apply on ${c}` : `Restored a backup on ${c}`, detail: r.backup ? `Backup ${r.backup} is in place.` : null, wrote: true, done: true };
    case 'nothing-to-do':
      return { tone: 'info', title: action === 'remove' ? 'Nothing to remove' : `Nothing to do on ${c}`, detail: 'The game file was not touched.', wrote: false, done: true };
    case 'skipped':
      return { tone: 'warning', title: NOT_DONE[action](c), detail: r.reason ?? 'A safety check stopped it before anything was written.', wrote: false, done: false };
    case 'needs-setup':
      return { tone: 'warning', title: `${c} is not set up`, detail: r.reason ?? 'Run setup for this channel first.', wrote: false, done: false };
    case 'needs-decision':
      return { tone: 'warning', title: `${NOT_DONE[action](c)}: needs your decision`, detail: r.reason, wrote: false, done: false };
    case 'failed':
      return { tone: 'error', title: FAILED[action](c), detail: join(r.reason ?? 'Nothing was written.', recovered), wrote: false, done: false };
  }
}
