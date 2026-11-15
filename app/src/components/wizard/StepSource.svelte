<script lang="ts">
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import CloudDownload from '@lucide/svelte/icons/cloud-download';
  import { Badge } from '@/components/ui/badge';
  import RadioCard from './RadioCard.svelte';
  import type { Draft } from './draft.svelte';

  let { draft }: { draft: Draft } = $props();
</script>

<p class="text-muted-foreground">Stats and mission details come from the game's own data. Choose where to read them.</p>

<fieldset class="grid gap-2">
  <legend class="sr-only">Game data source</legend>
  <RadioCard name="source" value="local" bind:group={draft.data_source}>
    <span class="flex items-center gap-2 pr-6 font-semibold"><HardDrive class="text-primary size-4" aria-hidden="true" /> Read it on this computer <Badge variant="success">Default</Badge></span>
    <span class="text-muted-foreground">
      Reads <span class="mono">Data.p4k</span> of each channel. No network. The first read per game build takes about a minute; after that it is cached.
    </span>
  </RadioCard>
  <RadioCard name="source" value="published-dumps" bind:group={draft.data_source}>
    <span class="flex items-center gap-2 pr-6 font-semibold"><CloudDownload class="text-primary size-4" aria-hidden="true" /> Use published game data <Badge variant="warning">Network</Badge></span>
    <span class="text-muted-foreground">
      Downloads the datasets for your exact build from <span class="mono">github.com/voidput/sc-gamedata-dumps</span>, checked by checksum and read as data only. A build
      without a published dump (often PTU and HOTFIX) is still read locally. This turns on Settings → Network → Use published game data.
    </span>
  </RadioCard>
</fieldset>
