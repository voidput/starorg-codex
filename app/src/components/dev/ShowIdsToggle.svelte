<script lang="ts">
  /** "Show IDs" (addendum 7.12): raw ids instead of readable names on the Dev screens; kept per viewer. */
  import Binary from '@lucide/svelte/icons/binary';
  import { Button } from '@/components/ui/button';
  import { dev } from '@/lib/stores/dev.svelte';

  let { size = 'sm' }: { size?: 'sm' | 'xs' } = $props();
</script>

<Button
  variant={dev.showIds ? 'secondary' : 'outline'}
  {size}
  aria-pressed={dev.showIds}
  title={dev.showIds ? 'Showing raw ids: GUIDs, @keys, codes. Click for readable names.' : 'Showing readable names. Click for raw ids.'}
  onclick={() => dev.setShowIds(!dev.showIds)}><Binary aria-hidden="true" /> Show IDs</Button
>
