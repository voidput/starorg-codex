<script lang="ts">
  /** Toast stack, bottom right. Info/success in one polite region; errors are role=alert (spec 9.6). */
  import X from '@lucide/svelte/icons/x';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import Info from '@lucide/svelte/icons/info';
  import { Button } from '@/components/ui/button';
  import { toasts, dismiss, type Toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  const icon = { info: Info, success: CircleCheck, warning: TriangleAlert, error: CircleAlert } as const;
  const tone = { info: 'text-primary', success: 'text-success', warning: 'text-warning', error: 'text-destructive' } as const;
  const errors = $derived(toasts.list.filter((t) => t.tone === 'error'));
  const others = $derived(toasts.list.filter((t) => t.tone !== 'error'));
</script>

{#snippet item(t: Toast)}
  {@const Icon = icon[t.tone]}
  <li class="bg-popover text-popover-foreground ring-foreground/10 pointer-events-auto flex w-80 items-start gap-2.5 rounded-lg p-3 shadow-lg ring-1">
    <Icon class={cn('mt-0.5 size-4 shrink-0', tone[t.tone])} aria-hidden="true" />
    <div class="min-w-0 flex-1">
      <div class="font-semibold">{t.title}</div>
      {#if t.detail}<div class="text-muted-foreground mt-0.5 text-[12px] break-words">{t.detail}</div>{/if}
      {#if t.action}
        <Button variant="link" size="xs" class="mt-1 h-auto p-0" onclick={() => { t.action?.run(); dismiss(t.id); }}>{t.action.label}</Button>
      {/if}
    </div>
    <Button variant="ghost" size="icon-xs" aria-label="Dismiss" onclick={() => dismiss(t.id)}><X aria-hidden="true" /></Button>
  </li>
{/snippet}

<div class="pointer-events-none fixed right-4 bottom-8 z-50 flex flex-col items-end gap-2">
  <ul role="alert" class="flex flex-col gap-2">
    {#each errors as t (t.id)}{@render item(t)}{/each}
  </ul>
  <ul aria-live="polite" class="flex flex-col gap-2">
    {#each others as t (t.id)}{@render item(t)}{/each}
  </ul>
</div>
