/**
 * The Shops screen's state that outlives the screen for the session: the left column's mode
 * (shops or the item search), the item search's text, picked item and deal, and the command
 * palette's "Where to buy …" hand-off. Memory only: a restart starts on the shop list.
 */
import type { ItemDeal, ShopsMode } from '@/lib/shops';

class ShopsState {
  mode = $state<ShopsMode>('shops');
  /** The item search box. */
  itemQuery = $state('');
  /** The picked `ShopItemMatch.key`; null = the best match. */
  itemKey = $state<string | null>(null);
  deal = $state<ItemDeal>('buy');

  /** Palette "Where to buy …": the item search with that text, then the screen. */
  findItem(q: string, go: () => void): void {
    this.mode = 'items';
    this.itemQuery = q.trim();
    this.itemKey = null;
    this.deal = 'buy';
    go();
  }
}

export const shopsState = new ShopsState();
