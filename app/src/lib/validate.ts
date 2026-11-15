/**
 * Override checks from spec 7.3, run as you type so the editor can say which rule a value breaks
 * before it is sent. The backend runs the same rules and is the authority (`set_override`).
 * Values are raw ini text: a line break is the two characters `\n`.
 */

export type RuleId = 'newline' | 'em-tags' | 'variables' | 'format-specifiers' | 'percent' | 'length';

export interface RuleCheck {
  rule: RuleId;
  ok: boolean;
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

export function checkOverride(original: string, value: string): RuleCheck[] {
  const o = percentTokens(original);
  const v = percentTokens(value);
  const formatKey = o.specs.length > 0;
  const stockSig = emSignature(original);
  const sig = emSignature(value);
  const vars = variables(original);
  const bytes = new TextEncoder().encode(value).length;
  return [
    { rule: 'newline', ok: !/[\r\n]|[\u0000-\u0008\u000b-\u001f]/.test(value), label: 'One line', message: 'A line break must be written as the two characters \\n; no other control characters.' },
    { rule: 'em-tags', ok: sig === stockSig || sig === '', label: 'EM tags', message: stockSig ? `Keep the stock tag shape (${stockSig}) or balance every <EMn> with </EMn>.` : 'Every <EMn> needs its closing </EMn>, not nested.' },
    { rule: 'variables', ok: inOrder(vars, variables(value)), label: 'Game variables', message: vars.length ? `Keep ${vars.join(', ')} in this order.` : 'No ~variable(…) tokens in the stock text.' },
    { rule: 'format-specifiers', ok: o.specs.join() === v.specs.join(), label: 'Placeholders', message: o.specs.length ? `Keep ${o.specs.join(' ')} exactly, in this order.` : 'The stock text has no %s / %d placeholders; do not add any.' },
    { rule: 'percent', ok: !(formatKey && v.literal), label: 'Percent signs', message: 'This key has placeholders, so a plain % must be written %%.' },
    { rule: 'length', ok: bytes <= 65536, label: 'Length', message: `At most 64 KiB (now ${bytes.toLocaleString('en-US')} bytes).` },
  ];
}
