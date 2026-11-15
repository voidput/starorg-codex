<script lang="ts">
  /**
   * One sound event of the Explorer's Sounds search (addendum 7.17): the ATL trigger, its Wwise
   * event and bank, and the sounds it plays, each playable; "Export .ogg…" writes them through
   * a native folder dialog (the asset cache answers without decoding again).
   */
  import Play from '@lucide/svelte/icons/play';
  import Square from '@lucide/svelte/icons/square';
  import Download from '@lucide/svelte/icons/download';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import Copy from '@lucide/svelte/icons/copy';
  import { Button } from '@/components/ui/button';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import { assetApi, bytesText, PERSONAL_USE, soundId, type SoundInfo, type TriggerInfo } from '@/lib/contract/assets';
  import { toContractError, type ContractError } from '@/lib/contract/client';
  import type { ChannelRef } from '@/lib/contract/types';
  import { player } from '@/lib/stores/assets.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  let { target, name }: { target: ChannelRef; name: string } = $props();
  let info = $state<TriggerInfo | null>(null);
  let error = $state<ContractError | null>(null);
  let loading = $state(false);
  let exporting = $state(false);
  let gen = 0;
  async function load(n: string) {
    const g = ++gen;
    loading = true;
    try {
      const r = await assetApi.trigger({ target: $state.snapshot(target), name: n });
      if (g === gen) {
        info = r;
        error = null;
      }
    } catch (e) {
      if (g === gen) error = toContractError(e);
    } finally {
      if (g === gen) loading = false;
    }
  }
  $effect(() => {
    void load(name);
  });
  const ref = (s: SoundInfo) => ({ media_id: s.media_id, source: s.source, bank: s.bank, media_path: s.media_path });
  const playable = $derived(info?.sounds.filter((s) => s.playable) ?? []);
  async function exportAll(wem: boolean) {
    if (!info) return;
    exporting = true;
    try {
      const r = await assetApi.exportSounds({ target: $state.snapshot(target), what: info.name, sounds: playable.map((s) => ({ trigger: info!.name, sound: ref(s) })), wem });
      if (r.cancelled) toast('info', 'Export cancelled', 'Nothing was written.');
      else toast('success', `Exported ${r.files} sound files${r.cached ? ' from the cache' : ''}`, `${bytesText(r.bytes)} · ${r.path}`);
    } catch (e) {
      toast('error', 'Sound export failed', toContractError(e).message);
    } finally {
      exporting = false;
    }
  }
  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast('success', 'Copied the trigger name');
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }
</script>

<div class="grid min-w-0 content-start gap-2 overflow-auto p-3 text-[12.5px]" data-trigger>
  <div class="flex min-w-0 items-center gap-2">
    <h2 class="mono min-w-0 flex-1 truncate text-[13.5px] font-semibold" title={name}>{name}</h2>
    {#if loading}<LoaderCircle class="text-muted-foreground size-4 animate-spin motion-reduce:animate-none" aria-hidden="true" />{/if}
    <Button variant="ghost" size="icon-xs" aria-label="Copy trigger name" title="Copy trigger name" onclick={() => void copy(name)}><Copy aria-hidden="true" /></Button>
  </div>
  {#if error}
    <ErrorState title="Could not read the sound event" {error} onretry={() => void load(name)} />
  {:else if info}
    <dl class="text-muted-foreground grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-0.5 text-[11.5px]">
      <dt>Event</dt><dd class="mono text-foreground truncate">{info.event ?? '–'}</dd>
      <dt>Bank</dt><dd class="mono truncate">{info.bank ?? '–'}</dd>
      <dt>Duration</dt><dd>{info.duration_type ?? '–'}</dd>
    </dl>
    {#if info.sounds.length === 0}
      <p class="text-muted-foreground">No media found for this event.</p>
    {:else}
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-muted-foreground">{playable.length} of {info.sounds.length} sounds playable</span>
        <span class="flex-1"></span>
        <Button variant="outline" size="xs" disabled={exporting || playable.length === 0} onclick={() => void exportAll(false)}><Download aria-hidden="true" /> Export .ogg…</Button>
        <Button variant="ghost" size="xs" disabled={exporting || playable.length === 0} onclick={() => void exportAll(true)}>+ .wem</Button>
      </div>
      <p class="text-muted-foreground text-[11px]">{PERSONAL_USE}</p>
      <div class="flex flex-wrap gap-1" aria-label="Sounds">
        {#each info.sounds as s (soundId(s))}
          {@const k = soundId(s)}
          <Button variant={player.playing === k ? 'default' : 'outline'} size="xs" disabled={!s.playable} title={s.playable ? `${s.source} · ${s.detail}` : s.detail} aria-label={`${player.playing === k ? 'Stop' : 'Play'} ${name} ${s.label}`} onclick={() => void player.play($state.snapshot(target), ref(s), `${name} ${s.label}`)}>
            {#if player.loading === k}<LoaderCircle class="animate-spin motion-reduce:animate-none" aria-hidden="true" />{:else if player.playing === k}<Square aria-hidden="true" />{:else}<Play aria-hidden="true" />{/if}
            <span class="mono">{s.label}</span>
          </Button>
        {/each}
      </div>
    {/if}
  {/if}
</div>
