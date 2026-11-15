<script lang="ts">
  /**
   * App frame (spec 9.3): sidebar, header with channel switcher / status / Apply, footer bar,
   * toasts, command palette (Ctrl/Cmd+K), Alt+1..9 screen shortcuts, skip link. The setup wizard
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
  import ShipsScreen from '@/components/screens/ShipsScreen.svelte';
  import BackupsScreen from '@/components/screens/BackupsScreen.svelte';
  import ActivityScreen from '@/components/screens/ActivityScreen.svelte';
  import SetupWizard from '@/components/wizard/SetupWizard.svelte';
  import UnsavedDialog from '@/components/common/UnsavedDialog.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import { Button } from '@/components/ui/button';
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import { findNav, NAV_ITEMS, type ScreenId } from '@/lib/nav';
  import { app } from '@/lib/stores/app.svelte';

  const params = new URLSearchParams(window.location.search);
  let palette = $state(false);
  let main = $state<HTMLElement | null>(null);
  const page = $derived(findNav(app.screen));
  /** Screens that work on one channel's files show "Set up <channel>" instead of an error. */
  const needsSetup = $derived(!!app.channel && !app.channel.set_up && ['enhancements', 'strings', 'patches', 'blueprints', 'ships', 'backups'].includes(app.screen));

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

  /** A dialog or sheet is open, or the user is typing: screen shortcuts stay quiet. */
  function shortcutsOff(e: KeyboardEvent): boolean {
    if (app.wizard || document.querySelector('[data-slot=dialog-content][data-state=open]')) return true;
    const t = e.target as HTMLElement | null;
    return !!t?.closest('input, textarea, select, [contenteditable=true]');
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      if (app.wizard) return;
      e.preventDefault();
      palette = !palette;
      return;
    }
    if (e.altKey && !e.ctrlKey && !e.metaKey && /^[1-9]$/.test(e.key) && !shortcutsOff(e)) {
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
          {#if needsSetup && app.channel}
            <FullEmpty icon={Wand} title={`${app.channel.channel} is not set up`}>
              Set up {app.channel.channel} to see its {page.label.toLowerCase()}. Setup asks before anything is written to the game.
              {#snippet actions()}<Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Set up {app.channel?.channel}</Button>{/snippet}
            </FullEmpty>
          {:else if app.screen === 'home'}
            <HomeScreen />
          {:else if app.screen === 'enhancements'}
            <EnhancementsScreen />
          {:else if app.screen === 'strings'}
            <StringsScreen />
          {:else if app.screen === 'patches'}
            <PatchesScreen />
          {:else if app.screen === 'blueprints'}
            <BlueprintsScreen />
          {:else if app.screen === 'ships'}
            <ShipsScreen />
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
  {#if !app.wizard}<CommandPalette bind:open={palette} go={(s) => go(s)} />{/if}
  <UnsavedDialog />
  <Toaster />
</Tooltip.Provider>
