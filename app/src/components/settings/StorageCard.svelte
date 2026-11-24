<script lang="ts">
  /**
   * Settings > Storage: what the app keeps on this PC per area (game data cache, WHERE TO GET
   * datasets, UEX, asset exports, temporary folders, backups), the cache folder's cap
   * (least recently used goes first above it), "Clear asset cache" (refused while an asset
   * job runs) and the live telemetry session history's own Clear. Sizes are measured on the
   * Rust side, off the UI thread.
   */
  import { onMount } from 'svelte';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Trash from '@lucide/svelte/icons/trash-2';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import PathText from '@/components/common/PathText.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import { telemetryApi } from '@/lib/contract/telemetry';
  import type { StorageStatus } from '@/lib/contract/types';
  import { AREA_HINTS, areaText, backupRule, capPercent, capText, sizeText } from '@/lib/storage';
  import { toast } from '@/lib/stores/toast.svelte';

  let s = $state<StorageStatus | null>(null);
  let busy = $state(false);

  /** `again`: "Measure again" walks the folders now; otherwise sizes kept from earlier serve. */
  async function load(again = false) {
    busy = true;
    try {
      s = await (again ? api.storageMeasure() : api.storageStatus());
    } catch (e) {
      toast('error', 'Reading storage sizes failed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }
  async function clearAssets() {
    busy = true;
    try {
      s = await api.storageClearAssets();
      toast('success', 'Asset cache cleared', null);
    } catch (e) {
      toast('error', 'Clearing the asset cache failed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }
  async function clearSessions() {
    busy = true;
    try {
      await telemetryApi.clearSessions();
      s = await api.storageMeasure();
      toast('success', 'Session history cleared', null);
    } catch (e) {
      toast('error', 'Clearing the session history failed', toContractError(e).message);
    } finally {
      busy = false;
    }
  }
  onMount(() => void load());
  const assets = $derived(s?.areas.find((a) => a.id === 'asset-exports') ?? null);
  const pct = $derived(s ? capPercent(s) : 0);
</script>

<Card.Root id="storage">
  <Card.Header>
    <Card.Title tabindex={-1} class="focus-visible:ring-ring/60 rounded-sm outline-none focus-visible:ring-2">Storage</Card.Title>
    <Card.Description>What the app keeps on this PC. The cache folder has a cap: above it the least recently used goes first (asset exports, then older builds), never what is in use and never backups.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3" data-storage>
    {#if s}
      <div class="grid gap-1.5">
        <div class="flex flex-wrap items-baseline gap-2">
          <span class="text-[13px] font-medium">Cache folder</span>
          <span class="text-muted-foreground text-[12px]">{capText(s)}</span>
        </div>
        <PathText path={s.cache_path} class="text-[12px]" />
        <div class="bg-muted h-1.5 overflow-hidden rounded" role="meter" aria-label="Cache folder use of its cap" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}><div class="bg-primary h-full" style="width: {pct}%"></div></div>
        <p class="text-muted-foreground text-[11.5px]">In all, with backups: {sizeText(s.total_bytes)}. The cap is <span class="mono">[storage] cache_max_mb</span> in config.toml.</p>
      </div>
      <ul class="grid gap-1.5" aria-label="Storage by area">
        {#each Array.isArray(s.areas) ? s.areas : [] as a (a.id)}
          <li class="grid gap-0.5 border-t pt-1.5" data-area={a.id}>
            <div class="flex flex-wrap items-baseline justify-between gap-2">
              <span class="text-[12.5px] font-medium">{a.label}</span>
              <span class="text-muted-foreground mono text-[12px]">{areaText(a)}</span>
            </div>
            <p class="text-muted-foreground text-[11.5px]">{AREA_HINTS[a.id]}{a.id === 'asset-exports' ? ` Up to ${sizeText(s.asset_cap_bytes)}.` : ''}{a.in_cache ? '' : ' Not counted toward the cap.'}</p>
            {#if a.id === 'backups'}<p class="text-muted-foreground text-[11.5px]">{backupRule(s)}</p>{/if}
            {#if a.id === 'telemetry-sessions' && a.bytes > 0}
              <div><ConfirmButton disabled={busy} confirmLabel="Clear the history?" onconfirm={() => void clearSessions()}><Trash aria-hidden="true" /> Clear session history</ConfirmButton></div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if s.asset_busy}<p class="text-muted-foreground text-[12px]" role="status">{s.asset_busy}</p>{/if}
    {/if}
    <div class="flex flex-wrap gap-1.5">
      <Button variant="outline" size="xs" disabled={busy} onclick={() => void load(true)}><RefreshCw aria-hidden="true" /> Measure again</Button>
      {#if s && assets}
        <ConfirmButton disabled={busy || assets.files === 0 || !!s.asset_busy} confirmLabel={`Delete ${sizeText(assets.bytes)}?`} onconfirm={() => void clearAssets()}><Trash aria-hidden="true" /> Clear asset cache</ConfirmButton>
      {/if}
    </div>
  </Card.Content>
</Card.Root>
