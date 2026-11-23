/**
 * Explorer and Files read the whole Data.p4k into a worker process (0.5-1 GB of memory while
 * open). Opening the screen only asks for the worker's status; the worker starts on a click
 * ("Open game data"), on a hand-off from another screen (Ctrl+K "Find record…", "Open owning
 * record"), or by itself when the viewer already opened it for this channel this session.
 * Plain TS, unit-tested in tests/lazyopen.test.ts.
 */
import type { WorkerState } from '@/lib/contract/dev';

/** `ready`: use it now; `open`: start it (or wait for its start); `ask`: offer the button. */
export type OpenPlan = 'ready' | 'open' | 'ask';

export function openPlan(state: WorkerState | null | undefined, handoff: boolean, openedBefore: boolean): OpenPlan {
  if (state === 'ready') return 'ready';
  // Another screen is starting it: waiting costs nothing extra.
  if (state === 'starting') return 'open';
  if (state === 'failed') return 'ask';
  return handoff || openedBefore ? 'open' : 'ask';
}

/** What the open costs, for the button's hint. */
export const OPEN_COST = 'read only; about 0.5-1 GB of memory while open';

/** The workers the viewer opened this session, per screen and channel (`explorer:<install>/<channel>`). */
export class Opened {
  #keys = new Set<string>();
  has(kind: string, channel: string): boolean {
    return this.#keys.has(`${kind}:${channel}`);
  }
  add(kind: string, channel: string): void {
    if (channel) this.#keys.add(`${kind}:${channel}`);
  }
  /** Closed by hand: the next visit asks again. */
  forget(kind: string, channel: string): void {
    this.#keys.delete(`${kind}:${channel}`);
  }
}
