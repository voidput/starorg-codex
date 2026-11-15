/**
 * Setup wizard state. Steps derive from the draft (spec 9.4: "steps derive from state, so an
 * interrupted setup resumes"); the draft is kept in localStorage per viewer until setup succeeds.
 */
import type { ChannelSummary, DataSource, InstallSummary, Preset, SetupRequest } from '@/lib/contract/types';

export type StepId = 'install' | 'channels' | 'source' | 'enhancements' | 'review' | 'apply';

export const STEPS: { id: StepId; title: string; short: string }[] = [
  { id: 'install', title: 'Find your game', short: 'Game' },
  { id: 'channels', title: 'Choose channels', short: 'Channels' },
  { id: 'source', title: 'Game data source', short: 'Data' },
  { id: 'enhancements', title: 'Pick enhancements', short: 'Enhancements' },
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
  /** Furthest step reached, for the rail and resume. */
  reached: number;
}

const KEY = 'starorg:setup-draft';

export function emptyDraft(): Draft {
  return { install: null, channels: {}, data_source: 'local', preset: 'recommended', fix_known_bugs: true, reached: 0 };
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
    default:
      return null;
  }
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
