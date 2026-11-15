<script lang="ts">
  /** The one empty state: muted, centred. Inside a table pass `colspan` and it becomes a full-width row. */
  import type { Snippet } from 'svelte';
  
  import { cn } from '@/lib/utils';

  let { colspan, class: className, children }: { colspan?: number; class?: string; children: Snippet } = $props();
</script>

{#if colspan}
  <tr>
    <td {colspan} class={cn('text-muted-foreground py-6 text-center text-sm', className)}>{@render children()}</td>
  </tr>
{:else}
  <p class={cn('text-muted-foreground m-0 py-6 text-center text-sm', className)}>{@render children()}</p>
{/if}
