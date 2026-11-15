<script lang="ts">
  /**
   * Preferences: a few app settings worth deciding at setup, all existing Settings fields
   * (Network, Appearance). Network switches stay off unless turned on here (docs/privacy.md).
   * The update question is not asked in the Microsoft Store build (the Store updates it).
   * The theme is previewed at once; nothing is saved until Apply.
   */
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import { app } from '@/lib/stores/app.svelte';
  import { updates } from '@/lib/stores/update.svelte';
  import { applyAppearance, defaultAppearance } from '@/lib/theme';
  import { requestHint, storeManaged } from '@/lib/update';
  import type { Theme } from '@/lib/contract/types';
  import type { Prefs } from './draft.svelte';

  let { prefs }: { prefs: Prefs } = $props();

  const store = $derived(storeManaged(updates.status));
  const repo = $derived(updates.status?.prefs.repo ?? 'voidput/starorg-strings');
  const themes: { id: Theme; label: string }[] = [
    { id: 'system', label: 'System' },
    { id: 'dark', label: 'Dark' },
    { id: 'light', label: 'Light' },
  ];

  // Live preview of the look (primitive deps); SetupWizard puts the saved look back on leave.
  const theme = $derived(prefs.theme);
  const reduce = $derived(prefs.reduce_effects);
  $effect(() => {
    const base = app.settings?.appearance ?? defaultAppearance();
    applyAppearance({ ...base, theme, reduce_effects: reduce });
  });
</script>

<div class="grid max-w-2xl gap-4">
  <section aria-labelledby="prefs-net" class="grid gap-1">
    <h3 id="prefs-net" class="eyebrow text-muted-foreground">Network</h3>
    {#if store}
      <p class="text-[13px]" role="status">{updates.status?.install_note || 'Updates come from the Microsoft Store.'}</p>
    {:else}
      <SwitchRow id="prefs-updates" label="Check for updates automatically" hint={requestHint(repo)} checked={prefs.check_updates} onchange={(v) => (prefs.check_updates = v)} />
    {/if}
    <SwitchRow
      id="prefs-starorg"
      label="Connect StarOrg Tools"
      hint="Lets you sign in to starorg.tools in your browser to pull and push owned blueprints. Nothing is sent until you sign in."
      checked={prefs.starorg}
      onchange={(v) => (prefs.starorg = v)}
    />
    <p class="text-muted-foreground text-[12px]">Both are off unless you turn them on. With them off, StarOrg Codex' own code makes no network request.</p>
  </section>
  <section aria-labelledby="prefs-look" class="grid gap-2">
    <h3 id="prefs-look" class="eyebrow text-muted-foreground">Appearance</h3>
    <div class="flex flex-wrap items-center gap-3">
      <span class="w-24">Theme</span>
      <Segmented ariaLabel="Theme" options={themes} value={prefs.theme} onchange={(v) => (prefs.theme = v)} />
    </div>
    <SwitchRow
      id="prefs-reduce"
      label="Reduce effects"
      hint="Solid surfaces instead of see-through blur, for a smoother window. On by default on Linux."
      checked={prefs.reduce_effects}
      onchange={(v) => (prefs.reduce_effects = v)}
    />
  </section>
  <p class="text-muted-foreground text-[12px]">All of these can be changed later in Settings.</p>
</div>
