/**
 * Optional disclosure for a card (CollapsibleCard sets it): the title becomes the toggle, the
 * description and content hide while collapsed. Without the context the card parts are unchanged.
 * Every Card.Title / Card.Content / Card.Description below the provider follows it, so a
 * collapsible card should not hold other cards.
 */
import { getContext, setContext } from 'svelte';

export interface CardCollapse {
  readonly open: boolean;
  set(open: boolean): void;
  readonly contentId: string;
}

const KEY = Symbol('card-collapse');

export const setCardCollapse = (c: CardCollapse) => setContext(KEY, c);
export const getCardCollapse = () => getContext<CardCollapse | undefined>(KEY);
