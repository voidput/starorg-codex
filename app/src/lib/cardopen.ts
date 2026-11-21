/**
 * Whether a collapsible Explorer card is open, kept per viewer in local storage under its own
 * key ("1" open, "0" closed). Blocked or empty storage falls back to the card's default, and a
 * blocked write only loses the choice for the next session.
 */
export type OpenStore = Pick<Storage, 'getItem' | 'setItem'>;

export function loadOpen(s: OpenStore | null, key: string, fallback: boolean): boolean {
  try {
    const v = s?.getItem(key);
    return v === '1' ? true : v === '0' ? false : fallback;
  } catch {
    return fallback;
  }
}

export function saveOpen(s: OpenStore | null, key: string, open: boolean): void {
  try {
    s?.setItem(key, open ? '1' : '0');
  } catch {
    /* storage blocked: the card still opens and closes for this session */
  }
}
