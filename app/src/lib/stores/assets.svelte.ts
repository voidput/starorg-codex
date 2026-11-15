/**
 * Developer mode assets in the webview (addendum 7.16): one player for game sounds with a
 * small LRU of decoded sounds (object URLs, 8 at most; the decoded files themselves stay in the
 * asset cache on disk). Nothing here calls the backend until a sound is played.
 */
import { assetApi, soundId, type SoundRef } from '@/lib/contract/assets';
import type { ChannelRef } from '@/lib/contract/types';
import { AudioLru, base64Blob, playFailure } from '@/lib/audiolru';
import { toContractError } from '@/lib/contract/client';
import { toast } from '@/lib/stores/toast.svelte';

/** A decoded sound in the LRU: its object URL and what to say when it does not play. */
interface PlayItem {
  url: string;
  format: string;
  hint: string | null;
}

class AssetPlayer {
  /** The sound playing (its key), or null. */
  playing = $state<string | null>(null);
  /** A sound being decoded (its key), or null. */
  loading = $state<string | null>(null);
  private lru = new AudioLru<PlayItem>(8, (i) => URL.revokeObjectURL(i.url));
  private audio: HTMLAudioElement | null = null;

  stop(): void {
    this.audio?.pause();
    this.audio = null;
    this.playing = null;
  }

  async play(target: ChannelRef, sound: SoundRef, label: string, trigger = ''): Promise<void> {
    const key = soundId(sound);
    if (this.playing === key) {
      this.stop();
      return;
    }
    this.stop();
    const exportIt = { label: 'Export…', run: () => void exportOne(target, sound, trigger, label) };
    let item = this.lru.get(key);
    if (!item) {
      this.loading = key;
      try {
        const a = await assetApi.sound({ target, sound });
        if (!a.data) {
          toast('info', `${label} cannot be played here`, a.note ?? (a.format ? `${a.format} is not converted` : null), exportIt);
          return;
        }
        item = { url: URL.createObjectURL(base64Blob(a.data, a.mime)), format: a.format, hint: a.hint };
        this.lru.put(key, item);
      } catch (e) {
        toast('error', `Could not play ${label}`, toContractError(e).message);
        return;
      } finally {
        this.loading = null;
      }
    }
    const el = new Audio(item.url);
    this.audio = el;
    this.playing = key;
    el.onended = () => {
      if (this.audio === el) this.stop();
    };
    try {
      await el.play();
    } catch (e) {
      if (this.audio === el) this.stop();
      // A pause before playback started (another Play) is not an error.
      if (e instanceof DOMException && e.name === 'AbortError') return;
      toast('error', `Could not play ${label}`, playFailure(e, item.format, item.hint), exportIt);
    }
  }
}

/** Export one sound through the usual folder dialog. */
async function exportOne(target: ChannelRef, sound: SoundRef, trigger: string, label: string): Promise<void> {
  try {
    const r = await assetApi.exportSounds({ target, what: trigger || label, sounds: [{ trigger, sound }], wem: false });
    if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
    else toast('success', `Exported ${label}`, [r.note, r.path].filter(Boolean).join(' · '));
  } catch (e) {
    toast('error', 'Sound export failed', toContractError(e).message);
  }
}

export const player = new AssetPlayer();
