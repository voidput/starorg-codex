<script lang="ts">
  /**
   * A card that folds to its title: drop-in for Card.Root (same Header/Title/Content inside). The
   * title becomes the disclosure button (Enter/Space, Left collapses, Right expands); the
   * description and content hide while collapsed. Remembered per `key` in localStorage.
   * `bare`: no card of its own, for a component that renders its own Card.Root inside.
   */
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';
  import * as Card from '@/components/ui/card';
  import { setCardCollapse } from '@/components/ui/card/collapse';
  import { cn } from '@/lib/utils';
  import { CARDS_KEY, readCollapsed, withCollapsed, writeCollapsed } from '@/lib/collapse';

  let {
    key,
    bare = false,
    class: className,
    children,
    ...rest
  }: HTMLAttributes<HTMLDivElement> & {
    /** Storage id, e.g. `blueprints.finder`. */
    key: string;
    bare?: boolean;
    children?: Snippet;
  } = $props();

  const uid = $props.id();
  const initial = () => !readCollapsed(CARDS_KEY).includes(key);
  let open = $state(initial());

  setCardCollapse({
    get open() {
      return open;
    },
    set(v: boolean) {
      open = v;
      // Read again: another card may have changed the list since this one mounted.
      writeCollapsed(CARDS_KEY, withCollapsed(readCollapsed(CARDS_KEY), key, !v));
    },
    contentId: `${uid}-body`,
  });
</script>

{#if bare}
  {@render children?.()}
{:else}
  <!-- Collapsed: as tall as its header, not stretched to the row of the grid it sits in. -->
  <Card.Root data-collapsed={open ? undefined : ''} class={cn(className, !open && 'self-start')} {...rest}>{@render children?.()}</Card.Root>
{/if}
