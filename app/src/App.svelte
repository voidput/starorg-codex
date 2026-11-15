<script lang="ts">
  /**
   * App frame (spec 9.3): sidebar, header with channel switcher / status / Apply, footer bar,
   * toasts, command palette (Ctrl/Cmd+K), Alt+1..8 screen shortcuts, skip link. The setup wizard
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
  import EnhancementsScreen from '@/components/screens/EnhancementsScreen.svelte';
  import StringsScreen from '@/components/screens/StringsScreen.svelte';
  import PatchesScreen from '@/components/screens/PatchesScreen.svelte';
  import BlueprintsScreen from '@/components/screens/BlueprintsScreen.svelte';
  import BackupsScreen from '@/components/screens/BackupsScreen.svelte';
  import ActivityScreen from '@/components/screens/ActivityScreen.svelte';
  import SetupWizard from '@/components/wizard/SetupWizard.svelte';
  import { findNav, NAV_ITEMS, type ScreenId } from '@/lib/nav';
  import { app } from '@/lib/stores/app.svelte';

  const params = new URLSearchParams(window.location.search);
  let palette = $state(false);
  let main = $state<HTMLElement | null>(null);
  const page = $derived(findNav(app.screen));

  function go(s: ScreenId) {
    app.go(s);
  }
  // Every navigation (sidebar, palette, Alt+n, links inside screens) moves focus to <main>.
  $effect(() => {
    if (app.navSeq === 0) return;
    void tick().then(() => {
      main?.focus();
      main?.scrollTo({ top: 0 });
    });
  });

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      palette = !palette;
      return;
    }
    if (e.altKey && !e.ctrlKey && !e.metaKey && /^[1-8]$/.test(e.key) && !app.wizard) {
      const n = NAV_ITEMS.find((i) => i.key === e.key);
      if (n) {
        e.preventDefault();
        go(n.id);
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
      <Sidebar bind:current={() => app.screen, (s) => go(s)} version={app.overview?.app_version ?? ''} />
      <div class="flex min-w-0 flex-1 flex-col">
        <AppHeader {page} />
        <main id="main" bind:this={main} tabindex="-1" aria-labelledby="page-title" class="min-h-0 flex-1 overflow-y-auto px-6 py-5 outline-none">
          {#if app.screen === 'home'}
            <HomeScreen />
          {:else if app.screen === 'enhancements'}
            <EnhancementsScreen />
          {:else if app.screen === 'strings'}
            <StringsScreen />
          {:else if app.screen === 'patches'}
            <PatchesScreen />
          {:else if app.screen === 'blueprints'}
            <BlueprintsScreen />
          {:else if app.screen === 'backups'}
            <BackupsScreen />
          {:else if app.screen === 'activity'}
            <ActivityScreen />
          {:else}
            <SettingsScreen />
          {/if}
        </main>
      </div>
    </div>
    <FooterBar />
  </div>
  {#if app.wizard}
    <SetupWizard onclose={() => { app.wizard = false; void app.refresh(); }} />
  {/if}
  <CommandPalette bind:open={palette} go={(s) => go(s)} />
  <Toaster />
</Tooltip.Provider>
