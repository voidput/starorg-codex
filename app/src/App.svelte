<script lang="ts">
  import { jumpToSection } from '@/lib/scroll';
  /**
   * App frame (spec 9.3): sidebar, header with channel switcher / status / Apply, footer bar,
   * toasts, command palette (Ctrl/Cmd+K), Alt+0..9 screen shortcuts, skip link. The setup wizard
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
  import ReplaceDialog from '@/components/common/ReplaceDialog.svelte';
  import FullEmpty from '@/components/common/FullEmpty.svelte';
  import { Button } from '@/components/ui/button';
  import Wand from '@lucide/svelte/icons/wand-sparkles';
  import { findNav, isDevScreen, navItems, type ScreenId } from '@/lib/nav';
  import ExplorerScreen from '@/components/screens/ExplorerScreen.svelte';
  import DatasetsScreen from '@/components/screens/DatasetsScreen.svelte';
  import DiffScreen from '@/components/screens/DiffScreen.svelte';
  import FilesScreen from '@/components/screens/FilesScreen.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import UpdateBanner from '@/components/shell/UpdateBanner.svelte';
  import { updates } from '@/lib/stores/update.svelte';

  const params = new URLSearchParams(window.location.search);
  let palette = $state(false);
  let main = $state<HTMLElement | null>(null);
  const devOn = $derived(!!app.settings?.advanced.developer_mode);
  /** With Developer mode off the Dev screens are not rendered; their routes fall back to Home. */
  const screen = $derived<ScreenId>(isDevScreen(app.screen) && !devOn ? 'home' : app.screen);
  const page = $derived(findNav(screen));
  /** Screens that work on one channel's files show "Set up <channel>" instead of an error. */
  const needsSetup = $derived(!!app.channel && !app.channel.set_up && ['enhancements', 'strings', 'patches', 'blueprints', 'ships', 'backups'].includes(app.screen));

  function go(s: ScreenId) {
    app.go(s);
  }
  // Every navigation (sidebar, palette, Alt+n, links inside screens) moves focus to <main>, or
  // to a Settings section when one was asked for (footer Privacy -> Network settings).
  $effect(() => {
    if (app.navSeq === 0) return;
    void tick().then(() => {
      const section = app.settingsSection;
      app.settingsSection = null;
      if (section && app.screen === 'settings' && jumpToSection(section, 'instant')) return;
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
    if (e.altKey && !e.ctrlKey && !e.metaKey && /^[0-9]$/.test(e.key) && !shortcutsOff(e)) {
      const n = navItems(devOn).find((i) => i.key === e.key);
      if (n) {
        e.preventDefault();
        go(n.id);
      }
    }
  }

  onMount(() => {
    void app.init().then(() => {
      if (params.has('wizard')) app.wizard = true;
      // The start-up update check: Rust asks only with the switch on, at most once per 24 h.
      void updates.init();
    });
  });
</script>

<svelte:window onkeydown={onKey} />

<Tooltip.Provider>
  <a href="#main" class="bg-primary text-primary-foreground sr-only z-50 rounded px-3 py-1 focus:not-sr-only focus:fixed focus:top-2 focus:left-2">Skip to content</a>
  <div class="flex h-full flex-col" inert={app.wizard}>
    <div class="flex min-h-0 flex-1">
      <Sidebar bind:current={() => screen, (s) => go(s)} version={app.overview?.app_version ?? ''} dev={devOn} />
      <div class="flex min-w-0 flex-1 flex-col">
        <AppHeader {page} />
        <main id="main" bind:this={main} tabindex="-1" aria-labelledby="page-title" class="min-h-0 flex-1 overflow-y-auto px-6 py-5 outline-none">
          <UpdateBanner />
          {#if needsSetup && app.channel}
            <FullEmpty icon={Wand} title={`${app.channel.channel} is not set up`}>
              Set up {app.channel.channel} to see its {page.label.toLowerCase()}. Setup asks before anything is written to the game.
              {#snippet actions()}<Button onclick={() => (app.wizard = true)}><Wand aria-hidden="true" /> Set up {app.channel?.channel}</Button>{/snippet}
            </FullEmpty>
          {:else if screen === 'home'}
            <HomeScreen />
          {:else if screen === 'enhancements'}
            <EnhancementsScreen />
          {:else if screen === 'strings'}
            <StringsScreen />
          {:else if screen === 'patches'}
            <PatchesScreen />
          {:else if screen === 'blueprints'}
            <BlueprintsScreen />
          {:else if screen === 'ships'}
            <ShipsScreen />
          {:else if screen === 'backups'}
            <BackupsScreen />
          {:else if screen === 'activity'}
            <ActivityScreen />
          {:else if screen === 'explorer'}
            <ExplorerScreen />
          {:else if screen === 'datasets'}
            <DatasetsScreen />
          {:else if screen === 'diff'}
            <DiffScreen />
          {:else if screen === 'files'}
            <FilesScreen />
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
  <ReplaceDialog />
  <Toaster />
</Tooltip.Provider>
