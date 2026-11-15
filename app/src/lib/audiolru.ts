/**
 * A small in-memory LRU of decoded sounds in the webview (addendum 7.16): object URLs by key,
 * at most `max` (default 8); the oldest is revoked when a new one comes in. The decoded files
 * themselves live in the asset cache on disk; this only saves the IPC round trip for replays.
 */
export class AudioLru<T = string> {
  private items = new Map<string, T>();
  private readonly max: number;
  private readonly revoke: (item: T) => void;

  constructor(max = 8, revoke: (item: T) => void = (u) => URL.revokeObjectURL(String(u))) {
    this.max = max;
    this.revoke = revoke;
  }

  get(key: string): T | undefined {
    const v = this.items.get(key);
    if (v !== undefined) {
      this.items.delete(key);
      this.items.set(key, v);
    }
    return v;
  }

  put(key: string, url: T): void {
    const old = this.items.get(key);
    if (old !== undefined && old !== url) this.revoke(old);
    this.items.delete(key);
    this.items.set(key, url);
    while (this.items.size > this.max) {
      const first = this.items.keys().next().value as string;
      this.revoke(this.items.get(first)!);
      this.items.delete(first);
    }
  }

  get size(): number {
    return this.items.size;
  }

  keys(): string[] {
    return [...this.items.keys()];
  }

  clear(): void {
    for (const u of this.items.values()) this.revoke(u);
    this.items.clear();
  }
}

/** Base64 -> Blob (for an `<audio>` object URL). */
export function base64Blob(b64: string, mime: string): Blob {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: mime });
}

/**
 * The webview refusing a sound (`NotSupportedError`: on Linux WebKitGTK hands `<audio>` to
 * GStreamer) says nothing useful; name the file and what this system lacks, when known.
 */
export function playFailure(e: unknown, format: string, hint: string | null): string {
  const name = e instanceof DOMException ? e.name : '';
  if (name === 'NotSupportedError' || name === 'NotAllowedError' || !name) {
    const what = format ? `this ${format} file` : 'this sound';
    const why = hint ?? (name === 'NotAllowedError' ? 'The webview blocked playback.' : 'Its media engine reported the format as not supported.');
    return `The app's webview could not play ${what}. ${why} Export it to play it in another player.`;
  }
  return String(e);
}
