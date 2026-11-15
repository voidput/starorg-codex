<script lang="ts">
  /** Home (spec 9.4): current channel status card, facts, Apply / Remove; other channels as rows. */
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import { Button } from '@/components/ui/button';
  import * as Alert from '@/components/ui/alert';
  import ConfirmButton from '@/components/common/ConfirmButton.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import ApplyStatusPill from '@/components/shell/ApplyStatusPill.svelte';
  import ApplyButton from '@/components/shell/ApplyButton.svelte';
  import { Skeleton } from '@/components/ui/skeleton';
  import { app } from '@/lib/stores/app.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import { toast } from '@/lib/stores/toast.svelte';
  import { buildLabel, fmtDateTime, statusView } from '@/lib/format';

  const ch = $derived(app.channel);
  const others = $derived(app.channels.filter((c) => c.channel !== ch?.channel));

  async function remove() {
    if (!app.target) return;
    try {
      const r = await api.removeChanges(app.target);
      toast('success', r.result === 'nothing-to-do' ? 'Nothing to remove' : `Removed StarOrg changes from ${r.channel}`, 'The game shows its stock text again.');
    } catch (e) {
      toast('error', 'Remove failed', toContractError(e).message);
    }
    await app.refresh();
  }
</script>

{#if app.loading && !app.overview}
  <div class="flex flex-col gap-3"><Skeleton class="h-40 w-full" /><Skeleton class="h-16 w-full" /></div>
{:else if app.error}
  <ErrorState title="Could not read your game channels" error={app.error} onretry={() => app.refresh()} />
{:else if !app.install}
  <FullEmpty icon={FolderSearch} title="No Star Citizen install found">
    Looked in the LUG Helper config, default Wine prefixes, Lutris and the RSI Launcher logs.
    {#snippet actions()}<Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Find my game</Button>{/snippet}
  </FullEmpty>
{:else if ch}
  {@const s = statusView(ch.status, ch.set_up)}
  <div class="flex flex-col gap-4">
    {#if ch.status.state === 'stale' && ch.status.reasons.includes('build')}
      <Alert.Root variant="warning">
        <RefreshCw aria-hidden="true" />
        <Alert.Title>The game was updated</Alert.Title>
        <Alert.Description>Re-apply so the text matches build {buildLabel(ch.build)}.</Alert.Description>
      </Alert.Root>
    {/if}
    <article class="bg-card chamfer-md shadow-glow-sm relative border" aria-labelledby="status-title">
      <div class="flex flex-wrap items-start justify-between gap-3 border-b px-5 py-4">
        <div>
          <div class="eyebrow text-muted-foreground">Channel status</div>
          <h2 id="status-title" class="text-[16px] font-semibold">{ch.channel}</h2>
          <p class="text-muted-foreground">{s.hint}</p>
        </div>
        <ApplyStatusPill channel={ch} />
      </div>
      <dl class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1.5 px-5 py-4">
        <dt class="text-muted-foreground">Build</dt>
        <dd class="mono">{ch.build.branch ?? '—'} · {ch.build.version ?? '—'}</dd>
        <dt class="text-muted-foreground">Change</dt>
        <dd class="mono">{ch.build.change ?? 'unknown'}</dd>
        <dt class="text-muted-foreground">Install</dt>
        <dd class="mono truncate" title={app.install.root}>{app.install.root}</dd>
        <dt class="text-muted-foreground">Data.p4k</dt>
        <dd class="mono">{fmtDateTime(ch.p4k_time)}</dd>
        {#if ch.foreign_tool}
          <dt class="text-muted-foreground">Other tool</dt>
          <dd>{ch.foreign_tool}'s file is in place. Applying keeps a backup of it.</dd>
        {/if}
      </dl>
      {#if ch.game_running}
        <div class="px-5 pb-3">
          <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>The game is running on {ch.channel}. Close it to apply.</Alert.Description></Alert.Root>
        </div>
      {/if}
      <div class="flex flex-wrap items-center gap-2 border-t px-5 py-3">
        {#if ch.set_up}
          <ApplyButton label={`Apply to ${ch.channel}`} busy={app.busy} disabled={ch.game_running} onclick={() => app.apply()} />
          {#if ch.status.state !== 'not-applied'}
            <ConfirmButton variant="outline" size="sm" confirmLabel="Remove? Click again" onconfirm={remove}>Remove changes</ConfirmButton>
          {/if}
        {:else}
          <Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Set up {ch.channel}</Button>
        {/if}
      </div>
    </article>

    {#if others.length}
      <section aria-labelledby="others-title" class="flex flex-col gap-2">
        <h2 id="others-title" class="eyebrow text-muted-foreground">Other channels</h2>
        <ul class="bg-card divide-y rounded-lg border">
          {#each others as o (o.channel)}
            <li class="flex items-center gap-3 px-4 py-2">
              <span class="w-20 font-semibold">{o.channel}</span>
              <span class="mono text-muted-foreground flex-1 truncate">{buildLabel(o.build)}</span>
              <ApplyStatusPill channel={o} />
              <Button variant="ghost" size="sm" onclick={() => app.selectChannel(o.channel)}>Open</Button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
{/if}
