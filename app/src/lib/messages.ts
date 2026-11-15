/**
 * Message catalogue (spec 9.6): words the player reads for values that come from the core.
 * Components lead with these and keep the technical value (rule slug, reason id, log prefix)
 * in a mono detail line. Imports types only, so `node --test` can load it without a bundler.
 */
import type { OverrideRule } from './contract/types';

/** M1 `validate::Rule` names in plain words. */
export const RULE_TEXT: Record<OverrideRule, { check: string; label: string; message: string }> = {
  'control-character': { check: 'One line', label: 'Line break or control character', message: 'Write a line break as the two characters \\n; other control characters are not allowed.' },
  'added-em-unbalanced': { check: 'Added tags closed', label: 'Unclosed highlight tag', message: 'Every <EMn> tag you add needs its closing </EMn>.' },
  'em-signature': { check: 'Original tags kept', label: 'Highlight tags changed', message: "Keep the original's <EMn> tags; only the text between them can change." },
  'variable-tokens': { check: 'Game variables kept', label: 'Game variable missing', message: 'Keep every ~variable(...) token of the original; the game fills them in.' },
  'format-specifiers': { check: 'Placeholders kept', label: 'Placeholder missing', message: "Keep the original's %s and %d placeholders, in the same order." },
  'format-key': { check: 'Percent in format text', label: 'Plain % in a format text', message: 'This key is a format string; write a plain % as %%.' },
  'percent-rule': { check: 'Percent signs', label: 'Lone percent sign', message: 'A % must be followed by the end of the text, \\n, or two spaces.' },
  'too-long': { check: 'Length', label: 'Too long', message: 'The text is longer than the game shows.' },
};

export function ruleText(rule: string | null | undefined): { check: string; label: string; message: string } {
  return (rule && RULE_TEXT[rule as OverrideRule]) || { check: 'Game text rule', label: 'Breaks a game text rule', message: 'The game could not show this text safely.' };
}

/** Stale reasons (core `apply::stale_reasons`) in plain words. An app update alone is no
 *  reason; `enhancer update` means the new version writes different text. */
const STALE: Record<string, string> = {
  build: 'game update',
  'game data': 'game update',
  profile: 'your settings',
  overrides: 'your edits',
  'enhancer update': 'improved enhancements',
  patches: 'patches',
  'ship names': 'ship names',
  'owned blueprints': 'owned blueprints',
  owned: 'owned blueprints',
  blueprints: 'owned blueprints',
  cache: 'game data',
};

export function staleWords(reasons: string[]): string {
  const words = [...new Set(reasons.map((r) => STALE[r] ?? r))];
  return words.join(', ');
}

/** Log message prefixes from the launch hooks. */
const LOG_PREFIX: [RegExp, string][] = [
  [/^prelaunch:\s*/, 'Before launch: '],
  [/^postexit:\s*/, 'After the game closed: '],
];

/** A log line in plain words, and the raw line when it differs (shown in mono). */
export function logText(message: string): { text: string; raw: string | null } {
  let text = message;
  for (const [re, word] of LOG_PREFIX) text = text.replace(re, word);
  text = text.replace(/:\s*([a-z0-9]+(?:-[a-z0-9]+)+) target changed$/, ': a patch no longer matches the game text');
  return { text, raw: text === message ? null : message };
}

/** Why writing to the game is blocked right now, or null. */
export function blockedReason(ch: { channel: string; game_running: boolean; set_up: boolean } | null, busy: boolean): string | null {
  if (!ch) return null;
  if (ch.game_running) return `Game running: close it to apply to ${ch.channel}`;
  if (busy) return 'A job is running';
  return null;
}
