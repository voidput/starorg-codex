/**
 * Support and donation links (Discord, Buy me a coffee, Patreon) (Footer, About). The webview has no opener permission: it names
 * an allowlisted link and Rust opens the fixed URL (`open_link`, contract/types.rs). The URLs
 * here are for display only.
 */
import { api, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { toast } from '@/lib/stores/toast.svelte';

export const LINKS: Record<T.ExternalLink, { label: string; short: string; url: string; title: string }> = {
  support: { label: 'Support', short: 'Support', url: 'https://discord.gg/GCkYhbFYNq', title: 'Support on Discord (opens your browser)' },
  donate: { label: 'Buy me a coffee', short: 'Coffee', url: 'https://buymeacoffee.com/zedwil', title: 'Buy me a coffee (opens your browser)' },
  patreon: { label: 'Patreon', short: 'Patreon', url: 'https://www.patreon.com/cw/StarOrgTools', title: 'StarOrg Tools on Patreon (opens your browser)' },
  uex: { label: 'UEX Corp', short: 'UEX', url: 'https://uexcorp.space', title: 'UEX Corp: sign in, then My Apps for a token (opens your browser)' },
};

export async function openLink(link: T.ExternalLink): Promise<void> {
  try {
    await api.openLink(link);
  } catch (e) {
    toast('error', `Could not open ${LINKS[link].label}`, `${toContractError(e).message} The link is ${LINKS[link].url}`);
  }
}
