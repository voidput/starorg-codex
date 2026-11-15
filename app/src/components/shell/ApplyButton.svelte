<script lang="ts">
  /** The primary Apply action (spec 9.1: one of the three chamfer + glow places). */
  import Rocket from '@lucide/svelte/icons/rocket';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import { cn } from '@/lib/utils';

  let {
    label,
    busy = false,
    disabled = false,
    onclick,
    class: className = '',
  }: { label: string; busy?: boolean; disabled?: boolean; onclick: () => void; class?: string } = $props();
</script>

<button
  type="button"
  class={cn(
    'bg-primary text-primary-foreground chamfer-sm shadow-glow-sm hover:shadow-glow-md focus-visible:ring-ring inline-flex h-9 items-center gap-2 px-4 text-[13px] font-semibold outline-none transition-shadow focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50',
    className,
  )}
  disabled={disabled || busy}
  aria-busy={busy}
  {onclick}
>
  {#if busy}<LoaderCircle class="size-4 animate-spin" aria-hidden="true" />{:else}<Rocket class="size-4" aria-hidden="true" />{/if}
  {label}
</button>
