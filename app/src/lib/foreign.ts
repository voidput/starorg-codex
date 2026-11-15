/**
 * Another tool's global.ini in a channel (Smart Citizen, a community pack, ...): the words shown
 * before anything is written (wizard Channels + Review, the Home apply confirm) and after (the
 * Apply result). One place, so every screen says the same thing. Plain TS (tests/foreign.test.ts).
 */
import type { ChannelSummary, Warning } from './contract/types';

export interface ForeignChannel {
  channel: string;
  /** The tool's name when it was recognised; null = "another tool". */
  tool: string | null;
}

/** Whether this channel holds another tool's file now. */
export function isForeign(c: Pick<ChannelSummary, 'foreign_file' | 'status'>): boolean {
  return c.foreign_file || c.status.state === 'changed-by-another-tool';
}

export function toolName(tool: string | null): string {
  return tool?.trim() || 'Another tool';
}

export function foreignOf(c: Pick<ChannelSummary, 'channel' | 'foreign_file' | 'foreign_tool' | 'status'>): ForeignChannel | null {
  if (!isForeign(c)) return null;
  const tool = c.foreign_tool ?? (c.status.state === 'changed-by-another-tool' ? c.status.tool : null);
  return { channel: c.channel, tool };
}

/** Before writing: title and body of the per-channel callout. */
export function foreignBefore(f: ForeignChannel): { title: string; body: string; stop: string } {
  const t = toolName(f.tool);
  const its = f.tool ? `${t}'s` : "that tool's";
  return {
    title: `${t} has changed ${f.channel}'s game text`,
    body: `Setting up will back up that file and replace it with StarOrg Codex's text. You can restore it any time from Backups.`,
    stop: `Running both tools will make them overwrite each other — turn off ${its} auto-apply or patching for ${f.channel} (or uninstall it) first.`,
  };
}

/** The same callout on Home, where the action is an apply rather than setup. */
export function foreignBeforeApply(f: ForeignChannel): { title: string; body: string; stop: string } {
  const b = foreignBefore(f);
  return { ...b, body: b.body.replace('Setting up will', 'Applying will') };
}

/** Label of the acknowledgement that unlocks the write. */
export const ACK_LABEL = 'Back up and replace';

/** Key of what was acknowledged: a new foreign channel (or tool) needs a new tick. */
export function ackKey(fs: ForeignChannel[]): string {
  return fs
    .map((f) => `${f.channel}:${f.tool ?? ''}`)
    .sort()
    .join(',');
}

/** The core's apply warning for a replaced file (kind `foreign-file`). */
export function isForeignWarning(w: Warning): boolean {
  return w.kind === 'foreign-file';
}

/** Tool name out of the core's warning text ("replacing a global.ini from <tool> (kept ..."). */
export function toolFromWarning(w: Warning): string | null {
  const m = /from (.+?) \(kept/.exec(w.detail);
  const t = m?.[1]?.trim() ?? null;
  return t && t !== 'another tool' ? t : null;
}

/** After writing: what the Apply result says instead of the core's terse warning. */
export function foreignAfter(f: ForeignChannel): { title: string; body: string } {
  const t = toolName(f.tool);
  const its = f.tool ? `${t}'s` : "the other tool's";
  return {
    title: `Replaced ${its} file on ${f.channel}`,
    body: `${its.charAt(0).toUpperCase() + its.slice(1)} global.ini was backed up (kept for good) and replaced with StarOrg Codex's text. Turn off ${its} auto-apply for ${f.channel}, or the two will overwrite each other at every launch.`,
  };
}
