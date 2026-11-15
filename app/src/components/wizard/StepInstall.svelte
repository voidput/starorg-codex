<script lang="ts">
  import { notFoundHint, onWindows, searchedWhere } from '@/lib/platform';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import ScanSearch from '@lucide/svelte/icons/scan-search';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import { Button } from '@/components/ui/button';
  import { Badge } from '@/components/ui/badge';
  import { Skeleton } from '@/components/ui/skeleton';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import RadioCard from './RadioCard.svelte';
  import type { InstallSummary } from '@/lib/contract/types';

  let {
    installs,
    scanning,
    error,
    value = $bindable(),
    onscan,
    onbrowse,
  }: {
    installs: InstallSummary[];
    scanning: boolean;
    error: { message: string; detail?: string | null } | null;
    value: string | null;
    onscan: (deep: boolean) => void;
    onbrowse: () => void;
  } = $props();
</script>

<p class="text-muted-foreground">
  {searchedWhere(onWindows)} Pick the install to use; the folder is the one that contains <span class="mono">LIVE</span>.
</p>

<div aria-live="polite" class="sr-only">{scanning ? 'Scanning for Star Citizen installs' : `${installs.length} installs found`}</div>

{#if error}
  <ErrorState title="Scanning failed" {error} onretry={() => onscan(false)} />
{:else if scanning && !installs.length}
  <div class="grid gap-2"><Skeleton class="h-24" /><Skeleton class="h-24" /></div>
{:else if !installs.length}
  <div class="bg-muted/20 flex flex-col items-center gap-2 rounded-lg border border-dashed px-6 py-8 text-center">
    <FolderSearch class="text-primary size-8" aria-hidden="true" />
    <div class="font-semibold">No Star Citizen install found</div>
    <p class="text-muted-foreground max-w-md">{notFoundHint(onWindows)}</p>
  </div>
{:else}
  <fieldset class="grid gap-2">
    <legend class="sr-only">Installs found</legend>
    {#each installs as inst (inst.id)}
      <RadioCard name="install" value={inst.id} bind:group={value}>
        <span class="flex flex-wrap items-center gap-2 pr-6">
          <span class="font-semibold">{inst.channels.length} channel{inst.channels.length === 1 ? '' : 's'}</span>
          <Badge variant="muted">found by {inst.found_by_label}</Badge>
          {#if inst.found_by === 'manual'}<Badge variant="info">Chosen folder</Badge>{/if}
        </span>
        <span class="mono text-muted-foreground break-all">{inst.root}</span>
        <span class="mt-1 flex flex-wrap gap-1.5">
          {#each inst.channels as c (c.channel)}
            <span class="bg-muted mono inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11.5px]">
              <span class="font-sans font-semibold">{c.channel}</span>{c.build.version ?? 'no build info'}
            </span>
          {/each}
        </span>
      </RadioCard>
    {/each}
  </fieldset>
{/if}

<div class="flex flex-wrap gap-2">
  <Button variant="outline" size="sm" onclick={onbrowse}><FolderOpen aria-hidden="true" /> Browse…</Button>
  <Button variant="ghost" size="sm" onclick={() => onscan(false)} disabled={scanning}><RefreshCw class={scanning ? 'animate-spin' : ''} aria-hidden="true" /> Scan again</Button>
  <Button variant="ghost" size="sm" onclick={() => onscan(true)} disabled={scanning}><ScanSearch aria-hidden="true" /> Deep scan</Button>
</div>
