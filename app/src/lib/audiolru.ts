/**
 * A small in-memory LRU of decoded sounds in the webview (addendum 7.16): object URLs by key,
 * at most `max` (default 8); the oldest is revoked when a new one comes in. The decoded files
 * themselves live in the asset cache on disk; this only saves the IPC round trip for replays.
 */
export class AudioLru {
  private items = new Map<string, string>();
  private readonly max: number;
  private readonly revoke: (url: string) => void;

  constructor(max = 8, revoke: (url: string) => void = (u) => URL.revokeObjectURL(u)) {
    this.max = max;
    this.revoke = revoke;
  }

  get(key: string): string | undefined {
    const v = this.items.get(key);
    if (v !== undefined) {
      this.items.delete(key);
      this.items.set(key, v);
    }
    return v;
  }

  put(key: string, url: string): void {
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
