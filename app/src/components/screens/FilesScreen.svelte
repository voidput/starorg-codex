<script lang="ts">
  /**
   * Files (addendum 7.16): the channel's Data.p4k as a folder tree, read only. Folder listings
   * come 500 at a time; search matches every word of the path. A file shows a preview: textures
   * as PNG (a mip at most 1024 px a side), CryXML and XML as text, WEM sounds with a play
   * button. Extract copies the selected file or the current folder to a folder you pick
   * (optionally textures as PNG and CryXML as XML); never into the game folder. The asset
   * worker starts when this screen first lists a folder and stops after 5 idle minutes.
   */
  import { untrack } from 'svelte';
  import Folder from '@lucide/svelte/icons/folder';
  import FileIcon from '@lucide/svelte/icons/file';
  import Search from '@lucide/svelte/icons/search';
  import X from '@lucide/svelte/icons/x';
  import Download from '@lucide/svelte/icons/download';
  import ArrowUp from '@lucide/svelte/icons/arrow-up';
  import Play from '@lucide/svelte/icons/play';
  import Square from '@lucide/svelte/icons/square';
  import Power from '@lucide/svelte/icons/power';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import JobProgress from '@/components/common/JobProgress.svelte';
  import { assetApi, bytesText, PERSONAL_USE, soundId, type AssetPreview, type AssetStatus, type FileEntry } from '@/lib/contract/assets';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import { app } from '@/lib/stores/app.svelte';
  import { player } from '@/lib/stores/assets.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { int } from '@/lib/format';
  import { cn } from '@/lib/utils';
  import { reminderText } from '@/lib/memory';
  import { ensureMemory, memory } from '@/lib/stores/memory.svelte';
  $effect(() => ensureMemory());

  const PAGE = 500;
  const target = $derived(app.target);
  const tkey = $derived(app.targetKey);

  let dir = $state('');
  let rows = $state.raw<FileEntry[]>([]);
  let total = $state(0);
  let listing = $state(false);
  let error = $state<ContractError | null>(null);
  let query = $state('');
  let searched = $state<string | null>(null);
  let selected = $state<FileEntry | null>(null);
  let preview = $state<AssetPreview | null>(null);
  let previewing = $state(false);
  let previewError = $state<ContractError | null>(null);
  let convert = $state(true);
  let extracting = $state(false);
  let status = $state<AssetStatus | null>(null);

  async function refreshStatus() {
    if (!target) return;
    try {
      status = await assetApi.status($state.snapshot(target));
    } catch {
      status = null;
    }
  }

  async function load(d: string, offset = 0) {
    if (!target) return;
    listing = true;
    try {
      const p = await assetApi.list({ target: $state.snapshot(target), dir: d, offset, limit: PAGE });
      dir = p.dir;
      rows = offset ? [...rows, ...p.entries] : p.entries;
      total = p.total;
      searched = null;
      error = null;
      void refreshStatus();
    } catch (e) {
      error = toContractError(e);
    } finally {
      listing = false;
    }
  }
  $effect(() => {
    void tkey;
    untrack(() => {
      selected = null;
      preview = null;
      void load('');
    });
  });

  async function search() {
    const q = query.trim();
    if (!q) return void load(dir);
    if (!target) return;
    listing = true;
    try {
      const r = await assetApi.search({ target: $state.snapshot(target), query: q, limit: PAGE });
      rows = r.hits;
      total = r.total;
      searched = q;
      error = null;
    } catch (e) {
      error = toContractError(e);
    } finally {
      listing = false;
    }
  }

  async function select(f: FileEntry) {
    if (f.dir) {
      query = '';
      selected = null;
      preview = null;
      return void load(f.path);
    }
    selected = f;
    preview = null;
    previewError = null;
    if (!target) return;
    previewing = true;
    try {
      preview = await assetApi.preview({ target: $state.snapshot(target), path: f.path });
    } catch (e) {
      previewError = toContractError(e);
    } finally {
      previewing = false;
    }
  }

  const crumbs = $derived(dir ? dir.split('\\').map((name, i, all) => ({ name, path: all.slice(0, i + 1).join('\\') })) : []);
  const parent = $derived(dir.includes('\\') ? dir.slice(0, dir.lastIndexOf('\\')) : dir ? '' : null);

  async function extract(paths: string[], what: string) {
    if (!target || paths.length === 0) return;
    extracting = true;
    try {
      const r = await assetApi.extract({ target: $state.snapshot(target), paths, convert });
      if (r.cancelled) toast('info', 'Extract cancelled', 'Nothing was written.');
      else toast('success', `Extracted ${int(r.files)} files from ${what}`, `${bytesText(r.bytes)} · ${r.path}${r.note ? ` · ${r.note}` : ''}${r.failed.length ? ` · ${r.failed.length} failed` : ''}`);
    } catch (e) {
      const err = toContractError(e);
      if (err.code === 'cancelled') toast('info', 'Extract stopped', 'Files already written stay.');
      else toast('error', 'Extract failed', err.message);
    } finally {
      extracting = false;
    }
  }

  async function close() {
    if (!target) return;
    player.stop();
    status = await assetApi.close($state.snapshot(target));
    toast('info', 'Asset browser closed', 'Its memory is returned; it opens again on the next request.');
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Backspace' && parent !== null && !(e.target as HTMLElement).closest('input')) {
      e.preventDefault();
      void load(parent);
    }
  }
  const wemSound = (p: string) => ({ media_id: 0, source: 'external' as const, bank: '', media_path: p });
