<script lang="ts">
  /** A state as icon + text in a tinted outline (spec 9.6: never colour alone). */
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import type { Component } from 'svelte';
  import type { StatusTone } from '@/lib/format';
  import { cn } from '@/lib/utils';

  let {
    tone,
    label,
    hint = undefined,
    icon = undefined,
    class: className = '',
  }: { tone: StatusTone; label: string; hint?: string; icon?: Component<{ class?: string }> | null; class?: string } = $props();

  const auto = { success: CircleCheck, warning: TriangleAlert, info: Info, muted: CircleDashed, destructive: CircleX } as const;
  const Icon = $derived(icon === undefined ? auto[tone] : icon);
  const cls = {
    success: 'border-success/40 text-success',
    warning: 'border-warning/40 text-warning',
    info: 'border-info/40 text-info',
    muted: 'border-border text-muted-foreground',
    destructive: 'border-destructive/40 text-destructive',
  } as const;
</script>

<span class={cn('inline-flex h-5 shrink-0 items-center gap-1 rounded-full border px-2 text-[11.5px] font-medium whitespace-nowrap', cls[tone], className)} title={hint}>
  {#if Icon}<Icon class="size-3" aria-hidden="true" />{/if}
  {label}
</span>
