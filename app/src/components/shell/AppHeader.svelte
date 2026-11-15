<script lang="ts">
  /** Header (spec 9.3): title + description left; channel switcher, status pill and Apply right. */
  import ChannelSwitcher from './ChannelSwitcher.svelte';
  import ApplyStatusPill from './ApplyStatusPill.svelte';
  import ApplyButton from './ApplyButton.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import type { NavItem } from '@/lib/nav';

  let { page }: { page: NavItem } = $props();
  const ch = $derived(app.channel);
</script>

<header class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b px-6 py-3">
  <div class="min-w-0">
    <div class="eyebrow text-primary">StarOrg Strings</div>
    <h1 id="page-title" class="text-h1 truncate">{page.label}</h1>
    <p class="text-muted-foreground truncate">{page.description}</p>
  </div>
  {#if ch && app.install}
    <div class="flex flex-wrap items-center gap-2">
      <ChannelSwitcher channels={app.channels} value={ch.channel} onchange={(c) => app.selectChannel(c)} />
      <ApplyStatusPill channel={ch} />
      <ApplyButton
        label={`Apply to ${ch.channel}`}
        busy={app.busy}
        disabled={!ch.set_up || ch.game_running}
        onclick={() => app.apply()}
      />
    </div>
  {/if}
</header>
