<script lang="ts">
  /**
   * Settings > Advanced > Memory limit (addendum 7.18): the cap of each Developer mode and MCP
   * worker, 1 GB .. RAM - 2 GB in 0.5 GB steps, saved in config.toml ([dev] memory_limit_gb).
   * Lists which measured jobs fit under it. Normal setup and apply are never limited.
   */
  import { onMount } from 'svelte';
  import Check from '@lucide/svelte/icons/check';
  import X from '@lucide/svelte/icons/x';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import { Button } from '@/components/ui/button';
  import { toContractError } from '@/lib/contract/client';
  import { enforcementText, fits as fitsUnder, fitsCount, gbText, limitText, machineText } from '@/lib/memory';
  import { loadMemory, memory, saveMemoryLimit } from '@/lib/stores/memory.svelte';
  import { toast } from '@/lib/stores/toast.svelte';

  const uid = $props.id();
  let draft = $state<number | null>(null);
  let busy = $state(false);
  onMount(() => void loadMemory());

  const s = $derived(memory.status);
  const value = $derived(draft ?? s?.limit_gb ?? 0);
  const fits = $derived(s ? s.estimates.map((e) => ({ ...e, fits: fitsUnder(e.bytes, value, s.stop_percent) })) : []);
  const counts = $derived(fitsCount({ estimates: fits }));

  async function save(gb: number | null) {
    busy = true;
    try {
      const r = await saveMemoryLimit(gb);
      toast('success', gb === null ? `Memory limit back to ${limitText(r.limit_gb)}` : `Memory limit ${limitText(r.limit_gb)}`, 'Applies to the next worker that starts.');
    } catch (e) {
      toast('error', 'Memory limit not saved', toContractError(e).message);
    } finally {
      draft = null;
      busy = false;
    }
  }
</script>

<div class="grid gap-1.5 border-t pt-3" data-memory-limit>
  <div class="flex flex-wrap items-baseline gap-2">
    <label for="{uid}-slider" class="text-[13px] font-medium">Memory limit</label>
    {#if s}<span class="mono text-[13px]" aria-live="polite">{limitText(value)}</span>
      <span class="text-muted-foreground text-[12px]">{s.is_default && draft === null ? 'default · ' : ''}{machineText(s)}</span>{/if}
  </div>
  <p class="text-muted-foreground text-[12px]">For each Developer mode worker (Explorer, Files, sounds, model exports) and each MCP extraction. A job estimated above it is refused before it starts; one that grows past it is stopped with a message. Setting up and applying are never limited.</p>
  {#if s}
    <div class="flex items-center gap-3">
      <span class="text-muted-foreground mono text-[11px]">{limitText(s.min_gb)}</span>
      <input
        id="{uid}-slider"
        type="range"
        class="accent-primary h-2 min-w-0 flex-1 cursor-pointer"
        min={s.min_gb}
        max={s.max_gb}
        step={s.step_gb}
        value={value}
        disabled={busy}
        aria-valuetext={limitText(value)}
        oninput={(e) => (draft = Number((e.currentTarget as HTMLInputElement).value))}
        onchange={(e) => void save(Number((e.currentTarget as HTMLInputElement).value))}
      />
      <span class="text-muted-foreground mono text-[11px]">{limitText(s.max_gb)}</span>
      <Button variant="ghost" size="xs" disabled={busy || (s.is_default && draft === null)} onclick={() => void save(null)} title="Back to {limitText(s.default_gb)}"><RotateCcw aria-hidden="true" /> Default</Button>
    </div>
    <div class="grid gap-0.5" role="list" aria-label="What fits under {limitText(value)}: {counts[0]} of {counts[1]}">
      {#each fits as e (e.id)}
        <div role="listitem" class="flex items-center gap-2 text-[12px]" data-fits={e.fits}>
          {#if e.fits}<Check class="text-success size-3.5 shrink-0" aria-label="fits" />{:else}<X class="text-destructive size-3.5 shrink-0" aria-label="over the limit" />{/if}
          <span class={e.fits ? '' : 'text-muted-foreground'}>{e.label}</span>
          <span class="mono text-muted-foreground ml-auto">{gbText(e.bytes)}</span>
        </div>
      {/each}
    </div>
    {#if s.note}<p class="text-warning text-[12px]" role="status">{s.note}</p>{/if}
    <p class="text-muted-foreground text-[11.5px]">{enforcementText(s.enforcement)} Command line: <span class="mono whitespace-nowrap">--memory-limit GB</span> or <span class="mono whitespace-nowrap">STARORG_MEMORY_LIMIT_GB</span> (0 = none).</p>
  {/if}
</div>
