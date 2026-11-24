/**
 * Live telemetry alerts (experimental): the Notifications choice, the rules and the recent
 * alerts, shared by Settings > Experimental and the Live telemetry screen's Alerts panel.
 *
 * The backend evaluates the rules only while a stream runs and Notifications is not Off, and
 * sends what fired with the frame (the telemetry store hands it to `take`). Alerts are rare, so
 * the recent list is a raw state replaced only when one fires: frames never re-render it. Each
 * alert is a toast (one per rule: a newer one replaces it) and, with "In app + sound", a tone
 * and optional speech through one queue (telemetryAlerts.ts), so two never talk at once.
 */
import { toContractError } from '@/lib/contract/client';
import { telemetryApi, type TelemetryAlert, type TelemetryAlerts, type TelemetryAlertSettings } from '@/lib/contract/telemetry';
import { AlertAudioQueue, mergeRecent, tonePattern } from '@/lib/telemetryAlerts';
import { playTone, speak, stopAll, unlock } from '@/lib/alertSound';
import { dismiss, toast } from './toast.svelte';

const TOAST_MS = { info: 5000, warn: 8000, critical: 12_000 } as const;

class TelemetryAlertsState {
  view = $state.raw<TelemetryAlerts | null>(null);
  /** Newest first. */
  recent = $state.raw<TelemetryAlert[]>([]);
  busy = $state(false);

  settings = $derived(this.view?.settings ?? null);

  private queue = new AlertAudioQueue();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private toastOf = new Map<string, number>();

  private takeView(v: TelemetryAlerts): TelemetryAlerts {
    this.view = v;
    this.recent = v.recent;
    return v;
  }

  async load(): Promise<TelemetryAlerts | null> {
    try {
      return this.takeView(await telemetryApi.alerts());
    } catch (e) {
      toast('error', 'Could not read the alert settings', toContractError(e).message);
      return null;
    }
  }

  /** Save the choice and every override (throws, so the caller can show the reason inline). */
  async save(next: TelemetryAlertSettings): Promise<TelemetryAlerts> {
    this.busy = true;
    // The click that picks sound is the gesture that lets the webview play it.
    if (next.notifications === 'in_app_sound') unlock();
    try {
      const v = this.takeView(await telemetryApi.configureAlerts(next));
      if (v.settings.notifications !== 'in_app_sound') {
        this.queue.clear();
        stopAll();
      }
      return v;
    } catch (e) {
      void this.load();
      throw e;
    } finally {
      this.busy = false;
    }
  }

  /** A test alert of one rule: toast, tone and speech as a real one, ignoring the cooldown. */
  async test(rule: string): Promise<void> {
    unlock();
    try {
      this.deliver([await telemetryApi.testAlert({ rule })]);
    } catch (e) {
      toast('error', 'No test alert', toContractError(e).message);
    }
  }

  /** Alerts that came with a frame. */
  take(fired: readonly TelemetryAlert[]): void {
    if (fired.length === 0) return;
    this.recent = mergeRecent(this.recent, fired, this.view?.recent_max ?? 50);
    this.deliver(fired);
  }

  private deliver(list: readonly TelemetryAlert[]): void {
    // The backend sends alerts only while notifications are on; a choice just switched off wins.
    if (this.view && this.view.settings.notifications === 'off') return;
    for (const a of list) {
      const old = this.toastOf.get(a.rule);
      if (old !== undefined) dismiss(old);
      this.toastOf.set(a.rule, toast(a.severity === 'info' ? 'info' : 'warning', a.text, a.severity === 'critical' ? 'Critical alert' : null, undefined, TOAST_MS[a.severity]));
      this.queue.submit(a, Date.now());
    }
    this.pump();
  }

  private pump(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    const now = Date.now();
    const n = this.queue.next(now);
    if (n) {
      if (n.preempt) stopAll();
      const p = tonePattern(n.alert.sound, n.alert.severity);
      const ms = p ? playTone(p) : 0;
      if (n.alert.speech) speak(n.alert.speech, ms + 80);
    }
    const at = this.queue.wakeAt(Date.now());
    if (at !== null) this.timer = setTimeout(() => this.pump(), Math.max(50, at - Date.now()));
  }
}

export const telemetryAlerts = new TelemetryAlertsState();
