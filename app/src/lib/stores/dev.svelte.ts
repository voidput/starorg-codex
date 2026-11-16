/**
 * Developer mode state shared by the Dev screens and the command palette (addendum 7.8): the
 * explorer worker's status, the visited-records history, the "Show IDs" switch, and hand-offs from Ctrl+K ("Find
 * record…", "Open record by GUID") and from the dataset viewer ("Open in Explorer").
 */
import { devApi, type DevStatus } from '@/lib/contract/dev';
import { toContractError, type ContractError } from '@/lib/contract/client';
import type { ChannelRef } from '@/lib/contract/types';
import { History } from '@/lib/devnav';
import { loadShowIds, saveShowIds } from '@/lib/readable';

const storage = (): Storage | null => {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage;
  } catch {
    return null;
  }
};

class DevState {
  status = $state<DevStatus | null>(null);
  statusFor = $state('');
  opening = $state(false);
  error = $state<ContractError | null>(null);
  /** A search the explorer should run and focus (palette "Find record…"). */
  pendingSearch = $state<string | null>(null);
  /** A record the explorer should open (palette GUID, dataset link). */
  pendingOpen = $state<string | null>(null);
  /** A dataset row Datasets should show (Diff "Open in Datasets", a linked row). */
  pendingRow = $state<{ source: string; dataset: string; table: string; row: number } | null>(null);
  /** A record Diff should show (Datasets "Show changes in Diff", a changed linked row). */
  pendingDiff = $state<{ dataset: string; table: string; id: string } | null>(null);
  /** Bumped on every hand-off so the explorer reacts even to the same text again. */
  seq = $state(0);
  history = new History();
  /** Bumped when the history changes (the class itself is not reactive). */
  histRev = $state(0);
  /** "Show IDs" (7.14): raw ids instead of readable names, on all three screens; per viewer. */
  showIds = $state(loadShowIds(storage()));

  setShowIds(on: boolean): void {
    this.showIds = on;
    saveShowIds(storage(), on);
  }

  findRecord(q: string, go: () => void): void {
    this.pendingSearch = q;
    this.seq++;
    go();
  }

  openRecord(what: string, go: () => void): void {
    this.pendingOpen = what;
    this.seq++;
    go();
  }

  openRow(at: { source: string; dataset: string; table: string; row: number }, go: () => void): void {
    this.pendingRow = at;
    this.seq++;
    go();
  }

  openDiff(at: { dataset: string; table: string; id: string }, go: () => void): void {
    this.pendingDiff = at;
    this.seq++;
    go();
  }

  /** Status of the worker for this channel; starts it when `open` (with progress events). */
  async ensure(target: ChannelRef, open: boolean): Promise<DevStatus | null> {
    const key = `${target.install}/${target.channel}`;
    try {
      let s = await devApi.status(target);
      if (open && s.state !== 'ready') {
        this.opening = true;
        s = await devApi.open(target);
      }
      this.status = s;
      this.statusFor = key;
      this.error = null;
      return s;
    } catch (e) {
      this.error = toContractError(e);
      try {
        this.status = await devApi.status(target);
        this.statusFor = key;
      } catch {
        /* keep the last status */
      }
      return null;
    } finally {
      this.opening = false;
    }
  }

  async close(target: ChannelRef): Promise<void> {
    try {
      this.status = await devApi.close(target);
    } catch (e) {
      this.error = toContractError(e);
    }
  }
}

export const dev = new DevState();