</script>

<div class="flex h-full min-h-[480px] flex-col gap-3" role="presentation" onkeydown={onKey}>
  <div class="flex flex-wrap items-center gap-2">
    <form
      class="relative min-w-56 flex-1"
      role="search"
      onsubmit={(e) => {
        e.preventDefault();
        void search();
      }}
    >
      <Search class="text-muted-foreground absolute top-1/2 left-2 size-3.5 -translate-y-1/2" aria-hidden="true" />
      <Input class="h-8 pl-7" placeholder="Search paths (every word): javelin diff dds" aria-label="Search the game files" bind:value={query} />
      {#if query}
        <button type="button" class="text-muted-foreground absolute top-1/2 right-2 -translate-y-1/2" aria-label="Clear search" onclick={() => ((query = ''), void load(dir))}><X class="size-3.5" aria-hidden="true" /></button>
      {/if}
    </form>
    <SwitchRow id="files-convert" size="sm" label="Convert" hint="Textures as PNG (mips merged), CryXML as XML" checked={convert} onchange={(v) => (convert = v)} />
    <Button variant="outline" size="sm" disabled={extracting || !dir || searched !== null} onclick={() => void extract([dir], dir)}><Download aria-hidden="true" /> Extract folder…</Button>
    {#if status?.state === 'ready'}
      <span class="text-muted-foreground text-[11.5px]">{int(status.entries)} files · {status.memory ? bytesText(status.memory) : ''}</span>
      <Button variant="ghost" size="icon-sm" title="Close the asset browser (frees its memory)" aria-label="Close the asset browser" onclick={() => void close()}><Power aria-hidden="true" /></Button>
    {/if}
  </div>
  <p class="text-muted-foreground -mt-1 text-[11.5px]">Read only: Data.p4k is never changed, and nothing is written into the game folder. {reminderText(memory.status)} {PERSONAL_USE}</p>
  {#if extracting}<JobProgress job={app.job} />{/if}

  <div class="flex min-h-0 flex-1 gap-3">
    <section class="bg-card flex w-[46%] min-w-72 flex-col overflow-hidden rounded-lg border" aria-label="Files">
      <nav class="flex min-h-9 flex-wrap items-center gap-1 border-b px-2 py-1 text-[12px]" aria-label="Folder">
        <Button variant="ghost" size="icon-xs" disabled={parent === null || searched !== null} aria-label="Up one folder" title="Up (Backspace)" onclick={() => parent !== null && void load(parent)}><ArrowUp aria-hidden="true" /></Button>
        {#if searched !== null}
          <span>{int(total)} {total === 1 ? 'match' : 'matches'} for <span class="mono">{searched}</span>{total > rows.length ? ` (first ${rows.length})` : ''}</span>
        {:else}
          <button class="hover:underline" onclick={() => void load('')}>Data.p4k</button>
          {#each crumbs as c (c.path)}
            <span class="text-muted-foreground">/</span>
            <button class="mono hover:underline" onclick={() => void load(c.path)}>{c.name}</button>
          {/each}
        {/if}
        {#if listing}<LoaderCircle class="text-muted-foreground ml-auto size-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
      </nav>
      {#if error}
        <div class="p-3"><ErrorState title="Could not read the game files" {error} onretry={() => void load(dir)} /></div>
      {:else}
        <ul class="min-h-0 flex-1 overflow-y-auto text-[12.5px]" aria-label="Folder contents">
          {#each rows as f (f.path)}
            <li>
              <button
                class={cn('hover:bg-accent/60 flex w-full items-center gap-2 px-3 py-1 text-left', selected?.path === f.path && 'bg-accent/70')}
                title={f.path}
                onclick={() => void select(f)}
              >
                {#if f.dir}<Folder class="text-primary size-3.5 shrink-0" aria-hidden="true" />{:else}<FileIcon class="text-muted-foreground size-3.5 shrink-0" aria-hidden="true" />{/if}
                <span class="mono min-w-0 flex-1 truncate">{searched !== null ? f.path : f.name}</span>
                {#if !f.dir}<span class="text-muted-foreground mono shrink-0 text-[11px]">{bytesText(f.size)}</span>{/if}
              </button>
            </li>
          {:else}
            <li class="text-muted-foreground p-3">{listing ? 'Reading…' : searched !== null ? 'No file matches.' : 'Empty folder.'}</li>
          {/each}
          {#if searched === null && rows.length < total}
            <li class="p-2"><Button variant="outline" size="xs" onclick={() => void load(dir, rows.length)}>Show {Math.min(PAGE, total - rows.length)} more ({int(total - rows.length)} left)</Button></li>
          {/if}
        </ul>
      {/if}
    </section>

    <section class="bg-card flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border" aria-label="Preview">
      {#if selected}
        <div class="grid gap-1 border-b px-3 py-2">
          <div class="flex flex-wrap items-center gap-2">
            <h2 class="mono min-w-0 flex-1 truncate text-[13px] font-semibold" title={selected.path}>{selected.name}</h2>
            <Button variant="outline" size="xs" disabled={extracting} onclick={() => selected && void extract([selected.path], selected.name)}><Download aria-hidden="true" /> Extract…</Button>
          </div>
          <p class="text-muted-foreground mono truncate text-[11.5px]" title={selected.path}>{selected.path}</p>
          <p class="text-muted-foreground text-[11.5px]">{bytesText(selected.size)} · {bytesText(selected.packed)} in the archive{preview?.width ? ` · ${preview.width} x ${preview.height}` : ''}{preview?.cached ? ' · from the cache' : ''}</p>
        </div>
        <div class="min-h-0 flex-1 overflow-auto p-3">
          {#if previewing}
            <p class="text-muted-foreground flex items-center gap-2 text-[12px]"><LoaderCircle class="size-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" /> Decoding…</p>
          {:else if previewError}
            <ErrorState title="No preview" error={previewError} />
          {:else if preview?.kind === 'image' && preview.image}
            <img src={preview.image} alt={`Texture ${selected.name}`} class="h-auto w-full max-w-[1024px] rounded border bg-[repeating-conic-gradient(#8884_0_25%,transparent_0_50%)] bg-[length:16px_16px] [image-rendering:pixelated]" />
          {:else if preview?.kind === 'text'}
            <pre class="mono bg-background/60 rounded-md border p-2 text-[11.5px] leading-snug whitespace-pre-wrap">{preview.text}</pre>
            {#if preview.truncated}<p class="text-muted-foreground mt-1 text-[11.5px]">Cut at 1 MiB; extract the file for all of it.</p>{/if}
          {:else if preview?.kind === 'audio'}
            {@const s = wemSound(selected.path)}
            <Button variant={player.playing === soundId(s) ? 'default' : 'outline'} size="sm" onclick={() => target && selected && void player.play($state.snapshot(target), s, selected.name)}>
              {#if player.loading === soundId(s)}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else if player.playing === soundId(s)}<Square aria-hidden="true" />{:else}<Play aria-hidden="true" />{/if}
              {player.playing === soundId(s) ? 'Stop' : 'Play'}
            </Button>
          {/if}
          {#if preview?.note}<p class="text-muted-foreground mt-2 text-[12px]">{preview.note}</p>{/if}
        </div>
      {:else}
        <p class="text-muted-foreground p-4 text-[12.5px]">Pick a file to preview it. Folders open with a click; Backspace goes up.</p>
      {/if}
    </section>
  </div>
</div>
