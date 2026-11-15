<script lang="ts">
  /**
   * The Explorer's Model tab (addendum 7.20): an entity's Print shell (geometry only) in 3D, with
   * a detail slider from LOD 4 (least) to LOD 0 (most). Loaded on demand by ExplorerScreen, so
   * three.js (modelscene.ts) is its own chunk.
   *
   * Nothing is exported until "Load" (a cached LOD shows at once, without a worker). Each LOD is
   * exported once into the asset cache in a fresh worker, then handed over in chunks as raw
   * binary. The memory limit applies as for exports: a step over it is refused, one over the
   * free memory asks first.
   */
  import { onDestroy, untrack } from 'svelte';
  import Box from '@lucide/svelte/icons/box';
  import Download from '@lucide/svelte/icons/download';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import Grid3x3 from '@lucide/svelte/icons/grid-3x3';
  import Square from '@lucide/svelte/icons/square';
  import * as Dialog from '@/components/ui/dialog';
  import { Button } from '@/components/ui/button';
  import { Progress } from '@/components/ui/progress';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import { assetApi, PERSONAL_USE, type AssetModelInfo, type AssetModelView } from '@/lib/contract/assets';
  import { toContractError } from '@/lib/contract/client';
  import type { ChannelRef } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { gbText } from '@/lib/memory';
  import { cn } from '@/lib/utils';
  import { fetchGlb, FetchCancelled, lodLabel, lodToSlider, mbText, MAX_LOD, sizeText, sliderToLod, stepNote, stepState, trianglesText, webglProblem, type FetchProgress } from '@/lib/modelview';
  import { ModelScene, parseGlb, type ModelStats } from '@/lib/modelscene';

  let { target, guid, name }: { target: ChannelRef; guid: string; name: string } = $props();

  type Phase = 'idle' | 'exporting' | 'loading' | 'building' | 'ready';
  let info = $state<AssetModelInfo | null>(null);
  let infoError = $state<{ message: string; detail?: string | null } | null>(null);
  let slider = $state(lodToSlider(3));
  let phase = $state<Phase>('idle');
  let fetched = $state<FetchProgress | null>(null);
  let error = $state<{ message: string; detail?: string | null } | null>(null);
  let memAsk = $state<string | null>(null);
  let wire = $state(false);
  let noGl = $state<string | null>(null);
  let shown = $state<{ lod: number; view: AssetModelView; stats: ModelStats } | null>(null);
  let exporting = $state(false);

  let host = $state<HTMLDivElement | null>(null);
  /** Height of the readouts over the canvas: the model is fitted above them. */
  let readoutH = $state(0);
  let scene: ModelScene | null = null;
  /** Bumped by each load and each record change; an older load's result is dropped. */
  let token = 0;
  let cancelled = false;

  const lod = $derived(sliderToLod(slider));
  const step = $derived(info?.lods[lod] ?? null);
  const stepKind = $derived(step && info ? stepState(step, info.max_bytes) : null);
  const busy = $derived(phase === 'exporting' || phase === 'loading' || phase === 'building');
  const theme = (): 'dark' | 'light' => (document.documentElement.dataset.theme === 'light' ? 'light' : 'dark');

  async function loadInfo(forGuid: string): Promise<AssetModelInfo | null> {
    try {
      const i = await assetApi.modelInfo({ target: $state.snapshot(target), what: forGuid, name });
      if (forGuid !== guid) return null;
      info = i;
      infoError = null;
      return i;
    } catch (e) {
      if (forGuid === guid) infoError = toContractError(e);
      return null;
    }
  }

  // A new record: forget the last one, read its steps (no worker), show LOD 3 if cached.
  $effect(() => {
    const g = guid;
    untrack(() => {
      // An export still running for the last record is stopped (one export at a time).
      if (phase === 'exporting') void app.cancel();
      token++;
      cancelled = true;
      info = null;
      infoError = null;
      shown = null;
      error = null;
      memAsk = null;
      phase = 'idle';
      fetched = null;
      void loadInfo(g).then((i) => {
        if (!i) return;
        slider = lodToSlider(i.default_lod);
        if (i.lods[i.default_lod]?.cached) void load(false);
      });
    });
  });

  // The canvas follows the app theme.
  $effect(() => {
    const mo = new MutationObserver(() => scene?.setTheme(theme()));
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    return () => mo.disconnect();
  });

  $effect(() => {
    const w = wire;
    scene?.setWireframe(w);
  });
  $effect(() => {
    const h = readoutH;
    if (shown) scene?.setInset(h);
  });

  onDestroy(() => {
    if (phase === 'exporting') void app.cancel();
    token++;
    cancelled = true;
    scene?.dispose();
    scene = null;
  });

  function ensureScene(): ModelScene | null {
    if (scene) return scene;
    if (!host) return null;
    try {
      scene = new ModelScene(host, theme());
      scene.setWireframe(wire);
      scene.onContextLost = () => {
        error = { message: 'The 3D view lost its graphics context (a driver reset, or the GPU was needed elsewhere). Load the model again.' };
        shown = null;
        phase = 'idle';
        scene?.dispose();
        scene = null;
      };
      return scene;
    } catch (e) {
      noGl = webglProblem(e);
      return null;
    }
  }

  /** Export (unless cached), fetch, parse and show the slider's LOD. */
  async function load(force: boolean) {
    const my = ++token;
    cancelled = false;
    const g = guid;
    const asked = lod;
    error = null;
    memAsk = null;
    fetched = null;
    const cachedStep = info?.lods[asked]?.cached ?? false;
    phase = cachedStep ? 'loading' : 'exporting';
    try {
      const v = await assetApi.modelView({ target: $state.snapshot(target), what: g, name, lod: asked, force });
      if (my !== token) return;
      phase = 'loading';
      const data = await fetchGlb(v, $state.snapshot(target), assetApi.modelChunk, (p) => my === token && (fetched = p), () => cancelled || my !== token);
      if (my !== token) return;
      phase = 'building';
      // Let the "Building" state paint before the parse takes the main thread.
      await new Promise((r) => requestAnimationFrame(() => r(null)));
      const root = await parseGlb(data);
      if (my !== token) return;
      const sc = ensureScene();
      if (!sc) {
        phase = 'idle';
        return;
      }
      sc.setInset(readoutH);
      const stats = sc.show(root);
      shown = { lod: v.lod, view: v, stats };
      phase = 'ready';
      void loadInfo(g);
    } catch (e) {
      if (my !== token) return;
      phase = shown ? 'ready' : 'idle';
      if (e instanceof FetchCancelled) return;
      const err = toContractError(e);
      if (err.code === 'needs-decision') memAsk = err.message;
      else if (err.code === 'cancelled') toast('info', 'Model load stopped', 'Nothing was kept.');
      else error = err;
      void loadInfo(g);
    }
  }

  function stop() {
    if (phase === 'exporting') void app.cancel();
    else cancelled = true;
  }

  async function exportGlb() {
    if (!shown) return;
    exporting = true;
    try {
      const r = await assetApi.exportModel({ target: $state.snapshot(target), what: guid, name, preset: 'print-shell', lod: shown.lod, stl: false, force: false });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${name}${r.cached ? ' from the cache' : ''}`, [mbText(r.bytes), r.path].filter(Boolean).join(' · '));
    } catch (e) {
      toast('error', 'Model export failed', toContractError(e).message);
    } finally {
      exporting = false;
    }
  }

  // Moving onto a cached step shows it at once (cheap); others wait for "Load".
  let pickTimer: ReturnType<typeof setTimeout> | null = null;
  function onSlide(v: number) {
    slider = v;
    if (pickTimer) clearTimeout(pickTimer);
    const s = info?.lods[sliderToLod(v)];
    if (s?.cached && !busy && shown?.lod !== s.uses) pickTimer = setTimeout(() => void load(false), 150);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-y-auto" data-model-viewer data-phase={phase}>
  <div class="grid gap-1 border-b px-3 py-2">
    <div class="grid gap-0.5">
      <div class="flex items-center gap-2">
        <label for="model-detail" class="w-11 shrink-0 text-[12.5px] font-medium">Detail</label>
        <input
          id="model-detail"
          type="range"
          class="accent-primary h-2 min-w-0 flex-1 cursor-pointer"
          min="0"
          max={MAX_LOD}
          step="1"
          value={slider}
          disabled={!info || busy}
          aria-valuetext={lodLabel(lod)}
          aria-describedby="model-step-note"
          oninput={(e) => onSlide(Number((e.currentTarget as HTMLInputElement).value))}
        />
      </div>
      {#if info}
        <!-- One column per slider step, left (LOD 4) to right (LOD 0): its memory or its cache. -->
        <ol class="text-muted-foreground mono ml-[3.25rem] grid grid-cols-5 text-center text-[10px] leading-tight whitespace-nowrap" aria-label="Memory per level of detail">
          {#each [...info.lods].reverse() as s (s.lod)}
            <li class={cn(s.lod === lod && 'text-foreground font-semibold', !s.cached && s.fit === 'over-limit' && 'text-destructive')} data-step={s.lod} data-cached={s.cached}>
              <span class="block">LOD {s.lod}</span>
              <span class="block">{s.cached ? (s.bytes !== null ? mbText(s.bytes) : 'cached') : `~${gbText(s.need)}`}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
    <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1">
      {#if step && info}
        <p id="model-step-note" aria-live="polite" class={cn('min-w-48 flex-1 text-[11.5px]', stepKind === 'refused' || stepKind === 'too-big' ? 'text-destructive' : stepKind === 'ask' ? 'text-amber-700 dark:text-amber-300' : 'text-muted-foreground')} data-step-note={stepKind}>
          <span class="text-foreground mono">{lodLabel(lod)}</span> · {stepNote(step, info)}
        </p>
      {:else if infoError}
        <div class="flex-1"><ErrorState title="Could not read the model's levels of detail" error={infoError} onretry={() => void loadInfo(guid)} /></div>
      {:else}
        <span class="flex-1"></span>
      {/if}
      {#if busy}
        {#if phase === 'exporting'}
          <JobProgress job={app.job} compact />
        {:else}
          <Button variant="outline" size="xs" onclick={stop} disabled={phase === 'building'}><Square aria-hidden="true" /> Stop</Button>
        {/if}
      {:else}
        <Button size="xs" disabled={!info || stepKind === 'refused' || stepKind === 'too-big' || (shown?.lod === step?.uses && !error)} onclick={() => void load(false)} data-load>
          <Box aria-hidden="true" /> {shown?.lod === step?.uses && !error ? `LOD ${shown?.lod} shown` : stepKind === 'cached' ? `Show LOD ${lod}` : `Load LOD ${lod}`}
        </Button>
      {/if}
    </div>
  </div>

  <div class="relative min-h-[180px] flex-1 overflow-hidden bg-[radial-gradient(ellipse_at_center,var(--color-muted)_0%,transparent_75%)]">
    <div bind:this={host} class="absolute inset-0" aria-label={shown ? `3D view of ${name}, LOD ${shown.lod}: drag to orbit, scroll to zoom, right-drag to pan` : undefined} role={shown ? 'img' : undefined}></div>
    {#if shown}
      <div class="absolute top-2 right-2 flex flex-wrap justify-end gap-1">
        <Button variant={wire ? 'default' : 'outline'} size="xs" role="switch" aria-checked={wire} aria-label="Wireframe" title="Wireframe" onclick={() => (wire = !wire)}><Grid3x3 aria-hidden="true" /><span class="max-[1100px]:sr-only">Wireframe</span></Button>
        <Button variant="outline" size="xs" aria-label="Reset view" title="Reset view" onclick={() => scene?.resetView()}><RotateCcw aria-hidden="true" /><span class="max-[1100px]:sr-only">Reset</span></Button>
        <Button variant="outline" size="xs" disabled={exporting} title="Save this LOD's GLB (copied from the cache)" onclick={() => void exportGlb()}><Download aria-hidden="true" /> Export .glb</Button>
      </div>
    {/if}
    <div bind:clientHeight={readoutH} class="from-card/95 via-card/70 pointer-events-none absolute inset-x-0 bottom-0 grid gap-0.5 bg-gradient-to-t to-transparent px-3 pt-4 pb-1.5 text-[11.5px]">
      {#if shown}
        <p class="flex flex-wrap gap-x-3 gap-y-0.5">
          <span title="Bounding box of the triangles: width × height × length (glTF is Y-up)" class="pointer-events-auto" data-model-size><span class="text-muted-foreground">Size</span> <span class="mono">{sizeText(shown.stats.size)}</span></span>
          <span class="mono" data-triangles>{trianglesText(shown.stats.triangles)}</span>
          <span class="text-muted-foreground mono">{shown.stats.meshes} meshes · {mbText(shown.view.bytes)}{shown.view.cached ? ' · from the cache' : shown.view.ms !== null ? ` · exported in ${shown.view.ms} ms${shown.view.peak_rss ? `, peak ${gbText(shown.view.peak_rss)}` : ''}` : ''}</span>
          {#if shown.lod !== shown.view.asked && shown.view.available.length}<span class="text-muted-foreground">LOD {shown.view.asked} asked; the mesh has LOD 0-{Math.max(...shown.view.available)}</span>{/if}
        </p>
      {/if}
      <p class="text-muted-foreground text-[11px]">{PERSONAL_USE}{shown ? ' Drag to orbit, scroll to zoom, right-drag to pan.' : ''}</p>
    </div>
    {#if !shown || busy || error || noGl}
      <div class="pointer-events-none absolute inset-0 grid place-items-center p-4 pb-12">
        <div class="bg-card/90 pointer-events-auto grid max-w-md gap-2 rounded-lg border p-3 text-[12.5px] shadow-sm">
          {#if noGl}
            <ErrorState title="No 3D view" error={{ message: noGl }} />
          {:else if error}
            <ErrorState title="Could not show the model" {error} onretry={() => void load(false)} />
          {:else if phase === 'exporting'}
            <p class="flex items-center gap-2"><LoaderCircle class="size-4 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Exporting LOD {lod} in its own process (about a second; its memory goes back when it ends)…</p>
          {:else if phase === 'loading'}
            <p class="flex items-center gap-2"><LoaderCircle class="size-4 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Reading the model from the cache…</p>
            {#if fetched && fetched.total > 0}
              <Progress value={fetched.done} max={fetched.total} class="h-1" aria-label="Model read" />
              <p class="text-muted-foreground mono text-[11px]">{mbText(fetched.done)} of {mbText(fetched.total)}</p>
            {/if}
          {:else if phase === 'building'}
            <p class="flex items-center gap-2"><LoaderCircle class="size-4 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Building the 3D view…</p>
          {:else if info}
            <p><span class="font-medium">Print shell</span> of {name}: geometry only, no materials, interior or lights.</p>
            <p class="text-muted-foreground text-[12px]">Pick the detail, then {stepKind === 'cached' ? 'Show' : 'Load'}. Each level is exported once, in its own process, into the asset cache; after that it shows without one.</p>
          {:else if !infoError}
            <p class="text-muted-foreground flex items-center gap-2"><LoaderCircle class="size-4 shrink-0 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Reading the levels of detail…</p>
          {/if}
        </div>
      </div>
    {/if}
  </div>

</div>

<Dialog.Root open={memAsk !== null} onOpenChange={(o) => !o && (memAsk = null)}>
  <Dialog.Content class="max-w-md">
    <Dialog.Header>
      <Dialog.Title>Not much memory free</Dialog.Title>
      <Dialog.Description>{memAsk}</Dialog.Description>
    </Dialog.Header>
    <p class="text-muted-foreground text-[12.5px]">The export runs in its own process under the memory limit; if memory runs out, that process stops and the app keeps running.</p>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (memAsk = null)}>Cancel</Button>
      <Button onclick={() => void load(true)}>Load anyway</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
