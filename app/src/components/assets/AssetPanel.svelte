<script lang="ts">
  /**
   * A record's sounds and model (addendum 7.16), under the Explorer's record header. Nothing
   * loads until "Sounds" is opened or "Export model…" is used; the first sounds request reads
   * the DataCore and the Wwise index in the asset worker (a few seconds), later ones are quick.
   * Exports go through native dialogs; the personal-use note sits next to every export button.
   */
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import Play from '@lucide/svelte/icons/play';
  import Square from '@lucide/svelte/icons/square';
  import Box from '@lucide/svelte/icons/box';
  import Download from '@lucide/svelte/icons/download';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import { assetApi, bytesText, DEFAULT_LOD, hasModel, MAX_LOD, PERSONAL_USE, soundId, type ModelPreset, type RecordSounds, type SoundInfo } from '@/lib/contract/assets';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import type { ChannelRef } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { player } from '@/lib/stores/assets.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { fits, gbText, limitText } from '@/lib/memory';
  import { loadMemory, memory } from '@/lib/stores/memory.svelte';

  let { target, guid, name, type }: { target: ChannelRef; guid: string; name: string; type: string } = $props();

  let openSounds = $state(false);
  $effect(() => {
    if (modelOpen) void loadMemory();
  });
  let sounds = $state<RecordSounds | null>(null);
  let soundsFor = $state('');
  let loading = $state(false);
  let error = $state<ContractError | null>(null);
  let exporting = $state(false);

  // A new record closes the panel again (no request for it until asked).
  $effect(() => {
    if (soundsFor && soundsFor !== guid) {
      openSounds = false;
      sounds = null;
      error = null;
      soundsFor = '';
    }
  });

  async function loadSounds() {
    loading = true;
    error = null;
    try {
      sounds = await assetApi.sounds({ target: $state.snapshot(target), what: guid });
      soundsFor = guid;
    } catch (e) {
      error = toContractError(e);
    } finally {
      loading = false;
    }
  }
  function toggleSounds() {
    openSounds = !openSounds;
    if (openSounds && soundsFor !== guid) void loadSounds();
  }
  const ref = (s: SoundInfo) => ({ media_id: s.media_id, source: s.source, bank: s.bank, media_path: s.media_path });

  async function exportSounds(wem: boolean) {
    exporting = true;
    try {
      const r = await assetApi.exportSounds({ target: $state.snapshot(target), what: guid, sounds: [], wem });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${r.files} sound files${r.cached ? ' from the cache' : ''}`, `${bytesText(r.bytes)} · ${r.path}${r.failed.length ? ` · ${r.failed.length} failed` : ''}`);
    } catch (e) {
      const err = toContractError(e);
      if (err.code === 'cancelled') toast('info', 'Export stopped', 'Files already written stay.');
      else toast('error', 'Sound export failed', err.message);
    } finally {
      exporting = false;
    }
  }

  // --- Model ---------------------------------------------------------------------------
  let modelOpen = $state(false);
  let preset = $state<ModelPreset>('print-shell');
  let lod = $state(0);
  let stl = $state(false);
  function setPreset(p: ModelPreset) {
    preset = p;
    lod = DEFAULT_LOD[p];
    if (p === 'full') stl = false;
  }
  /** Set when the backend says free memory is short: the dialog asks before going on. */
  let memWarn = $state<string | null>(null);
  async function exportModel(force = false) {
    modelOpen = false;
    memWarn = null;
    exporting = true;
    try {
      const r = await assetApi.exportModel({ target: $state.snapshot(target), what: guid, name, preset, lod, stl, force });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${name}${r.cached ? ' from the cache' : ''}`, `${bytesText(r.bytes)} · ${r.path}`);
    } catch (e) {
      const err = toContractError(e);
      if (err.code === 'needs-decision') memWarn = err.message;
      else if (err.code === 'cancelled') toast('info', 'Model export stopped', 'Nothing was written.');
      else toast('error', 'Model export failed', err.message);
    } finally {
      exporting = false;
    }
  }
</script>

