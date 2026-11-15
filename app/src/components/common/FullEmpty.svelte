<script lang="ts">
  /** Full-page empty state, on the tactical grid (spec 9.1). */
  import type { Snippet, Component } from 'svelte';

  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- lucide icon components
  let { icon: Icon, title, children, actions }: { icon: Component<any>; title: string; children?: Snippet; actions?: Snippet } = $props();
</script>

<div class="tactical-grid flex h-full min-h-80 flex-col items-center justify-center gap-3 rounded-lg border px-6 py-12 text-center">
  <Icon class="text-primary size-9" aria-hidden="true" />
  <h2 class="text-[16px] font-semibold">{title}</h2>
  {#if children}<div class="text-muted-foreground max-w-md">{@render children()}</div>{/if}
  {#if actions}<div class="mt-1 flex flex-wrap justify-center gap-2">{@render actions()}</div>{/if}
</div>
