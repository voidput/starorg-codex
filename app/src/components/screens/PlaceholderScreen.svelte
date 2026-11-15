<script lang="ts">
  import Construction from '@lucide/svelte/icons/construction';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import type { NavItem } from '@/lib/nav';

  let { page }: { page: NavItem } = $props();
</script>

<FullEmpty icon={Construction} title="{page.label} is not built yet">
  Its commands are in the contract and answer from the mock backend; the screen arrives in the next part of issue #15.
</FullEmpty>
