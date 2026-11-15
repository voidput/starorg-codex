/** Toasts (spec 9.6: one polite live region; failures are role=alert). */
export type ToastTone = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  detail?: string | null;
  action?: { label: string; run: () => void };
}

let next = 1;
export const toasts = $state<{ list: Toast[] }>({ list: [] });

export function dismiss(id: number): void {
  toasts.list = toasts.list.filter((t) => t.id !== id);
}

export function toast(tone: ToastTone, title: string, detail?: string | null, action?: Toast['action']): number {
  const id = next++;
  toasts.list = [...toasts.list.slice(-3), { id, tone, title, detail, action }];
  // Errors stay until dismissed; the rest leave on their own.
  if (tone !== 'error') setTimeout(() => dismiss(id), tone === 'warning' ? 8000 : 5000);
  return id;
}
