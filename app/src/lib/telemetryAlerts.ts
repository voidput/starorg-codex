/**
 * Live telemetry alerts (experimental) in the webview: pure helpers, no DOM, so node tests load
 * them. The backend evaluates the rules (starorg_telemetry::alerts) and sends what fired with
 * the frame; it already strips the tone and the speech text unless the user chose sound (and
 * speech). Here: the audio queue (one alert at a time, priority, critical first), the tone
 * patterns, the recent list and how a rule's threshold reads.
 */
import type { StatusTone } from './format';
import type { AlertSeverity, TelemetryAlert, TelemetryAlertRule, TelemetryAlertSettings, TelemetryRuleOverride } from './contract/telemetry';

const RANK: Record<AlertSeverity, number> = { info: 0, warn: 1, critical: 2 };

/** Rough playback length of an alert (tone plus speech), ms. */
export function estDuration(a: TelemetryAlert): number {
  return 600 + 75 * (a.speech?.length ?? 0);
}

export interface Queued {
  alert: TelemetryAlert;
  /** When it was queued (ms, the queue's clock). */
  t: number;
}

/**
 * sc-telemetry's AudioScheduler: one alert plays at a time and starts at least `gapMs` after the
 * last. Highest severity first, then oldest. A critical alert cuts off a lower one that is still
 * playing and skips the gap unless the last one was critical too. At most `max` wait; anything
 * older than `ttlMs` is dropped, and a newer alert of the same rule replaces a waiting one.
 */
export class AlertAudioQueue {
  private queue: Queued[] = [];
  private lastStart = -Infinity;
  private playing: { severity: AlertSeverity; until: number } | null = null;

  readonly gapMs: number;
  readonly max: number;
  readonly ttlMs: number;

  constructor(gapMs = 2000, max = 6, ttlMs = 10_000) {
    this.gapMs = gapMs;
    this.max = max;
    this.ttlMs = ttlMs;
  }

  get size(): number {
    return this.queue.length;
  }

  submit(alert: TelemetryAlert, now: number): void {
    if (!alert.sound && !alert.speech) return;
    this.queue = this.queue.filter((q) => q.alert.rule !== alert.rule);
    this.queue.push({ alert, t: now });
    while (this.queue.length > this.max) {
      // Drop the least important, oldest first.
      let low = 0;
      for (let i = 1; i < this.queue.length; i++) {
        const a = this.queue[i]!;
        const b = this.queue[low]!;
        if (RANK[a.alert.severity] < RANK[b.alert.severity] || (RANK[a.alert.severity] === RANK[b.alert.severity] && a.t < b.t)) low = i;
      }
      this.queue.splice(low, 1);
    }
  }

  clear(): void {
    this.queue = [];
  }

  /** The alert to start now and whether it cuts off the one playing, or null. */
  next(now: number): { alert: TelemetryAlert; preempt: boolean } | null {
    this.queue = this.queue.filter((q) => now - q.t <= this.ttlMs || q.alert.test);
    if (this.queue.length === 0) return null;
    let top = this.queue[0]!;
    for (const q of this.queue) {
      if (RANK[q.alert.severity] > RANK[top.alert.severity] || (RANK[q.alert.severity] === RANK[top.alert.severity] && q.t < top.t)) top = q;
    }
    const sev = RANK[top.alert.severity];
    const busy = this.playing && now < this.playing.until ? RANK[this.playing.severity] : null;
    const lastSev = this.playing ? RANK[this.playing.severity] : -1;
    let preempt = false;
    if (busy !== null) {
      if (sev <= busy || sev < RANK.critical) return null;
      preempt = true;
    } else if (now - this.lastStart < this.gapMs && !(sev === RANK.critical && lastSev < sev)) return null;
    this.queue = this.queue.filter((q) => q !== top);
    this.lastStart = now;
    this.playing = { severity: top.alert.severity, until: now + estDuration(top.alert) };
    return { alert: top.alert, preempt };
  }

  /** When `next` may have something again (ms), or null when nothing waits. */
  wakeAt(now: number): number | null {
    if (this.queue.length === 0) return null;
    return Math.max(now + 50, this.lastStart + this.gapMs, this.playing?.until ?? 0);
  }
}

