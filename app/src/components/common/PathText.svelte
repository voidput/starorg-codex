<script lang="ts">
  /** A file path that wraps only after a separator, never mid-word ("St|arCitizen"). */
  import { cn } from '@/lib/utils';
  let { path, class: className = '' }: { path: string | null | undefined; class?: string } = $props();
  const parts = $derived((path ?? '').split(/(?<=[/\\])/));
</script>

<span class={cn('mono min-w-0', className)}>{#each parts as p, i (i)}{p}<wbr />{/each}</span>
