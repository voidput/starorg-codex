<script lang="ts">
  /** A card that is a native radio input (arrow keys and form semantics for free). */
  import type { Snippet } from 'svelte';
  import { cn } from '@/lib/utils';

  let {
    name,
    value,
    group = $bindable(),
    disabled = false,
    class: className = '',
    children,
    onselect,
  }: { name: string; value: string; group: string | null; disabled?: boolean; class?: string; children: Snippet; onselect?: (v: string) => void } = $props();
  const on = $derived(group === value);
</script>

<label
  class={cn(
    'bg-card/80 has-focus-visible:ring-ring/70 relative flex cursor-pointer flex-col gap-1 rounded-lg border p-3 transition-colors has-focus-visible:ring-2',
    on ? 'border-primary ring-primary/40 bg-primary/5 ring-1' : 'hover:bg-accent/50',
    disabled && 'cursor-not-allowed opacity-60',
    className,
  )}
>
  <input
    type="radio"
    class="sr-only"
    {name}
    {value}
    {disabled}
    checked={on}
    onchange={() => {
      group = value;
      onselect?.(value);
    }}
  />
  <span class={cn('absolute top-3 right-3 size-4 rounded-full border-2', on ? 'border-primary bg-primary shadow-[inset_0_0_0_2px_var(--card)]' : 'border-input')} aria-hidden="true"></span>
  {@render children()}
</label>
