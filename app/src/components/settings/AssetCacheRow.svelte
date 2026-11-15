<script lang="ts">
  /**
   * Settings > Advanced > Asset cache (addendum 7.16): where decoded sounds, exported models and
   * texture previews are kept (default: the app's cache folder), its size and cap, Browse…
   * (validated: writable, not in a game folder), Reset to default, Clear. After a switch the
   * old folder keeps its files until "Move existing cache" (or it is left alone).
   */
  import { onMount } from 'svelte';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Trash from '@lucide/svelte/icons/trash-2';
  import MoveRight from '@lucide/svelte/icons/move-right';
  import { Button } from '@/components/ui/button';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import PathText from '@/components/common/PathText.svelte';
  import { assetApi, bytesText, type AssetCacheStatus } from '@/lib/contract/assets';
  import { toContractError } from '@/lib/contract/client';
  import { toast } from '@/lib/stores/toast.svelte';

  let s = $state<AssetCacheStatus | null>(null);
  let busy = $state(false);
  let leftAlone = $state('');

  async function run(what: string, f: () => Promise<AssetCacheStatus>, done?: (x: AssetCacheStatus) => string | null) {
    busy = true;
    try {
      const before = s;
      s = await f();
      const msg = done?.(s);
      if (msg) toast('success', msg, null);
      if (before && s.path !== before.path) leftAlone = '';
    } catch (e) {
      toast('error', `${what} failed`, toContractError(e).message);
    } finally {
      busy = false;
    }
  }
  onMount(() => void run('Reading the asset cache', assetApi.cacheStatus));
  const pct = $derived(s && s.max_bytes ? Math.min(100, Math.round((s.bytes / s.max_bytes) * 100)) : 0);
</script>

<div class="grid gap-1.5 border-t pt-3" data-asset-cache>
  <div class="flex flex-wrap items-baseline gap-2">
    <span class="text-[13px] font-medium">Asset cache</span>
    {#if s}<span class="text-muted-foreground text-[12px]">{bytesText(s.bytes)} of {bytesText(s.max_bytes)} · {s.files} files{s.is_default ? ' · default folder' : ''}</span>{/if}
  </div>
  <p class="text-muted-foreground text-[12px]">Decoded sounds, exported models and texture previews, kept per build so they are not decoded again; a game patch starts over. The oldest go first when it is full.</p>
  {#if s}
    <PathText path={s.path} class="text-[12px]" />
    <div class="bg-muted h-1.5 overflow-hidden rounded" role="meter" aria-label="Asset cache use" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}><div class="bg-primary h-full" style="width: {pct}%"></div></div>
    {#if s.problem}<p class="text-destructive text-[12px]" role="alert">{s.problem}</p>{/if}
    {#if s.other_drive}<p class="text-muted-foreground text-[11.5px]">On another drive than the app's own folders. That is fine.</p>{/if}
    <div class="flex flex-wrap gap-1.5">
      <Button variant="outline" size="xs" disabled={busy} onclick={() => void run('Changing the cache folder', assetApi.cachePick)}><FolderOpen aria-hidden="true" /> Browse…</Button>
      <Button variant="outline" size="xs" disabled={busy || s.is_default} onclick={() => void run('Reset', assetApi.cacheReset, () => 'Asset cache back in the default folder')}><RotateCcw aria-hidden="true" /> Reset to default</Button>
      <ConfirmButton disabled={busy || s.files === 0} confirmLabel={`Delete ${bytesText(s.bytes)}?`} onconfirm={() => void run('Clearing the cache', assetApi.cacheClear, () => 'Asset cache cleared')}><Trash aria-hidden="true" /> Clear</ConfirmButton>
    </div>
    {#if s.previous && leftAlone !== s.previous.path}
      <div class="bg-background/60 grid gap-1.5 rounded-md border p-2 text-[12px]" role="status">
        <p>The old cache folder still holds {s.previous.files} files ({bytesText(s.previous.bytes)}): <span class="mono break-all">{s.previous.path}</span>. Nothing there is deleted.</p>
        <div class="flex flex-wrap gap-1.5">
          <Button size="xs" disabled={busy} onclick={() => void run('Moving the cache', assetApi.cacheMove, (x) => `Moved to ${x.path}`)}><MoveRight aria-hidden="true" /> Move existing cache</Button>
          <Button variant="ghost" size="xs" onclick={() => (leftAlone = s?.previous?.path ?? '')}>Leave it</Button>
        </div>
      </div>
    {/if}
  {/if}
</div>
