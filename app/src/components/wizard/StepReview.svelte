<script lang="ts">
  /** Review with jump-to-edit (spec 9.4 step 5): target path and link target, counts, backup location, the user.cfg line. */
  import Pencil from '@lucide/svelte/icons/pencil';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import * as Alert from '@/components/ui/alert';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { InstallSummary, SetupPlan } from '@/lib/contract/types';
  import { int } from '@/lib/format';
  import { request, type Draft, type StepId } from './draft.svelte';

  let { draft, install, onedit }: { draft: Draft; install: InstallSummary; onedit: (s: StepId) => void } = $props();

  let plan = $state<SetupPlan | null>(null);
  let error = $state<ContractError | null>(null);
  const reqJson = $derived(JSON.stringify(request(draft, install)));

  async function load(json: string) {
    error = null;
    try {
      plan = await api.planSetup(JSON.parse(json));
    } catch (e) {
      error = toContractError(e);
    }
  }
  $effect(() => {
    void load(reqJson);
  });

  const picked = $derived(install.channels.filter((c) => draft.channels[c.channel]?.consent));
  const presetName = $derived({ recommended: 'Recommended', minimal: 'Minimal', everything: 'Everything' }[draft.preset]);
</script>

{#snippet edit(step: StepId, what: string)}
  <Button variant="ghost" size="xs" onclick={() => onedit(step)} aria-label="Edit {what}"><Pencil aria-hidden="true" /> Edit</Button>
{/snippet}

<dl class="bg-card/80 divide-y rounded-lg border">
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Game</dt>
    <dd class="mono min-w-0 break-all">{install.root}</dd>
    {@render edit('install', 'game')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Channels</dt>
    <dd>
      {picked.map((c) => c.channel).join(', ') || 'None'}
      {#if install.channels.length > picked.length}<span class="text-muted-foreground"> · not set up: {install.channels.filter((c) => !picked.includes(c)).map((c) => c.channel).join(', ')}</span>{/if}
    </dd>
    {@render edit('channels', 'channels')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Game data</dt>
    <dd>{draft.data_source === 'local' ? 'Read on this computer (no network)' : 'Published game data when available (network)'}</dd>
    {@render edit('source', 'game data source')}
  </div>
  <div class="grid grid-cols-[9rem_1fr_auto] items-start gap-3 px-3 py-2">
    <dt class="text-muted-foreground">Enhancements</dt>
    <dd>{presetName}{draft.fix_known_bugs ? ', with text fixes' : ', no text fixes'}</dd>
    {@render edit('enhancements', 'enhancements')}
  </div>
</dl>

<section aria-labelledby="writes-title" class="grid gap-2">
  <h3 id="writes-title" class="eyebrow text-muted-foreground">What will be written</h3>
  {#if error}
    <ErrorState title="Could not prepare the review" {error} onretry={() => load(reqJson)} />
  {:else if !plan}
    <Skeleton class="h-32" />
  {:else}
    {#each plan.warnings as w (w.kind + w.detail)}
      <Alert.Root variant="warning"><TriangleAlert aria-hidden="true" /><Alert.Description>{w.detail}</Alert.Description></Alert.Root>
    {/each}
    {#each plan.channels as c (c.channel)}
      <article class="bg-card/80 rounded-lg border" aria-label="{c.channel} plan">
        <header class="flex items-baseline justify-between border-b px-3 py-2">
          <span class="font-semibold">{c.channel}</span>
          <span class="text-muted-foreground"><span class="mono text-foreground">{int(c.counts.changed)}</span> strings changed</span>
        </header>
        <dl class="grid grid-cols-[9rem_1fr] gap-x-3 gap-y-1 px-3 py-2">
          <dt class="text-muted-foreground">Writes</dt>
          <dd class="mono break-all">{c.target}{#if c.link_target}<span class="text-muted-foreground"> → {c.link_target}</span>{/if}</dd>
          <dt class="text-muted-foreground">Backups</dt>
          <dd class="mono break-all">{c.backup_dir}</dd>
          <dt class="text-muted-foreground">user.cfg</dt>
          <dd>{#if c.user_cfg_line}adds <span class="mono">{c.user_cfg_line}</span>{:else}No change needed{/if}</dd>
          {#if c.replaces_tool}
            <dt class="text-muted-foreground">Replaces</dt>
            <dd>{c.replaces_tool}'s file (backed up first, kept for good)</dd>
          {/if}
          <dt class="text-muted-foreground">Counts</dt>
          <dd class="mono text-muted-foreground">
            ships {int(c.counts.ships)} · components {int(c.counts.components)} · ship weapons {int(c.counts.ship_weapons)} · FPS {int(c.counts.fps_weapons)} · missiles {int(c.counts.missiles)} · missions {int(c.counts.missions)}
          </dd>
        </dl>
      </article>
    {/each}
  {/if}
</section>
