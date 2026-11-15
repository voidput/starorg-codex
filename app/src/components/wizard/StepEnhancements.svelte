<script lang="ts">
  /** Preset cards with a live in-game preview (spec 9.4 step 4). */
  import { Skeleton } from '@/components/ui/skeleton';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import RadioCard from './RadioCard.svelte';
  import Database from '@lucide/svelte/icons/database';
  import { Button } from '@/components/ui/button';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import WorkingState from '@/components/common/WorkingState.svelte';
  import { api, ContractError, toContractError } from '@/lib/contract/client';
  import type { ChannelRef, InstallSummary, PreviewItem, Preset } from '@/lib/contract/types';
  import { latest, previewView, type Load } from '@/lib/loadstate';
  import { BUSY_RETRY_MS, request, type Draft } from './draft.svelte';

  let { draft, target, install = null }: { draft: Draft; target: ChannelRef | null; install?: InstallSummary | null } = $props();

  const presets: { id: Preset; title: string; text: string }[] = [
    { id: 'recommended', title: 'Recommended', text: 'Stat blocks for ships, components and weapons, name tags, mission rewards and the text fixes.' },
    { id: 'minimal', title: 'Minimal', text: 'Name tags and text fixes only. Descriptions stay as the game has them.' },
    { id: 'everything', title: 'Everything', text: 'All of Recommended, plus owned-blueprint markers in mission text.' },
  ];

  let load = $state<Load<PreviewItem[], ContractError>>({ state: 'idle' });
  let reading = $state<Load<null, ContractError>>({ state: 'idle' });
  /** Bumped after game data was read, to ask for the preview again. */
  let again = $state(0);
  const view = $derived(previewView(load));
  /** Game data is known to be read: an empty preview then means "no sample for this preset". */
  const hasData = $derived(
    reading.state === 'ok' || (!!install?.saved && install.channels.find((c) => c.channel === target?.channel)?.cache === 'fresh'),
  );
  // Primitive deps only (preset string, install/channel strings).
  const preset = $derived(draft.preset);
  const installId = $derived(target?.install ?? null);
  const channel = $derived(target?.channel ?? null);
  $effect(() => {
    void again;
    if (!installId || !channel) return;
    const l = latest<PreviewItem[], ContractError>((x) => (load = x), toContractError);
    void l.run(() => api.buildPreview({ target: { install: installId, channel }, profile: null, preset }));
    return () => l.stop();
  });

  /** Why a finished read still leaves no preview (the channel was skipped); null = it was read. */
  let readNote = $state<string | null>(null);
  const ticked = $derived(!!channel && !!draft.channels[channel]?.consent);
  // Stopped when the step closes: a read that finishes later sets nothing here (Review, which
  // asks the backend again, waits for the same read and uses it).
  let closed = false;
  const reader = latest<null, ContractError>((x) => (reading = x), toContractError);
  $effect(() => () => {
    closed = true;
    reader.stop();
  });

  /** Read the channel's game data now (the same dry run Review does), then ask again. */
  async function readNow() {
    if (!install || !channel || !ticked) return;
    const req = request(draft, install);
    // Only this channel: the others are read at Review.
    req.channels = req.channels.map((c) => ({ ...c, consent: c.channel === channel && c.consent }));
    readNote = null;
    await reader.run(async () => {
      const plan = await api.planSetup(req);
      const w = plan.warnings.find((x) => x.kind === 'game-data');
      if (w) throw new ContractError({ code: 'internal', message: w.detail, detail: null });
      if (!plan.channels.some((c) => c.channel === channel)) {
        readNote = plan.warnings.map((x) => x.detail).join(' ') || `${channel} was skipped, so its game data was not read.`;
      }
      return null;
    });
    if (closed) return;
    if (reading.state === 'error' && reading.error.code === 'busy') {
      // Another process (a launch hook, the command line) is reading: try again shortly.
      setTimeout(() => !closed && void readNow(), BUSY_RETRY_MS);
      return;
    }
    if (reading.state === 'ok') again++;
  }
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
      {#if !target}
        <p class="text-muted-foreground bg-card/80 rounded-lg border p-3">Choose a channel first; the preview shows its text.</p>
      {:else if reading.state === 'loading'}
        <WorkingState
          label="Reading game data"
          title="Reading {target.channel}'s game data from Data.p4k…"
          detail="Nothing is written to the game. The data is kept, so Review and later applies are quick."
          slow="The first read of a large Data.p4k can take a few minutes on a hard drive, or while the RSI Launcher is verifying or downloading. Leave it running."
          since={reading.since}
        />
      {:else if reading.state === 'error' && reading.error.code === 'busy'}
        <WorkingState
          label="Waiting for another read"
          title="Waiting for another game-data read to finish…"
          detail="A launch hook or the command line is reading Data.p4k right now. This continues by itself."
          since={Date.now()}
        />
      {:else if reading.state === 'error'}
        <ErrorState title="Could not read {target.channel}'s game data" error={reading.error} onretry={readNow} />
      {:else if view === 'error' && load.state === 'error'}
        <ErrorState title="Preview failed" error={load.error} onretry={() => again++} />
      {:else if view === 'loading'}
        <Skeleton class="h-28" /><Skeleton class="h-14" />
      {:else if view === 'nodata' && readNote}
        <p class="text-muted-foreground bg-card/80 rounded-lg border p-3" data-testid="preview-note">No preview: {readNote}</p>
      {:else if view === 'nodata' && hasData}
        <p class="text-muted-foreground bg-card/80 rounded-lg border p-3">This preset changes none of the sample texts (ship, component, weapon, missile and contract descriptions); the preview has nothing to show.</p>
      {:else if view === 'nodata'}
        <div class="bg-card/80 grid gap-2 rounded-lg border p-3" data-testid="preview-nodata">
          <p class="flex items-center gap-2 font-medium"><Database class="text-muted-foreground size-4" aria-hidden="true" /> No preview yet</p>
          <p class="text-muted-foreground">
            The preview is built from {target.channel}'s own game text, and StarOrg Codex has not read it on this computer yet. Reading it takes a few
            seconds to a few minutes the first time (Data.p4k is large) and writes nothing to the game. Otherwise it is read at Review.
          </p>
          {#if ticked}
            <div><Button variant="outline" size="sm" onclick={readNow} disabled={!install}><Database aria-hidden="true" /> Read game data now</Button></div>
          {:else}
            <p class="text-muted-foreground text-[12.5px]">{target.channel} is not ticked under Channels, so it is not read; tick it there to see its preview.</p>
          {/if}
        </div>
      {:else if load.state === 'ok'}
        {#each load.value as it (it.key)}
          <div class="grid gap-1">
            <div class="text-[12px] font-semibold">{it.label}</div>
            <InGamePreview text={it.result} />
          </div>
        {/each}
      {/if}
    </div>
  </section>
</div>