/** A short tone per sound id, made in the app (WebAudio): no audio file, nothing from the game. */
export interface TonePattern {
  freq: number;
  beeps: number;
  beepMs: number;
  gapMs: number;
  wave: 'sine' | 'square' | 'triangle';
}

export const TONES: Record<string, TonePattern> = {
  info: { freq: 880, beeps: 1, beepMs: 140, gapMs: 0, wave: 'sine' },
  warn: { freq: 660, beeps: 2, beepMs: 130, gapMs: 90, wave: 'triangle' },
  critical: { freq: 990, beeps: 3, beepMs: 110, gapMs: 60, wave: 'square' },
};

export function tonePattern(sound: string | null, severity: AlertSeverity): TonePattern | null {
  if (!sound) return null;
  return TONES[sound] ?? TONES[severity === 'critical' ? 'critical' : severity === 'warn' ? 'warn' : 'info']!;
}

export function severityTone(s: AlertSeverity): StatusTone {
  return s === 'critical' ? 'destructive' : s === 'warn' ? 'warning' : 'info';
}

export function severityLabel(s: AlertSeverity): string {
  return s === 'critical' ? 'Critical' : s === 'warn' ? 'Warning' : 'Info';
}

/** New alerts in front, the list cut at `max`. */
export function mergeRecent(list: readonly TelemetryAlert[], fired: readonly TelemetryAlert[], max: number): TelemetryAlert[] {
  if (fired.length === 0) return list as TelemetryAlert[];
  return [...[...fired].reverse(), ...list].slice(0, max);
}

/** A threshold as the user reads and types it: a fraction as a percentage. */
export function toShown(rule: Pick<TelemetryAlertRule, 'unit'>, v: number | null): number | null {
  if (v === null) return null;
  return rule.unit === 'fraction' ? Math.round(v * 1000) / 10 : v;
}

export function fromShown(rule: Pick<TelemetryAlertRule, 'unit'>, v: number): number {
  return rule.unit === 'fraction' ? Math.round(v * 10) / 1000 : v;
}

export function unitSuffix(rule: Pick<TelemetryAlertRule, 'unit'>): string {
  return rule.unit === 'fraction' ? '%' : '';
}

/** "≤ 25 % (again above 35 %)" for a threshold rule, else null. */
export function thresholdText(rule: TelemetryAlertRule): string | null {
  if (rule.threshold === null || !rule.op) return null;
  const op = { '<': '<', '<=': '≤', '>': '>', '>=': '≥', '==': '=', '!=': '≠' }[rule.op] ?? rule.op;
  const u = unitSuffix(rule);
  const s = `${op} ${toShown(rule, rule.threshold)}${u}`;
  if (rule.rearm === null) return s;
  const below = rule.op === '<' || rule.op === '<=';
  return `${s} (arms again ${below ? 'above' : 'below'} ${toShown(rule, rule.rearm)}${u})`;
}

export function isChanged(rule: TelemetryAlertRule): boolean {
  return rule.enabled !== rule.default_enabled || rule.cooldown_s !== rule.default_cooldown_s || rule.threshold !== rule.default_threshold || rule.rearm !== rule.default_rearm;
}

const EMPTY: TelemetryRuleOverride = { enabled: null, cooldown_s: null, value: null, rearm: null };

/** The settings with one rule's override patched (a field back at its default drops out). */
export function withOverride(s: TelemetryAlertSettings, rule: TelemetryAlertRule, patch: Partial<TelemetryRuleOverride> | null): TelemetryAlertSettings {
  const overrides = { ...s.overrides };
  if (patch === null) delete overrides[rule.id];
  else {
    const o: TelemetryRuleOverride = { ...EMPTY, ...overrides[rule.id], ...patch };
    if (o.enabled === rule.default_enabled) o.enabled = null;
    if (o.cooldown_s === rule.default_cooldown_s) o.cooldown_s = null;
    if (o.value === rule.default_threshold && (o.rearm === null || o.rearm === rule.default_rearm)) {
      o.value = null;
      o.rearm = null;
    }
    if (o.enabled === null && o.cooldown_s === null && o.value === null && o.rearm === null) delete overrides[rule.id];
    else overrides[rule.id] = o;
  }
  return { ...s, overrides };
}

/** "14:03:07" in the user's locale. */
export function clockText(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}
