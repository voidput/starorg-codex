/**
 * UEX market data (Settings -> Network -> "Download UEX market data"): "Fetch now" with a toast,
 * shared by Settings and the Enhancements Shops card. The token never reaches the webview.
 */
import { api, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { toast } from '@/lib/stores/toast.svelte';

/** Downloads the UEX files; the new folder status, or null when it failed (toast says why). */
export async function fetchUexNow(): Promise<T.ObtainStatus | null> {
  try {
    const s = await api.fetchUex();
    toast('success', 'UEX market data downloaded', 'Apply again to update the Buy lines in game.');
    return s;
  } catch (e) {
    toast('error', 'UEX download failed', toContractError(e).message);
    return null;
  }
}
