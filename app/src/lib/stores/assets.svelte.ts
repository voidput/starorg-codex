/**
 * Developer mode assets in the webview (addendum 7.16): one player for game sounds with a
 * small LRU of decoded sounds (object URLs, 8 at most; the decoded files themselves stay in the
 * asset cache on disk). Nothing here calls the backend until a sound is played.
 */
import { assetApi, soundId, type SoundRef } from '@/lib/contract/assets';
import type { ChannelRef } from '@/lib/contract/types';
import { AudioLru, base64Blob } from '@/lib/audiolru';
import { toContractError } from '@/lib/contract/client';
import { toast } from '@/lib/stores/toast.svelte';

class AssetPlayer {
  /** The sound playing (its key), or null. */
  playing = $state<string | null>(null);
  /** A sound being decoded (its key), or null. */
  loading = $state<string | null>(null);
  private lru = new AudioLru(8);
  private audio: HTMLAudioElement | null = null;

  stop(): void {
    this.audio?.pause();
    this.audio = null;
    this.playing = null;
  }

  async play(target: ChannelRef, sound: SoundRef, label: string): Promise<void> {
    const key = soundId(sound);
    if (this.playing === key) {
      this.stop();
      return;
    }
    this.stop();
    let url = this.lru.get(key);
    if (!url) {
      this.loading = key;
      try {
        const a = await assetApi.sound({ target, sound });
        if (!a.data) {
          toast('info', `${label} cannot be played here`, a.note);
          return;
        }
        url = URL.createObjectURL(base64Blob(a.data, a.mime));
        this.lru.put(key, url);
      } catch (e) {
        toast('error', `Could not play ${label}`, toContractError(e).message);
        return;
      } finally {
        this.loading = null;
      }
    }
    const el = new Audio(url);
    this.audio = el;
    this.playing = key;
    el.onended = () => {
      if (this.audio === el) this.stop();
    };
    try {
      await el.play();
    } catch (e) {
      if (this.audio === el) this.stop();
      toast('error', `Could not play ${label}`, String(e));
    }
  }
}

export const player = new AssetPlayer();
