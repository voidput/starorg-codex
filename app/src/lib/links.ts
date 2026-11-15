/**
 * Support and donation links (Footer, About). The webview has no opener permission: it names
 * an allowlisted link and Rust opens the fixed URL (`open_link`, contract/types.rs). The URLs
 * here are for display only.
 */
import { api, toContractError } from '@/lib/contract/client';
import type * as T from '@/lib/contract/types';
import { toast } from '@/lib/stores/toast.svelte';

export const LINKS: Record<T.ExternalLink, { label: string; short: string; url: string; title: string }> = {
  support: { label: 'Support', short: 'Support', url: 'https://discord.gg/GCkYhbFYNq', title: 'Support on Discord (opens your browser)' },
  donate: { label: 'Buy me a coffee', short: 'Coffee', url: 'https://buymeacoffee.com/zedwil', title: 'Buy me a coffee (opens your browser)' },
};

export async function openLink(link: T.ExternalLink): Promise<void> {
  try {
    await api.openLink(link);
  } catch (e) {
    toast('error', `Could not open ${LINKS[link].label}`, `${toContractError(e).message} The link is ${LINKS[link].url}`);
  }
}
