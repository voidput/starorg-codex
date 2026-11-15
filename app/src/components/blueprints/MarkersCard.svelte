<script lang="ts">
  /**
   * Markers in game: how the channel's saved profile marks blueprints ([Owned] / [Missing] on a
   * contract's blueprint lines, [BP Owned] after its title), with a jump to Enhancements →
   * Blueprints where they are set. Reads the saved profile, not an unsaved Enhancements draft.
   */
  import SlidersHorizontal from '@lucide/svelte/icons/sliders-horizontal';
  import * as Card from '@/components/ui/card';
  import { Button } from '@/components/ui/button';
  import { Skeleton } from '@/components/ui/skeleton';
  import CollapsibleCard from '@/components/common/CollapsibleCard.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, Profile } from '@/lib/contract/types';
  import { markerSummary } from '@/lib/bpmarkers';
  import { app } from '@/lib/stores/app.svelte';
  import { cn } from '@/lib/utils';

  let { target }: { target: ChannelRef } = $props();

  let profile = $state<Profile | null>(null);
  let failed = $state<string | null>(null);
  const key = $derived(`${target.install}/${target.channel}/${app.rev}`);
  $effect(() => {
    void key;
    const t = $state.snapshot(target);
    let stale = false;
    api
      .getProfile(t)
      .then((p) => {
        if (stale) return;
        profile = p;
        failed = null;
      })
      .catch((e) => !stale && (failed = toContractError(e).message));
    return () => (stale = true);
  });
  const states = $derived(profile ? markerSummary(profile) : []);

  function edit() {
    app.enhancementsTab = 'blueprints';
    app.go('enhancements');
  }
</script>

<CollapsibleCard key="blueprints.markers" id="bp-markers" class="min-w-0">
  <Card.Header>
    <Card.Title>Markers in game</Card.Title>
    <Card.Description>How contracts mark blueprints on {target.channel}: on the blueprint lines of a contract and after its title. Saved settings; Apply writes them into the game.</Card.Description>
  </Card.Header>
  <Card.Content class="grid gap-3">
    {#if failed}
      <p class="text-muted-foreground text-[12px]">{failed}</p>
    {:else if !profile}
      <Skeleton class="h-16 w-full" />
    {:else}
      <dl class="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-4 gap-y-1 text-[12.5px]" aria-label="Blueprint markers">
        {#each states as s (s.label)}
          <dt class="mono">{s.label}</dt>
          <dd class={cn('m-0', s.on ? 'text-foreground' : 'text-muted-foreground')}>{s.text}</dd>
        {/each}
      </dl>
    {/if}
    <div><Button variant="outline" size="sm" onclick={edit}><SlidersHorizontal aria-hidden="true" /> Edit blueprint markers</Button></div>
  </Card.Content>
</CollapsibleCard>
