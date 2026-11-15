/**
 * Setup wizard state. Steps derive from the draft (spec 9.4: "steps derive from state, so an
 * interrupted setup resumes"); the draft is kept in localStorage per viewer until setup succeeds.
 */
import { ACK_LABEL, ackKey, foreignOf, type ForeignChannel } from '../../lib/foreign.ts';
import type { ChannelSummary, DataSource, InstallSummary, Preset, Settings, SetupRequest, Theme } from '@/lib/contract/types';

export type StepId = 'install' | 'channels' | 'source' | 'enhancements' | 'prefs' | 'review' | 'apply';

export const STEPS: { id: StepId; title: string; short: string }[] = [
  { id: 'install', title: 'Find your game', short: 'Game' },
  { id: 'channels', title: 'Choose channels', short: 'Channels' },
  { id: 'source', title: 'Game data source', short: 'Data' },
  { id: 'enhancements', title: 'Pick enhancements', short: 'Enhancements' },
  { id: 'prefs', title: 'Preferences', short: 'Preferences' },
  { id: 'review', title: 'Review', short: 'Review' },
  { id: 'apply', title: 'Apply', short: 'Apply' },
];

export interface ChannelChoice {
  consent: boolean;
  /** For a non-English channel: null = not answered yet (the wizard asks, never defaults). */
  switch_language: boolean | null;
}

export interface Draft {
  install: string | null;
  channels: Record<string, ChannelChoice>;
  data_source: DataSource;
  preset: Preset;
  fix_known_bugs: boolean;
  /** App settings asked in setup; null until seeded from the saved Settings (or the defaults). */
  prefs: Prefs | null;
  /** Furthest step reached, for the rail and resume. */
  reached: number;
  /** `ackKey` of the other tools' files the user agreed to back up and replace (Review). */
  replace_ack: string | null;
}

const KEY = 'starorg:setup-draft';

export function emptyDraft(): Draft {
  return { install: null, channels: {}, data_source: 'local', preset: 'recommended', fix_known_bugs: true, prefs: null, reached: 0, replace_ack: null };
}

export function loadDraft(): Draft | null {
  try {
    const s = localStorage.getItem(KEY);
    return s ? { ...emptyDraft(), ...(JSON.parse(s) as Partial<Draft>) } : null;
  } catch {
    return null;
  }
}

export function saveDraft(d: Draft): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(d));
  } catch {
    /* fine */
  }
}

export function clearDraft(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* fine */
  }
}

/** Default choice for a channel the first time it is shown: English channels with a build are ticked. */
export function defaultChoice(c: ChannelSummary): ChannelChoice {
  return { consent: c.language !== 'other' && !c.build_error, switch_language: c.language === 'other' ? null : false };
}

export function ensureChoices(d: Draft, install: InstallSummary | null): void {
  if (!install) return;
  for (const c of install.channels) d.channels[c.channel] ??= defaultChoice(c);
  for (const k of Object.keys(d.channels)) if (!install.channels.some((c) => c.channel === k)) delete d.channels[k];
}

/** Why the user cannot leave a step yet; null = ok. */
export function blocker(step: StepId, d: Draft, install: InstallSummary | null): string | null {
  switch (step) {
    case 'install':
      return install ? null : 'Choose an install, or browse to the folder that contains LIVE.';
    case 'channels': {
      if (!install) return 'Choose an install first.';
      const picked = install.channels.filter((c) => d.channels[c.channel]?.consent);
      if (!picked.length) return 'Choose at least one channel.';
      const open = picked.find((c) => c.language === 'other' && d.channels[c.channel]?.switch_language !== true);
      return open ? `Answer the language question for ${open.channel}.` : null;
    }
    case 'review': {
      if (!install) return 'Choose an install first.';
      const fs = foreignPicked(d, install);
      return fs.length && d.replace_ack !== ackKey(fs) ? `Tick "${ACK_LABEL}" to let setup replace ${fs.map((f) => `${f.channel}'s`).join(' and ')} file.` : null;
    }
    default:
      return null;
  }
}

/** Ticked channels that hold another tool's file: setup backs it up and replaces it. */
export function foreignPicked(d: Draft, install: InstallSummary): ForeignChannel[] {
  return install.channels.filter((c) => d.channels[c.channel]?.consent).flatMap((c) => foreignOf(c) ?? []);
}

export function request(d: Draft, install: InstallSummary): SetupRequest {
  return {
    install: install.id,
    channels: install.channels.map((c) => ({
      channel: c.channel,
      consent: !!d.channels[c.channel]?.consent,
      switch_language: d.channels[c.channel]?.switch_language === true,
    })),
    data_source: d.data_source,
    preset: d.preset,
    fix_known_bugs: d.fix_known_bugs,
  };
}

/**
 * The basic app settings the Preferences step asks (all existing Settings fields, saved with
 * save_settings on Apply). Network switches default off (docs/privacy.md).
 */
export interface Prefs {
  /** settings.network.check_updates. Not asked in the Microsoft Store build. */
  check_updates: boolean;
  theme: Theme;
  reduce_effects: boolean;
  /** settings.network.starorg: offer the starorg.tools sign-in. */
  starorg: boolean;
}

/** Defaults when no saved Settings could be read: as Rust's (theme.ts `defaultAppearance`). */
export function defaultPrefs(linux: boolean): Prefs {
  return { check_updates: false, theme: 'dark', reduce_effects: linux, starorg: false };
}

/** The current values, so running setup again shows (and keeps) what is saved. */
export function prefsFrom(s: Settings): Prefs {
  return { check_updates: s.network.check_updates, theme: s.appearance.theme, reduce_effects: s.appearance.reduce_effects, starorg: s.network.starorg };
}

/** Settings with the step's choices; everything else as saved. The Store build keeps its update switch. */
export function withPrefs(s: Settings, p: Prefs, store: boolean): Settings {
  return {
    ...s,
    appearance: { ...s.appearance, theme: p.theme, reduce_effects: p.reduce_effects },
    network: { ...s.network, check_updates: store ? s.network.check_updates : p.check_updates, starorg: p.starorg },
  };
}

const THEME_NAME: Record<Theme, string> = { system: 'Same as the system', dark: 'Dark', light: 'Light' };

/** Review rows for the Preferences step. */
export function prefsSummary(p: Prefs, store: boolean): { label: string; value: string }[] {
  const rows: { label: string; value: string }[] = [];
  if (!store) {
    rows.push({
      label: 'Updates',
      value: p.check_updates ? 'Check automatically (api.github.com, at most once a day)' : 'Do not check (no request)',
    });
  }
  rows.push({ label: 'Appearance', value: `${THEME_NAME[p.theme]}${p.reduce_effects ? ', reduced effects' : ''}` });
  rows.push({ label: 'StarOrg Tools', value: p.starorg ? 'Sign-in offered (nothing sent until you sign in)' : 'Off' });
  return rows;
}
