<script lang="ts">
  /**
   * Settings -> Network -> Download UEX market data (spec obtain-hints). The switch is the
   * consent; with it on: the optional UEX token (saved in the keyring or a private file, never
   * shown again), "Fetch now" and the age of the three files in the UEX folder.
   */
  import Download from '@lucide/svelte/icons/download';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ObtainStatus, UexTokenStatus } from '@/lib/contract/types';
  import { LINKS, openLink } from '@/lib/links';
  import { uexFilesText, uexTokenText } from '@/lib/obtain';
  import { toast } from '@/lib/stores/toast.svelte';
  import { fetchUexNow } from '@/lib/uex';
  import { cn } from '@/lib/utils';

  let { checked, onchange }: { checked: boolean; onchange: (v: boolean) => void } = $props();

  let token = $state<UexTokenStatus | null>(null);
  let files = $state<ObtainStatus | null>(null);
  let draft = $state('');
  let saving = $state(false);
  let fetching = $state(false);

  $effect(() => {
    if (!checked) return;
    let stale = false;
    api
      .uexTokenStatus()
      .then((t) => !stale && (token = t))
      .catch(() => !stale && (token = null));
    api
      .obtainStatus()
      .then((s) => !stale && (files = s))
      .catch(() => !stale && (files = null));
    return () => (stale = true);
  });

  async function saveToken(value: string | null) {
    saving = true;
    try {
      token = await api.setUexToken(value);
      draft = '';
      toast('success', value ? uexTokenText(token) : 'UEX token removed');
    } catch (e) {
      toast('error', value ? 'Token not saved' : 'Token not removed', toContractError(e).message);
    } finally {
      saving = false;
    }
  }

  async function fetchNow() {
    fetching = true;
    const s = await fetchUexNow();
    if (s) files = s;
    fetching = false;
  }
</script>

<SwitchRow
  id="net-uex"
  label="Download UEX market data"
  hint="Lets Fetch now download where items are sold and what commodity kiosks pay from api.uexcorp.space (UEX Corp, player-reported) into the UEX folder, for the Buy and Sell lines of Where to get. Sends the app's name and version, and your UEX token if you save one; nothing else. Shop names go into the game text without prices; sell prices only when you turn them on (rounded, with the fetch day)."
  {checked}
  {onchange}
/>
{#if checked}
  <div class="mb-1 ml-11.5 grid gap-2.5 border-l pl-3">
    <div class="grid gap-1">
      <label for="net-uex-token" class="text-[13px]">UEX token (optional)</label>
      <form
        class="flex max-w-md gap-1.5"
        onsubmit={(e) => {
          e.preventDefault();
          if (draft.trim()) void saveToken(draft);
        }}
      >
        <Input id="net-uex-token" type="password" class="mono h-7 text-[12px]" bind:value={draft} placeholder={token?.set ? 'Saved; type a new one to replace it' : 'Paste a token'} spellcheck={false} autocomplete="off" aria-describedby="net-uex-token-hint" />
        <Button type="submit" variant="outline" size="sm" disabled={saving || !draft.trim()}>Save</Button>
        {#if token?.set}<Button type="button" variant="ghost" size="sm" disabled={saving} onclick={() => saveToken(null)}>Remove</Button>{/if}
      </form>
      <p id="net-uex-token-hint" class="text-muted-foreground text-[12px]">
        <span role="status">{uexTokenText(token)}.</span>
        Get a token:
        <button type="button" class="text-primary underline-offset-2 hover:underline" title={LINKS.uex.title} onclick={() => openLink('uex')}>uexcorp.space</button>
        → My Apps. Kept in the system keyring (or a private file when there is none), sent only to api.uexcorp.space.
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="outline" size="xs" disabled={fetching} onclick={fetchNow}><Download class={cn(fetching && 'animate-pulse')} aria-hidden="true" /> {fetching ? 'Downloading…' : 'Fetch now'}</Button>
      {#if files}<p class="text-muted-foreground text-[12px]" role="status">{uexFilesText(files)}</p>{/if}
    </div>
    {#if files}<p class="text-muted-foreground text-[12px]">Into <span class="mono select-all break-all">{files.uex_folder}</span>, shared with the Explorer Sources card and the local MCP server. Apply again afterwards.</p>{/if}
  </div>
{/if}
