/**
 * One colour per game channel (addendum §7.19), shared by the channel picker, the Home cards and
 * the Blueprints screen, so LIVE / PTU / HOTFIX look the same everywhere. Colour is never the only
 * signal: every badge also carries the channel's name (spec 9.6). Hues avoid the status tones'
 * meaning by always sitting next to text; each has a dark- and a light-theme value with at least
 * 4.5:1 contrast on the app's card background.
 */
export interface ChannelColor {
  dark: string;
  light: string;
}

const KNOWN: Record<string, ChannelColor> = {
  LIVE: { dark: '#2dd4bf', light: '#0e6b64' },
  PTU: { dark: '#fb923c', light: '#b4410b' },
  EPTU: { dark: '#facc15', light: '#765600' },
  HOTFIX: { dark: '#f472b6', light: '#be185d' },
  'TECH-PREVIEW': { dark: '#a3e635', light: '#3f6a0a' },
};

/** For channels without a fixed colour, picked by a stable hash of the name. */
const SPARE: ChannelColor[] = [
  { dark: '#60a5fa', light: '#1d4ed8' },
  { dark: '#c4b5fd', light: '#6d28d9' },
  { dark: '#fda4af', light: '#be123c' },
  { dark: '#86efac', light: '#166534' },
];

export function channelColor(channel: string): ChannelColor {
  const k = channel.trim().toUpperCase();
  const known = KNOWN[k];
  if (known) return known;
  let h = 0;
  for (let i = 0; i < k.length; i++) h = (h * 31 + k.charCodeAt(i)) >>> 0;
  return SPARE[h % SPARE.length]!;
}

/** CSS custom properties for an element using the `chan` classes (see ChannelBadge). */
export function channelStyle(channel: string): string {
  const c = channelColor(channel);
  return `--chan-d: ${c.dark}; --chan-l: ${c.light};`;
}
