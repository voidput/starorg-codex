/**
 * Loading state for a backend call that can be slow (the first read of Data.p4k) or fail, and
 * that is re-run when its inputs change. Only the latest run may set the state: an older,
 * slower answer that arrives after a newer one is dropped (a wizard step re-asks when a choice
 * changes). Plain TS, so the paths are unit-tested without Svelte (tests/loadstate.test.ts).
 */
export type Load<T, E> =
  | { state: 'idle' }
  | { state: 'loading'; since: number }
  | { state: 'ok'; value: T }
  | { state: 'error'; error: E };

export interface Latest<T, E> {
  /** Start a run; resolves when it settles (or was overtaken by a newer run). */
  run(f: () => Promise<T>): Promise<void>;
  /** Drop whatever is running (component gone). */
  stop(): void;
}

export function latest<T, E>(set: (l: Load<T, E>) => void, toError: (e: unknown) => E, now: () => number = Date.now): Latest<T, E> {
  let seq = 0;
  return {
    async run(f) {
      const me = ++seq;
      set({ state: 'loading', since: now() });
      try {
        const value = await f();
        if (me === seq) set({ state: 'ok', value });
      } catch (e) {
        if (me === seq) set({ state: 'error', error: toError(e) });
      }
    },
    stop() {
      seq++;
    },
  };
}

/**
 * A stale-answer guard for click-driven loads that set several fields (not one `Load`): each
 * `next()` starts a run and returns its check, true only while no newer run started and
 * `drop()` was not called. Click A, then B: A's slower answer finds its check false and is
 * dropped instead of overwriting B.
 */
export interface Guard {
  next(): () => boolean;
  drop(): void;
}

export function guard(): Guard {
  let seq = 0;
  return {
    next() {
      const me = ++seq;
      return () => me === seq;
    },
    drop() {
      seq++;
    },
  };
}

/** Seconds since `since`, for "Reading game data… 12 s". */
export function elapsed(since: number, now: number): number {
  return Math.max(0, Math.floor((now - since) / 1000));
}

/** After this long the loading panel explains why a first read can take minutes. */
export const SLOW_AFTER_S = 15;

/** "12 s", "1 min 05 s". */
export function fmtElapsed(s: number): string {
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, '0')} s`;
}

/**
 * The in-game preview before setup: the real backend answers an empty list while it has not read
 * the channel's game data (an install that is not saved yet, or no cache for this build). That is
 * not an error and not "nothing to show", so the step says why and offers to read it now.
 */
export type PreviewView = 'loading' | 'error' | 'nodata' | 'items';
export function previewView<T>(l: Load<T[], unknown>): PreviewView {
  if (l.state === 'error') return 'error';
  if (l.state === 'ok') return l.value.length ? 'items' : 'nodata';
  return 'loading';
}

/**
 * Run async jobs one at a time, in call order (the mock backend's model of the real backend's
 * game-data read lock: a second read waits for the first instead of failing or skipping).
 */
export function serial(): <X>(f: () => Promise<X>) => Promise<X> {
  let tail: Promise<void> = Promise.resolve();
  return async <X>(f: () => Promise<X>): Promise<X> => {
    const prev = tail;
    let release!: () => void;
    tail = new Promise<void>((r) => (release = r));
    await prev;
    try {
      return await f();
    } finally {
      release();
    }
  };
}
