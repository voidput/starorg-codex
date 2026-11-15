<script lang="ts">
  /**
   * A detail sheet: a modal dialog docked to the right edge (spec 9.4 Strings / Patches). Focus is
   * trapped and returns to the opener on close (bits-ui Dialog); Escape closes.
   */
  import type { Snippet } from 'svelte';
  import * as Dialog from '@/components/ui/dialog';
  import { cn } from '@/lib/utils';

  let {
    open = $bindable(false),
    title,
    description = null,
    width = 'w-[min(560px,calc(100vw-48px))]',
    children,
    footer,
    beforeClose = null,
  }: {
    open: boolean;
    title: string;
    description?: string | null;
    width?: string;
    children: Snippet;
    footer?: Snippet;
    /** Return false to keep the sheet open (the screen asks first, e.g. unsaved text). */
    beforeClose?: (() => boolean) | null;
  } = $props();
</script>

<Dialog.Root bind:open={() => open, (v) => { if (v || !beforeClose || beforeClose()) open = v; }}>
  <Dialog.Content
    class={cn(
      'top-0 right-0 left-auto flex h-full max-w-none translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-l p-0 sm:max-w-none data-open:zoom-in-100 data-closed:zoom-out-100 data-open:slide-in-from-right-8',
      width,
    )}
  >
    <Dialog.Header class="border-b px-5 py-4 pr-12">
      <Dialog.Title class="mono truncate text-[14px]">{title}</Dialog.Title>
      {#if description}<Dialog.Description>{description}</Dialog.Description>{/if}
    </Dialog.Header>
    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">{@render children()}</div>
    {#if footer}<div class="flex flex-wrap items-center justify-end gap-2 border-t px-5 py-3">{@render footer()}</div>{/if}
  </Dialog.Content>
</Dialog.Root>
