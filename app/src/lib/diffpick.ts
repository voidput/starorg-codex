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
  // Listed newest first within a channel, right after its build. Only those sharing a dataset
  // with the current build (Diff compares just those; none yet right after a game update,
  // before WHERE TO GET reads the new build). The same game version read again (after an
  // app update) differs in extraction only, so another version comes first.
  const files = new Set((current?.datasets ?? []).map((d) => d.file));
  const cached = ready.filter((s) => s.kind === 'cached' && s.id.startsWith(`cached:${channel}:`) && s.datasets.some((d) => files.has(d.file)));
  const previous = cached.find((s) => !current?.build || s.build !== current.build) ?? cached[0];
  if (current && previous) return { a: previous.id, b: current.id };
  const first = ready[0];
  const a = first?.id ?? null;
  // B: the next one, passing over a cached build that has nothing to compare with A.
  const has = new Set((first?.datasets ?? []).map((d) => d.file));
  const comparable = (s: DevSource) => s.kind !== 'cached' || s.datasets.some((d) => has.has(d.file));
  const b = ready.find((s) => s.id !== a && comparable(s)) ?? ready.find((s) => s.id !== a);
  return { a, b: b?.id ?? null };
}

/**
 * The pair to show once the sources are listed: `cur` (what was picked before, or the last
 * comparison's) as far as its ids are still listed and ready (an older cached build may have
 * been pruned since), the rest filled in as {@link defaultPair} would.
 */
export function keepPair(sources: DevSource[] | null | undefined, channel: string, cur: DiffPair): DiffPair {
  const ready = Array.isArray(sources) ? sources.filter((s) => s.state === 'ready') : [];
  const listed = (id: string | null) => (id !== null && ready.some((s) => s.id === id) ? id : null);
  let a = listed(cur.a);
  let b = listed(cur.b);
  if (a === null) {
    const first = defaultPair(ready, channel);
    a = first.a === b ? (ready.find((s) => s.id !== b)?.id ?? null) : first.a;
    b ??= first.b === a ? null : first.b;
  }
  b ??= ready.find((s) => s.id !== a)?.id ?? null;
  return { a, b };
}
