<script lang="ts">
  /**
   * App frame (spec 9.3): sidebar, header with channel switcher / status / Apply, footer bar,
   * toasts, command palette (Ctrl/Cmd+K), Alt+1..7 screen shortcuts, skip link. The setup wizard
   * takes the full window on first run.
   */
  import { onMount, tick } from 'svelte';
  import * as Tooltip from '@/components/ui/tooltip';
  import Sidebar from '@/components/shell/Sidebar.svelte';
  import AppHeader from '@/components/shell/AppHeader.svelte';
  import FooterBar from '@/components/shell/FooterBar.svelte';
  import Toaster from '@/components/shell/Toaster.svelte';
  import CommandPalette from '@/components/shell/CommandPalette.svelte';
  import HomeScreen from '@/components/screens/HomeScreen.svelte';
  import SettingsScreen from '@/components/screens/SettingsScreen.svelte';
  import PlaceholderScreen from '@/components/screens/PlaceholderScreen.svelte';
  import SetupWizard from '@/components/wizard/SetupWizard.svelte';
  import { findNav, NAV_ITEMS, type ScreenId } from '@/lib/nav';
  import { app } from '@/lib/stores/app.svelte';

  const params = new URLSearchParams(window.location.search);
  let current = $state<ScreenId>((NAV_ITEMS.find((n) => n.id === params.get('screen'))?.id ?? 'home') as ScreenId);
  let palette = $state(false);
  let main = $state<HTMLElement | null>(null);
  const page = $derived(findNav(current));

  async function go(s: ScreenId) {
    current = s;
    await tick();
    main?.focus();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      palette = !palette;
      return;
    }
    if (e.altKey && !e.ctrlKey && !e.metaKey && /^[1-7]$/.test(e.key) && !app.wizard) {
      const n = NAV_ITEMS.find((i) => i.key === e.key);
      if (n) {
        e.preventDefault();
        void go(n.id);
      }
    }
  }

  onMount(() => {
    void app.init().then(() => {
      if (params.has('wizard')) app.wizard = true;
    });
  });
</script>

<svelte:window onkeydown={onKey} />

<Tooltip.Provider>
  <a href="#main" class="bg-primary text-primary-foreground sr-only z-50 rounded px-3 py-1 focus:not-sr-only focus:fixed focus:top-2 focus:left-2">Skip to content</a>
  <div class="flex h-full flex-col" inert={app.wizard}>
    <div class="flex min-h-0 flex-1">
      <Sidebar bind:current version={app.overview?.app_version ?? ''} />
      <div class="flex min-w-0 flex-1 flex-col">
        <AppHeader {page} />
        <main id="main" bind:this={main} tabindex="-1" aria-labelledby="page-title" class="min-h-0 flex-1 overflow-y-auto px-6 py-5 outline-none">
          {#if current === 'home'}
            <HomeScreen />
          {:else if current === 'settings'}
            <SettingsScreen />
          {:else}
            <PlaceholderScreen {page} />
          {/if}
        </main>
      </div>
    </div>
    <FooterBar />
  </div>
  {#if app.wizard}
    <SetupWizard onclose={() => { app.wizard = false; void app.refresh(); }} />
  {/if}
  <CommandPalette bind:open={palette} go={(s) => void go(s)} />
  <Toaster />
</Tooltip.Provider>
