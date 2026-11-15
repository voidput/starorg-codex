<script lang="ts">
  import { onMount } from 'svelte';
  import { Construction } from '@lucide/svelte';
  import Sidebar from '@/components/shell/Sidebar.svelte';
  import ChannelsPanel from '@/components/shell/ChannelsPanel.svelte';
  import { findNav, type ScreenId } from '@/lib/nav';
  import { appInfo } from '@/lib/api';

  let current = $state<ScreenId>('home');
  let version = $state('');
  const page = $derived(findNav(current));

  onMount(() => {
    appInfo()
      .then((i) => (version = i.version))
      .catch(() => {});
  });
</script>

<div class="flex h-full flex-col">
  <div class="flex min-h-0 flex-1">
    <Sidebar bind:current {version} />
    <div class="flex min-w-0 flex-1 flex-col">
      <header class="flex items-center justify-between gap-4 border-b px-6 py-4">
        <div class="min-w-0">
          <div class="eyebrow text-primary">StarOrg Strings</div>
          <h1 class="text-h1 truncate">{page.label}</h1>
          <p class="text-muted-foreground truncate">{page.description}</p>
        </div>
      </header>
      <main class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        {#if current === 'home'}
          <ChannelsPanel />
        {:else}
          <div class="flex h-full flex-col items-center justify-center gap-2 text-center">
            <Construction class="text-muted-foreground size-8" aria-hidden="true" />
            <div class="font-semibold">{page.label} is not built yet</div>
            <p class="text-muted-foreground">Planned for milestone {page.milestone}.</p>
          </div>
        {/if}
      </main>
    </div>
  </div>
  <footer class="bg-sidebar text-muted-foreground mono flex h-6 shrink-0 items-center gap-4 border-t px-3 text-[11px]">
    <span>{version ? `v${version}` : 'StarOrg Strings'}</span>
    <span>No job running</span>
  </footer>
</div>
