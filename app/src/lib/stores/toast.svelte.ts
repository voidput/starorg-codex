/**
 * Toasts (spec 9.6): info/success in one polite live region; errors are role=alert.
 * Errors and warnings stay until dismissed (a warning from an apply is something to read);
 * info and success leave after 5 s, paused while the pointer or focus is on the stack.
 * At most 5 non-error toasts are kept; errors are never evicted.
 */
export type ToastTone = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  detail?: string | null;
  action?: { label: string; run: () => void };
}

const AUTO_MS = 5000;
const KEEP = 5;

let next = 1;
export const toasts = $state<{ list: Toast[]; paused: boolean }>({ list: [], paused: false });
const timers = new Map<number, { left: number; started: number; handle: ReturnType<typeof setTimeout> | null }>();

export function dismiss(id: number): void {
  const t = timers.get(id);
  if (t?.handle) clearTimeout(t.handle);
  timers.delete(id);
  toasts.list = toasts.list.filter((t) => t.id !== id);
}

function arm(id: number): void {
  const t = timers.get(id);
  if (!t || t.handle || toasts.paused) return;
  t.started = Date.now();
  t.handle = setTimeout(() => dismiss(id), t.left);
}

/** Hover or focus on the stack: stop the clocks; leaving starts them again with the time left. */
export function pauseToasts(paused: boolean): void {
  toasts.paused = paused;
  for (const [id, t] of timers) {
    if (paused && t.handle) {
      clearTimeout(t.handle);
      t.handle = null;
      t.left = Math.max(1000, t.left - (Date.now() - t.started));
    } else if (!paused) arm(id);
  }
}

/**
 * `autoMs`: leave after this long whatever the tone (a live alert is stale once read; an
 * error never takes it).
 */
export function toast(tone: ToastTone, title: string, detail?: string | null, action?: Toast['action'], autoMs?: number): number {
  const id = next++;
  const others = toasts.list.filter((t) => t.tone !== 'error');
  const drop = new Set(others.slice(0, Math.max(0, others.length - (KEEP - 1))).map((t) => t.id));
  for (const d of drop) dismiss(d);
  toasts.list = [...toasts.list, { id, tone, title, detail, action }];
  const auto = tone === 'error' ? undefined : (autoMs ?? (tone === 'info' || tone === 'success' ? AUTO_MS : undefined));
  if (auto !== undefined) {
    timers.set(id, { left: auto, started: Date.now(), handle: null });
    arm(id);
  }
  return id;
}
