/**
 * Override checks from spec 7.3, run as you type so the editor can say which rule a value breaks
 * before it is sent. The backend runs the same rules and is the authority (`set_override`).
 * Values are raw ini text: a line break is the two characters `\n`.
 */

import type { OverrideRule } from './contract/types';
import { RULE_TEXT } from './messages';

/** Rule ids are M1's `validate::Rule::name()` (the contract's OverrideRule). */
export type RuleId = OverrideRule;

export interface RuleCheck {
  rule: RuleId;
  ok: boolean;
  /** Plain words, from the message catalogue: the check's name, and the problem when it fails. */
  check: string;
  label: string;
  message: string;
}

const SPEC = /%[-+ 0#]*\d*(?:\.\d+)?(?:ll|l|h)?[sSdiufFgGxXc]/y;

/** Format specifiers in order, and whether a literal (unescaped, non-specifier) % is present. */
export function percentTokens(s: string): { specs: string[]; literal: boolean } {
  const specs: string[] = [];
  let literal = false;
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== '%') continue;
    if (s[i + 1] === '%') {
      i++;
      continue;
    }
    SPEC.lastIndex = i;
    const m = SPEC.exec(s);
    if (m) {
      specs.push(m[0]);
      i += m[0].length - 1;
    } else literal = true;
  }
  return { specs, literal };
}

export const variables = (s: string): string[] => s.match(/~[A-Za-z]+\([^)]*\)/g) ?? [];

/** Per EM number: unmatched opens, unmatched closes, nested opens (spec 7.3 EM signature). */
export function emSignature(s: string): string {
  const sig = new Map<string, [number, number, number]>();
  const open = new Map<string, number>();
  let depth = 0;
  for (const m of s.matchAll(/<(\/?)EM(\d)>/g)) {
    const n = m[2]!;
    const e = sig.get(n) ?? [0, 0, 0];
    if (!m[1]) {
      if (depth > 0) e[2]++;
      open.set(n, (open.get(n) ?? 0) + 1);
      depth++;
    } else if ((open.get(n) ?? 0) > 0) {
      open.set(n, open.get(n)! - 1);
      depth--;
    } else e[1]++;
    sig.set(n, e);
  }
  for (const [n, c] of open) if (c) sig.get(n)![0] += c;
  return [...sig]
    .filter(([, v]) => v.some((x) => x))
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([n, v]) => `EM${n}:${v.join('/')}`)
    .join(' ');
}

function inOrder(needles: string[], hay: string[]): boolean {
  let j = 0;
  for (const h of hay) if (h === needles[j]) j++;
  return j === needles.length;
}

/** Count of each opening <EMn> tag. */
function emOpens(s: string): Map<string, number> {
  const m = new Map<string, number>();
  for (const x of s.matchAll(/<EM(\d)>/g)) m.set(x[1]!, (m.get(x[1]!) ?? 0) + 1);
  return m;
}

export function checkOverride(original: string, value: string): RuleCheck[] {
  const o = percentTokens(original);
  const v = percentTokens(value);
  const formatKey = o.specs.length > 0;
  const stockSig = emSignature(original);
  const sig = emSignature(value);
  const stockOpens = emOpens(original);
  const opens = emOpens(value);
  const vars = variables(original);
  const bytes = new TextEncoder().encode(value).length;
  const c = (rule: RuleId, ok: boolean, message: string = RULE_TEXT[rule].message): RuleCheck => ({ rule, ok, check: RULE_TEXT[rule].check, label: RULE_TEXT[rule].label, message });
  return [
    // eslint-disable-next-line no-control-regex
    c('control-character', !/[\u0000-\u001f\u007f]/.test(value)),
    c('added-em-unbalanced', sig === '' || sig === stockSig, 'Every <EMn> you add needs its closing </EMn>, not nested.'),
    c('em-signature', [...stockOpens].every(([n, k]) => (opens.get(n) ?? 0) >= k), stockSig || stockOpens.size ? "Keep the original's highlight tags." : 'The original has no highlight tags.'),
    c('variable-tokens', inOrder(vars, variables(value)), vars.length ? `Keep ${vars.join(', ')} in this order.` : 'The original has no ~variable(...) tokens.'),
    c('format-specifiers', o.specs.join() === v.specs.join(), o.specs.length ? `Keep ${o.specs.join(' ')} exactly, in this order.` : 'The original has no %s / %d placeholders; do not add any.'),
    c('format-key', !(formatKey && v.literal), 'This key has placeholders, so a plain % must be written %%.'),
    c('percent-rule', formatKey || !/% [A-Za-z]/.test(value)),
    c('too-long', bytes <= 65536, `At most 64 KiB (now ${bytes.toLocaleString('en-US')} bytes).`),
  ];
}
