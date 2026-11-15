<script lang="ts">
  /**
   * A raw value with what it reads as (addendum 7.12): the readable label first, the raw id
   * after it (muted, mono, full text in the tooltip), an "enhanced" mark when our text differs
   * from stock, and an optional copy button for the raw id. Without a label, or with "Show
   * IDs" on, the raw value alone (ids nothing reads are marked). Text only, never HTML.
   */
  import Copy from '@lucide/svelte/icons/copy';
  import type { Readable } from '@/lib/contract/dev';
  import { shown } from '@/lib/readable';
  import { dev } from '@/lib/stores/dev.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  let {
    raw,
    label = null,
    copy = false,
    stack = false,
    class: className = '',
  }: {
    raw: string;
    label?: Readable | null;
    /** A copy button for the raw value (not inside grid rows). */
    copy?: boolean;
    /** Raw id on its own line under the label. */
    stack?: boolean;
    class?: string;
  } = $props();

  const s = $derived(shown(raw, label, dev.showIds));
  async function doCopy(e: MouseEvent) {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(raw);
      toast('success', 'Copied the id');
    } catch {
      toast('error', 'The clipboard is not available here');
    }
  }
</script>

<span class={cn('inline-flex min-w-0 max-w-full gap-x-1.5', stack ? 'flex-col items-start' : 'items-baseline', className)} title={s.title}>
  <span class={cn('min-w-0 truncate', s.raw && 'mono', s.unresolved && 'text-muted-foreground')}>{s.primary}</span>
  {#if s.enhanced}<span class="text-primary shrink-0 text-[10px] font-semibold tracking-wide uppercase" aria-label="enhanced text differs">enh</span>{/if}
  {#if s.secondary}<span class={cn('text-muted-foreground mono min-w-0 truncate text-[10.5px]', !stack && 'shrink-[2]')}>{s.secondary}</span>{/if}
  {#if s.unresolved}<span class="text-muted-foreground shrink-0 text-[10px] italic">unresolved</span>{/if}
  {#if copy}
    <button type="button" class="text-muted-foreground hover:text-foreground focus-visible:ring-ring/60 shrink-0 self-center rounded outline-none focus-visible:ring-2" aria-label="Copy id {raw}" title="Copy the raw id" onclick={doCopy}><Copy class="size-3" aria-hidden="true" /></button>
  {/if}
</span>
