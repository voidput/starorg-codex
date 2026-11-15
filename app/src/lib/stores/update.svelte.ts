/**
 * Update check state (docs/updates.md): the start-up check, "Check now", skip, the banner.
 * Every request decision (switch, 24 h throttle) is made in Rust; this only asks and shows.
 */
import { api, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { toast } from './toast.svelte';

class UpdateState {
  status = $state<T.UpdateStatus | null>(null);
  checking = $state(false);
  installing = $state(false);
  /** Banner closed with X: hidden for this session only (Skip is the lasting choice). */
  dismissed = $state<string | null>(null);

  /** On start: the saved result, and the start-up check when the switch is on. Quiet on errors. */
  async init(): Promise<void> {
    try {
      this.status = await api.checkForUpdates(false);
    } catch {
      try {
        this.status = await api.updateStatus();
      } catch {
        /* the rest of the app works without it */
      }
    }
  }

  async refresh(): Promise<void> {
    try {
      this.status = await api.updateStatus();
    } catch (e) {
      toast('error', 'Could not read the update status', toContractError(e).message);
    }
  }

  /** After the switch is turned on: the first check runs now (it is due), not on next start. */
  async enabled(): Promise<void> {
    await this.check(false);
  }

  async check(force = true): Promise<void> {
    this.checking = true;
    try {
      this.status = await api.checkForUpdates(force);
      this.dismissed = null;
    } catch (e) {
      toast('error', 'Could not check for updates', toContractError(e).message);
    } finally {
      this.checking = false;
    }
  }

  async setPrefs(p: T.UpdatePrefs): Promise<boolean> {
    try {
      this.status = await api.setUpdatePrefs(p);
      return true;
    } catch (e) {
      toast('error', 'Not saved', toContractError(e).message);
      return false;
    }
  }

  async skip(version: string): Promise<void> {
    try {
      this.status = await api.skipUpdate(version);
      if (version) toast('info', `Skipping ${version}`, 'A newer version will show again. Settings -> Network can undo this.');
    } catch (e) {
      toast('error', 'Could not skip this version', toContractError(e).message);
    }
  }

  async openPage(): Promise<void> {
    try {
      await api.openReleasePage();
    } catch (e) {
      toast('error', 'Could not open the release page', toContractError(e).message);
    }
  }

  /** Downloads, verifies the signature, installs and restarts (AppImage / Windows installer). */
  async install(): Promise<void> {
    this.installing = true;
    try {
      this.status = await api.installUpdate();
    } catch (e) {
      toast('error', 'Update not installed', toContractError(e).message);
    } finally {
      this.installing = false;
    }
  }
}

export const updates = new UpdateState();
