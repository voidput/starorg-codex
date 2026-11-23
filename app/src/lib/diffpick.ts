/**
 * Diff's first A and B (addendum 7.6), as a pure function. A reads as the older side, B as
 * the newer: when the target channel has an older build kept in the dataset cache, A is the
 * newest of those and B the channel's current build ("what changed since the build I read
 * before"); otherwise the first two sources as listed (this build, then the next).
 */
import type { DevSource } from './contract/dev';

export interface DiffPair {
  a: string | null;
  b: string | null;
}

export function defaultPair(sources: DevSource[] | null | undefined, channel: string): DiffPair {
  const ready = Array.isArray(sources) ? sources.filter((s) => s.state === 'ready') : [];
  const current = ready.find((s) => s.kind === 'build' && s.id === `build:${channel}`);
  // Listed newest first within a channel, right after its build.
  const previous = ready.find((s) => s.kind === 'cached' && s.id.startsWith(`cached:${channel}:`));
  if (current && previous) return { a: previous.id, b: current.id };
  const a = ready[0]?.id ?? null;
  return { a, b: ready.find((s) => s.id !== a)?.id ?? null };
}
