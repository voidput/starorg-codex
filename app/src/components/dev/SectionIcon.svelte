<script lang="ts">
  /** The icon of an Explorer section (7.17) or media view; a neutral dot for anything else. */
  import Rocket from '@lucide/svelte/icons/rocket';
  import Crosshair from '@lucide/svelte/icons/crosshair';
  import Cpu from '@lucide/svelte/icons/cpu';
  import Bomb from '@lucide/svelte/icons/bomb';
  import Swords from '@lucide/svelte/icons/swords';
  import Shirt from '@lucide/svelte/icons/shirt';
  import Puzzle from '@lucide/svelte/icons/puzzle';
  import Pill from '@lucide/svelte/icons/pill';
  import Pickaxe from '@lucide/svelte/icons/pickaxe';
  import Package from '@lucide/svelte/icons/package';
  import ScrollText from '@lucide/svelte/icons/scroll-text';
  import MapPin from '@lucide/svelte/icons/map-pin';
  import Flag from '@lucide/svelte/icons/flag';
  import Hammer from '@lucide/svelte/icons/hammer';
  import Backpack from '@lucide/svelte/icons/backpack';
  import Store from '@lucide/svelte/icons/store';
  import User from '@lucide/svelte/icons/user';
  import Box from '@lucide/svelte/icons/box';
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
  import CircleDashed from '@lucide/svelte/icons/circle-dashed';
  import Volume2 from '@lucide/svelte/icons/volume-2';
  import { cn } from '@/lib/utils';

  let { id, class: className = '' }: { id: string; class?: string } = $props();
  const ICONS = {
    ships: Rocket,
    'ship-weapons': Crosshair,
    components: Cpu,
    missiles: Bomb,
    'fps-weapons': Swords,
    armor: Shirt,
    attachments: Puzzle,
    consumables: Pill,
    mining: Pickaxe,
    commodities: Package,
    missions: ScrollText,
    locations: MapPin,
    factions: Flag,
    blueprints: Hammer,
    loadouts: Backpack,
    shops: Store,
    characters: User,
    items: Box,
    'audio-ui': LayoutDashboard,
    models: Box,
    sounds: Volume2,
  } as const;
  const Icon = $derived(ICONS[id as keyof typeof ICONS] ?? CircleDashed);
</script>

<Icon class={cn('text-muted-foreground size-3.5 shrink-0', className)} aria-hidden="true" />
