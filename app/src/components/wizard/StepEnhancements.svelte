<script lang="ts">
  /** Preset cards with a live in-game preview (spec 9.4 step 4). */
  import { Skeleton } from '@/components/ui/skeleton';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import RadioCard from './RadioCard.svelte';
  import { api, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, PreviewItem, Preset } from '@/lib/contract/types';
  import type { Draft } from './draft.svelte';

  let { draft, target }: { draft: Draft; target: ChannelRef | null } = $props();

  const presets: { id: Preset; title: string; text: string }[] = [
    { id: 'recommended', title: 'Recommended', text: 'Stat blocks for ships, components and weapons, name tags, mission rewards and the text fixes.' },
    { id: 'minimal', title: 'Minimal', text: 'Name tags and text fixes only. Descriptions stay as the game has them.' },
    { id: 'everything', title: 'Everything', text: 'All of Recommended, plus owned-blueprint markers in mission text.' },
  ];

  let items = $state<PreviewItem[] | null>(null);
  let err = $state<string | null>(null);
  // Primitive deps only (preset string, install/channel strings).
  const preset = $derived(draft.preset);
  const install = $derived(target?.install ?? null);
  const channel = $derived(target?.channel ?? null);
  $effect(() => {
    if (!install || !channel) return;
    let live = true;
    err = null;
    api
      .buildPreview({ target: { install, channel }, profile: null, preset })
      .then((r) => live && (items = r))
      .catch((e) => live && (err = toContractError(e).message));
    return () => {
      live = false;
    };
  });
</script>

<div class="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)]">
  <div class="grid content-start gap-3">
    <fieldset class="grid gap-2">
      <legend class="sr-only">Preset</legend>
      {#each presets as p (p.id)}
        <RadioCard name="preset" value={p.id} bind:group={draft.preset}>
          <span class="pr-6 font-semibold">{p.title}</span>
          <span class="text-muted-foreground">{p.text}</span>
        </RadioCard>
      {/each}
    </fieldset>
    <SwitchRow
      id="fix-bugs"
      label="Fix known CIG text bugs"
      hint="Built-in patches for typos and missing lines. Each is checked against your build and skipped when it no longer matches."
      checked={draft.fix_known_bugs}
      onchange={(v) => (draft.fix_known_bugs = v)}
    />
    <p class="text-muted-foreground text-[12px]">Every switch can be changed later under Enhancements, per channel.</p>
  </div>
  <section aria-labelledby="preview-title" class="grid content-start gap-2">
    <h3 id="preview-title" class="eyebrow text-muted-foreground">In-game preview{target ? ` · ${target.channel}` : ''}</h3>
    <div aria-live="polite" class="grid gap-3">
      {#if err}
        <p class="text-destructive" role="alert">Preview failed: {err}</p>
      {:else if !items}
        <Skeleton class="h-28" /><Skeleton class="h-14" />
      {:else}
        {#each items as it (it.key)}
          <div class="grid gap-1">
            <div class="text-[12px] font-semibold">{it.label}</div>
            <InGamePreview text={it.result} />
          </div>
        {/each}
      {/if}
    </div>
  </section>
</div>
