/**
 * Patches list in two steps: `list_patches` answers at once (an enabled patch reads `checking`
 * until the channel's table is built), `patch_states` brings the states. Plain TS, unit-tested
 * in tests/patchstates.test.ts.
 */
import type { PatchView } from './contract/types';

/** Whether a second call is needed for the states. */
export function needsStates(rows: readonly PatchView[]): boolean {
  return rows.some((p) => p.state === 'checking');
}

/**
 * Fill the `checking` rows from `states`. Rows the user changed meanwhile (a toggle answered
 * with its own state, a new or deleted patch) keep what they have: only a row still `checking`
 * with the same switch position takes the late state.
 */
export function fillStates(rows: readonly PatchView[], states: readonly PatchView[]): PatchView[] {
  const by = new Map(states.map((s) => [s.id, s]));
  return rows.map((p) => {
    const s = by.get(p.id);
    return p.state === 'checking' && s && s.enabled === p.enabled ? { ...p, state: s.state } : p;
  });
}