<div class="grid min-w-0 gap-1.5" data-assets>
  <div class="flex flex-wrap items-center gap-1.5">
    <Button variant="outline" size="xs" aria-expanded={openSounds} aria-controls="rec-sounds" onclick={toggleSounds}>
      <Volume2 aria-hidden="true" /> Sounds <ChevronDown aria-hidden="true" class={openSounds ? 'rotate-180' : ''} />
    </Button>
    {#if hasModel(type)}
      <Button variant="outline" size="xs" disabled={exporting} onclick={() => (modelOpen = true)}><Box aria-hidden="true" /> Export model…</Button>
    {/if}
    {#if exporting}<JobProgress job={app.job} compact />{/if}
  </div>
  {#if openSounds}
    <div id="rec-sounds" class="bg-background/40 grid max-h-64 min-w-0 gap-1 overflow-x-hidden overflow-y-auto rounded-md border p-2 text-[12px]">
      {#if loading}
        <p class="text-muted-foreground flex items-center gap-2"><LoaderCircle class="size-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Reading the record's audio triggers… (the first time takes a few seconds)</p>
      {:else if error}
        <ErrorState title="Could not read the sounds" {error} onretry={() => void loadSounds()} />
      {:else if sounds && sounds.triggers.length === 0}
        <p class="text-muted-foreground">No audio triggers in this record{sounds.loadout ? ' or its loadout' : ''}.</p>
      {:else if sounds}
        <div class="flex flex-wrap items-center gap-2">
          <span class="text-muted-foreground">{sounds.triggers.length} triggers · {sounds.playable} of {sounds.sounds} sounds playable{sounds.loadout ? ` · includes ${sounds.loadout} loadout items` : ''}</span>
          <span class="flex-1"></span>
          <Button variant="outline" size="xs" disabled={exporting || sounds.playable === 0} onclick={() => void exportSounds(false)}><Download aria-hidden="true" /> Export .ogg…</Button>
          <Button variant="ghost" size="xs" disabled={exporting || sounds.playable === 0} onclick={() => void exportSounds(true)}>+ .wem</Button>
        </div>
        <p class="text-muted-foreground text-[11px]">{PERSONAL_USE}</p>
        <ul class="grid gap-1" aria-label="Audio triggers">
          {#each sounds.triggers as t (t.name)}
            <li class="grid min-w-0 gap-0.5 border-t pt-1 first:border-t-0 first:pt-0">
              <div class="flex min-w-0 items-baseline gap-2">
                <span class="mono truncate font-medium" title={t.property}>{t.name}</span>
                {#if t.from !== sounds.name}<span class="text-muted-foreground truncate text-[11px]">from {t.from}</span>{/if}
                {#if t.bank}<span class="text-muted-foreground mono ml-auto truncate text-[11px]">{t.bank}</span>{/if}
              </div>
              {#if t.sounds.length === 0}
                <span class="text-muted-foreground text-[11px]">No media found for this trigger.</span>
              {:else}
                <div class="flex flex-wrap gap-1">
                  {#each t.sounds as s (soundId(s))}
                    {@const k = soundId(s)}
                    <Button
                      variant={player.playing === k ? 'default' : 'outline'}
                      size="xs"
                      disabled={!s.playable}
                      title={s.playable ? `${s.source} · ${s.detail}` : s.detail}
                      aria-label={`${player.playing === k ? 'Stop' : 'Play'} ${t.name} ${s.label}`}
                      onclick={() => void player.play($state.snapshot(target), ref(s), `${t.name} ${s.label}`)}
                    >
                      {#if player.loading === k}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else if player.playing === k}<Square aria-hidden="true" />{:else}<Play aria-hidden="true" />{/if}
                      <span class="mono">{s.label}</span>
                    </Button>
                  {/each}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

<Dialog.Root bind:open={modelOpen}>
  <Dialog.Content class="max-w-md">
    <Dialog.Header>
      <Dialog.Title>Export model: {name}</Dialog.Title>
      <Dialog.Description>A GLB through StarBreaker's exporter, in its own process; its memory is returned when it ends.</Dialog.Description>
    </Dialog.Header>
    <div class="grid gap-3 text-[12.5px]">
      <Segmented
        ariaLabel="Preset"
        value={preset}
        onchange={setPreset}
        options={[
          { id: 'print-shell', label: 'Print shell (geometry only)', hint: 'No materials, no interior, no lights; LOD 0. About 1.5-2 GB of memory for a capital ship.' },
          { id: 'full', label: 'Full (textures)', hint: 'Materials and textures, interiors, lights. Up to about 11 GB of memory and 30 s for a capital ship.' },
        ]}
      />
      <label class="flex items-center gap-2">
        <span>Level of detail</span>
        <select class="border-input bg-background h-7 rounded-md border px-2" bind:value={lod} aria-label="Level of detail">
          {#each Array.from({ length: MAX_LOD + 1 }, (_, i) => i) as n (n)}<option value={n}>LOD {n}{n === 0 ? ' (most detail)' : ''}</option>{/each}
        </select>
      </label>
      <SwitchRow id="model-stl" label="Also write an STL" hint="Geometry with the node transforms baked in, Z-up, next to the GLB." checked={stl} onchange={(v) => (stl = v)} disabled={preset === 'full'} />
      {#if preset === 'full'}<p class="text-amber-700 dark:text-amber-300">Full exports of large ships need a lot of memory (Javelin about 6.5 GB, Idris about 11 GB).</p>{/if}
      {#if memory.status}
        {@const need = memory.status.estimates.find((e) => e.id === (preset === 'full' ? 'model-full' : 'model-print'))}
        {@const over = !!need && !fits(need.bytes, memory.status.limit_gb, memory.status.stop_percent)}
        <p class={over ? 'text-destructive' : 'text-muted-foreground'} data-memory-reminder>
          Memory limit {limitText(memory.status.limit_gb)}{need ? (over ? `: this preset needs about ${gbText(need.bytes)}, so it will be refused. Raise the limit under Settings > Advanced.` : `: this preset needs about ${gbText(need.bytes)}${preset === 'full' ? ' (capital ships about 11 GB)' : ''}.`) : '.'}
        </p>
      {/if}
      <p class="text-muted-foreground text-[11.5px]">{PERSONAL_USE}</p>
    </div>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (modelOpen = false)}>Cancel</Button>
      <Button onclick={() => void exportModel()}><Download aria-hidden="true" /> Choose file…</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root open={memWarn !== null} onOpenChange={(o) => !o && (memWarn = null)}>
  <Dialog.Content class="max-w-md">
    <Dialog.Header>
      <Dialog.Title>Not much memory free</Dialog.Title>
      <Dialog.Description>{memWarn}</Dialog.Description>
    </Dialog.Header>
    <p class="text-muted-foreground text-[12.5px]">The export runs in its own process; if memory runs out, that process stops and the app keeps running. Closing other programs (or the game) first helps.</p>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (memWarn = null)}>Cancel</Button>
      <Button onclick={() => void exportModel(true)}>Export anyway</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
