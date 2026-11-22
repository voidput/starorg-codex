<script lang="ts">
  /**
   * Enhancements (spec 9.4): a list (General, the generator categories, Name tags), the switches of
   * M2's profile with labels from `profile_schema`, the Tag Builder, mission options, the
   * blueprint markers (their own tab: [Owned] / [Missing] / [BP Owned] and their colours), Where to
   * get (WHERE TO GET blocks, spec obtain-hints) and a live preview. Settings are per channel; Save writes profile.toml, Save & apply also applies.
   *
   * The draft survives refreshes (the load effect is keyed on the install/channel string only).
   * While it differs from the saved profile it is registered as `app.draft`, so switching channel,
   * leaving the screen and the header Apply ask first (UnsavedDialog). "Copy settings from" and a
   * preset load into the draft; nothing is saved until Save.
   */
  import { onDestroy, untrack } from 'svelte';
  import Save from '@lucide/svelte/icons/save';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Check from '@lucide/svelte/icons/check';
  import Circle from '@lucide/svelte/icons/circle';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Download from '@lucide/svelte/icons/download';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import { Skeleton } from '@/components/ui/skeleton';
  import * as Card from '@/components/ui/card';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import SelectField from '@/components/common/SelectField.svelte';
  import ErrorState from '@/components/common/ErrorState.svelte';
  import TagBuilder from '@/components/enhancements/TagBuilder.svelte';
  import PreviewPanel from '@/components/enhancements/PreviewPanel.svelte';
  import InGamePreview from '@/components/common/InGamePreview.svelte';
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { CategoryId, ChannelRef, ObtainMode, ObtainStatus, PatchView, PreviewItem, Preset, Profile, ProfileSchema, TagKind, TagKindSettings } from '@/lib/contract/types';
  import { OBTAIN_MODES, fileAge } from '@/lib/obtain';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';
  import { LINKS, openLink } from '@/lib/links';
  import { fetchUexNow } from '@/lib/uex';

  type Tab = 'general' | CategoryId | 'blueprints' | 'name-tags' | 'quest-items';

  let schema = $state<ProfileSchema | null>(null);
  let saved = $state<string>('');
  let draft = $state<Profile | null>(null);
  let patches = $state<PatchView[]>([]);
  let error = $state<ContractError | null>(null);
  let preview = $state<PreviewItem[] | null>(null);
  let previewing = $state(false);
  // A jump from another screen (Blueprints → "Edit blueprint markers") picks the tab once.
  const asked = () => {
    const t = app.enhancementsTab;
    app.enhancementsTab = null;
    return (t === 'general' || t === 'blueprints' || t === 'name-tags' ? t : 'general') as Tab;
  };
  let selected = $state<Tab>(untrack(asked));
  let saving = $state(false);
  let loadSeq = $state(0);

  const refOf = (k: string): ChannelRef => {
    const i = k.lastIndexOf('/');
    return { install: k.slice(0, i), channel: k.slice(i + 1) };
  };
  const dirty = $derived(draft !== null && JSON.stringify(draft) !== saved);
  const otherChannels = $derived(app.channels.filter((c) => c.set_up && c.channel !== app.channel?.channel).map((c) => ({ id: c.channel, label: c.channel })));

  /** Core refuses %, <, ~, \n and non-ASCII in the header (M2 Profile::validate). */
  const headerError = $derived.by(() => {
    const h = draft?.header ?? '';
    if (/[^\x20-\x7e]/.test(h)) return 'Use plain ASCII letters, digits and punctuation only.';
    if (/[%<~]/.test(h) || h.includes('\\n')) return 'Not allowed here: %, <, ~ and \\n.';
    if (h.length > 80) return 'At most 80 characters.';
    return null;
  });

  // Load the saved profile when the install/channel changes. Keyed on the string only, so the
  // overview refresh after an apply does not reload it over the draft.
  $effect(() => {
    const k = app.targetKey;
    void loadSeq;
    if (!k) return;
    let stale = false;
    untrack(() => {
      error = null;
      const target = refOf(k);
      Promise.all([api.getProfile(target), schema ? Promise.resolve(schema) : api.profileSchema(), api.listPatches(target)])
        .then(([p, s, ps]) => {
          if (stale) return;
          schema = s;
          patches = ps;
          draft = p;
          saved = JSON.stringify(p);
        })
        .catch((e) => !stale && (error = toContractError(e)));
    });
    return () => (stale = true);
  });

  // Preview follows the draft (debounced); the backend renders it with the real generators.
  const draftJson = $derived(draft ? JSON.stringify(draft) : '');
  let previewSeq = $state(0);
  $effect(() => {
    const j = draftJson;
    const k = app.targetKey;
    void previewSeq;
    if (!j || !k) return;
    let stale = false;
    previewing = true;
    const t = setTimeout(() => {
      api
        .buildPreview({ target: refOf(k), profile: JSON.parse(j) as Profile, preset: null })
        .then((r) => !stale && (preview = r))
        .catch((e) => !stale && toast('error', 'Preview failed', toContractError(e).message))
        .finally(() => !stale && (previewing = false));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  });

  // Unsaved edits: navigation, channel switch and header Apply ask first.
  const handle = {
    label: 'Enhancements',
    save: () => save(false),
    discard: () => revert(),
  };
  $effect(() => {
    app.draft = dirty ? handle : null;
  });
  onDestroy(() => {
    if (app.draft === handle) app.draft = null;
  });

  function edit(f: (p: Profile) => void) {
    if (!draft) return;
    const p = structuredClone($state.snapshot(draft)) as Profile;
    f(p);
    p.preset = null;
    draft = p;
  }

  function revert() {
    if (saved) draft = JSON.parse(saved) as Profile;
  }

  async function save(andApply: boolean): Promise<boolean> {
    if (!draft || !app.target || headerError) return false;
    const target = $state.snapshot(app.target);
    saving = true;
    try {
      const p = await api.saveProfile({ target, profile: $state.snapshot(draft) as Profile });
      draft = p;
      saved = JSON.stringify(p);
      toast('success', `Saved settings for ${target.channel}`, andApply ? 'Applying now.' : 'Apply to write them into the game.');
      await app.refresh();
      if (andApply) await app.applyOrAsk();
      return true;
    } catch (e) {
      toast('error', 'Settings not saved', toContractError(e).message);
      return false;
    } finally {
      saving = false;
    }
  }

  async function usePreset(p: Preset) {
    try {
      const prof = await api.presetProfile(p);
      draft = { ...prof, patches_disabled: draft?.patches_disabled ?? [] };
    } catch (e) {
      toast('error', 'Preset not loaded', toContractError(e).message);
    }
  }

  /** Loads the other channel's saved settings into the draft; Save keeps them, Revert undoes. */
  async function copyFrom(channel: string) {
    if (!app.target) return;
    try {
      const p = await api.getProfile({ install: app.target.install, channel });
      draft = p;
      toast('info', `Loaded the settings of ${channel}`, `Not saved yet: Save keeps them for ${app.target.channel}, Revert goes back.`);
    } catch (e) {
      toast('error', 'Copy failed', toContractError(e).message);
    }
  }

  // Category on/off and stat lines.
  const CAT_FIELD: Record<CategoryId, 'ships' | 'components' | 'ship_weapons' | 'fps_weapons' | 'missiles' | 'missions' | 'obtain'> = {
    ships: 'ships',
    components: 'components',
    'ship-weapons': 'ship_weapons',
    'fps-weapons': 'fps_weapons',
    missiles: 'missiles',
    missions: 'missions',
    obtain: 'obtain',
  };
  const SWITCH_LABEL: Record<CategoryId, string> = {
    ships: 'Add a stats block to ship descriptions',
    components: 'Add stats to component descriptions',
    'ship-weapons': 'Add stats to ship weapon descriptions',
    'fps-weapons': 'Add stats to personal weapon descriptions',
    missiles: 'Add stats to missile and bomb descriptions',
    missions: 'Add tags and sections to contracts',
    obtain: 'Add WHERE TO GET to item and commodity descriptions',
  };
  const TAG_FIELD: Record<TagKind, 'components' | 'ship_weapons' | 'missiles'> = { components: 'components', 'ship-weapons': 'ship_weapons', missiles: 'missiles' };
  const isOn = (p: Profile, c: CategoryId) => p[CAT_FIELD[c]];
  const lineOn = (p: Profile, key: string) => !p.lines_off.includes(key);
  const setLine = (key: string, on: boolean) =>
    edit((p) => {
      p.lines_off = on ? p.lines_off.filter((k) => k !== key) : [...new Set([...p.lines_off, key])].sort();
    });

  // "Fix known text bugs" = no built-in fix in patches_disabled (spec 4.5; Patches lists each).
  // The hand-written quest hints are a built-in patch too, with their own switch (addendum §13).
  const QUEST_PATCH = 'quest-items';
  const builtin = $derived(patches.filter((p) => p.origin === 'built-in' && p.id !== QUEST_PATCH).map((p) => p.id));
  const questPack = $derived(patches.some((p) => p.id === QUEST_PATCH));
  const questPackOn = $derived(!!draft && questPack && !draft.patches_disabled.includes(QUEST_PATCH));
  const setQuestPack = (on: boolean) =>
    edit((p) => {
      p.patches_disabled = on ? p.patches_disabled.filter((id) => id !== QUEST_PATCH) : [...new Set([...p.patches_disabled, QUEST_PATCH])];
    });
  const fixesOff = $derived(draft ? builtin.filter((id) => draft!.patches_disabled.includes(id)).length : 0);
  const setFixes = (on: boolean) =>
    edit((p) => {
      p.patches_disabled = on ? p.patches_disabled.filter((id) => !builtin.includes(id)) : [...new Set([...p.patches_disabled, ...builtin])];
    });

  const tabs = $derived.by(() => {
    if (!schema || !draft) return [] as { id: Tab; label: string; on: boolean | null }[];
    const d = draft;
    return [
      { id: 'general' as Tab, label: 'General', on: null },
      ...schema.categories.flatMap((c) => {
        // Where to get goes last, after Quest items: both add to item descriptions.
        if (c.id === 'obtain') return [];
        const tab = { id: c.id as Tab, label: c.label, on: isOn(d, c.id) };
        // Blueprint markers sit right after Missions: they are part of the contract texts.
        return c.id === 'missions' ? [tab, { id: 'blueprints' as Tab, label: 'Blueprints', on: d.missions && (d.mission.blueprints || d.mission.title_blueprint) }] : [tab];
      }),
      { id: 'name-tags' as Tab, label: 'Name tags', on: d.tags.components.enabled || d.tags.ship_weapons.enabled || d.tags.missiles.enabled },
      { id: 'quest-items' as Tab, label: 'Quest items', on: d.quest_hints || questPackOn },
      ...schema.categories.filter((c) => c.id === 'obtain').map((c) => ({ id: c.id as Tab, label: c.label, on: d.obtain })),
    ];
  });
  const cat = $derived(schema?.categories.find((c) => c.id === selected) ?? null);
  const sectionLabel = $derived(tabs.find((t) => t.id === selected)?.label ?? 'this section');

  // "Read game data": the preview's examples without an apply (nothing goes into the game folder).
  let reading = $state(false);
  async function readGameData() {
    if (!app.target) return;
    reading = true;
    try {
      await api.readGameData($state.snapshot(app.target));
      previewSeq++;
      await app.refresh();
    } catch (e) {
      toast('error', 'Could not read the game data', toContractError(e).message);
    } finally {
      reading = false;
    }
  }

  // Where to get: the UEX folder and files, read when the tab opens ("Check again" re-reads).
  let uex = $state<ObtainStatus | null>(null);
  let uexError = $state<string | null>(null);
  let uexSeq = $state(0);
  $effect(() => {
    if (selected !== 'obtain') return;
    void uexSeq;
    let stale = false;
    untrack(() => {
      uexError = null;
      api
        .obtainStatus()
        .then((s) => !stale && (uex = s))
        .catch((e) => !stale && (uexError = toContractError(e).message));
    });
    return () => (stale = true);
  });
  const uexOn = $derived(!!app.settings?.network.uex);
  let uexFetching = $state(false);
  async function fetchUex() {
    uexFetching = true;
    const s = await fetchUexNow();
    if (s) uex = s;
    uexFetching = false;
  }
  function uexSettings() {
    app.settingsSection = 'network';
    app.go('settings');
  }
  const jumpTab = (t: Tab) => {
    selected = t;
    document.getElementById(`cat-tab-${t}`)?.focus();
  };

  function tabKey(e: KeyboardEvent, i: number) {
    const n = tabs.length;
    const to = e.key === 'ArrowDown' ? (i + 1) % n : e.key === 'ArrowUp' ? (i - 1 + n) % n : e.key === 'Home' ? 0 : e.key === 'End' ? n - 1 : -1;
    if (to < 0) return;
    e.preventDefault();
    selected = tabs[to]!.id;
    (e.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLElement>('[role=tab]')[to]?.focus();
  }

  const presets: { id: Preset; label: string }[] = [
    { id: 'recommended', label: 'Recommended' },
    { id: 'minimal', label: 'Minimal' },
    { id: 'everything', label: 'Everything' },
  ];
  // Colour settings: an EM tag number, 0 = none. EM3/EM4 are the looks stock mobiGlas text uses
  // (seen in game); EM1, EM2 and EM5 exist in stock, their look is not verified yet.
  type EmField = 'header_em' | 'label_em' | 'title_bp_em' | 'title_rep_em' | 'owned_em' | 'missing_em' | 'variant_em' | 'reward_em';
  const emOptions: { id: string; label: string }[] = [
    { id: '0', label: 'None' },
    { id: '3', label: 'EM3 · underline' },
    { id: '4', label: 'EM4 · blue' },
    { id: '1', label: 'EM1 · look not verified' },
    { id: '2', label: 'EM2 · look not verified' },
    { id: '5', label: 'EM5 · look not verified' },
  ];
  const setEm = (f: EmField, v: string) => edit((p) => (p.mission[f] = Number(v)));
  const COLOUR_TEST = 'Please select a contract. Colour test: <EM1>EM1</EM1> <EM2>EM2</EM2> <EM3>EM3</EM3> <EM4>EM4</EM4> <EM5>EM5</EM5>';
</script>

{#snippet emRow(f: EmField, label: string, disabled: boolean)}
  <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
    <span class="w-40 text-[12.5px]" id="em-{f}">{label}</span>
    <SelectField ariaLabel={`${label}: colour`} options={emOptions} value={String(draft?.mission[f] ?? 0)} {disabled} onchange={(v) => setEm(f, v)} class="w-44" />
  </div>
{/snippet}

{#if error}
  <ErrorState title="Could not read the enhancement settings" {error} onretry={() => loadSeq++} />
{:else if !draft || !schema}
  <div class="grid gap-3"><Skeleton class="h-10 w-full" /><Skeleton class="h-64 w-full" /></div>
{:else}
  {@const d = draft}
  <div class="@container grid gap-4">
    <div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Enhancement settings">
      <label class="text-muted-foreground text-[12px]" for="enh-preset">Start from</label>
      <SelectField id="enh-preset" ariaLabel="Start from a preset (replaces all switches)" options={presets} value={d.preset} placeholder="Custom" onchange={(v) => usePreset(v)} />
      {#if otherChannels.length}
        <SelectField ariaLabel="Load the settings of another channel" options={otherChannels} value={null} placeholder="Copy settings from…" onchange={(v) => copyFrom(v)} class="min-w-40" />
      {/if}
      <span class="text-muted-foreground text-[12px]">Replaces all switches; not saved until Save.</span>
      <span class="flex-1"></span>
      <span class={cn('text-[12px]', dirty ? 'text-warning' : 'text-muted-foreground')} aria-live="polite">{dirty ? 'Unsaved changes' : 'Saved'}</span>
      <Button variant="ghost" size="sm" disabled={!dirty || saving} onclick={revert}><Undo2 aria-hidden="true" /> Revert</Button>
      <Button variant="outline" size="sm" disabled={!dirty || saving || !!headerError} onclick={() => save(false)}><Save aria-hidden="true" /> Save</Button>
      <Button size="sm" disabled={saving || !!app.blocked || !app.channel?.set_up || !!headerError} title={app.blocked ?? undefined} onclick={() => save(true)}><Check aria-hidden="true" /> Save &amp; apply</Button>
    </div>

    <div class="grid grid-cols-[164px_minmax(0,1fr)] gap-4 @min-[880px]:grid-cols-[164px_minmax(0,1fr)_minmax(260px,330px)] @min-[1280px]:grid-cols-[164px_minmax(0,1fr)_minmax(340px,480px)] @min-[1700px]:grid-cols-[164px_minmax(0,1fr)_minmax(420px,620px)]">
      <div role="tablist" aria-orientation="vertical" aria-label="Categories" class="flex flex-col gap-0.5 self-start">
        {#each tabs as t, i (t.id)}
          <button
            type="button"
            role="tab"
            id="cat-tab-{t.id}"
            aria-selected={selected === t.id}
            aria-controls="cat-panel"
            tabindex={selected === t.id ? 0 : -1}
            class={cn(
              'focus-visible:ring-ring/60 flex h-8 items-center justify-between gap-2 rounded-md px-2.5 text-left outline-none focus-visible:ring-2',
              selected === t.id ? 'bg-accent text-foreground font-medium' : 'hover:bg-muted text-foreground/85',
            )}
            onclick={() => (selected = t.id)}
            onkeydown={(e) => tabKey(e, i)}
          >
            <span class="truncate">{t.label}</span>
            {#if t.on !== null}
              <span class={cn('flex items-center gap-1 text-[12px]', t.on ? 'text-success' : 'text-muted-foreground')}>
                {#if t.on}<CircleCheck class="size-3" aria-hidden="true" />{:else}<Circle class="size-3" aria-hidden="true" />{/if}{t.on ? 'On' : 'Off'}
              </span>
            {/if}
          </button>
        {/each}
      </div>

      <div id="cat-panel" role="tabpanel" aria-labelledby="cat-tab-{selected}" class="grid min-w-0 gap-4 self-start">
        <div class="sticky top-0 z-10 min-w-0 @min-[880px]:hidden"><PreviewPanel items={preview} loading={previewing} section={selected} {sectionLabel} canRead={!!app.channel?.set_up} {reading} onread={readGameData} strip /></div>

        {#if selected === 'general'}
          <Card.Root>
            <Card.Header><Card.Title>Format</Card.Title><Card.Description>Shared by every category that adds a stats block.</Card.Description></Card.Header>
            <Card.Content class="grid gap-3">
              <div class="flex flex-wrap items-center gap-3">
                <span class="w-28" id="fmt-place">Stats block</span>
                <Segmented
                  ariaLabel="Stats block placement"
                  options={[{ id: 'below', label: 'Below the description' }, { id: 'above', label: 'Above it' }]}
                  value={d.stats_above ? 'above' : 'below'}
                  onchange={(v) => edit((p) => (p.stats_above = v === 'above'))}
                />
              </div>
              <div class="flex flex-wrap items-start gap-3">
                <label for="fmt-header" class="w-28 pt-1.5">Header line</label>
                <div class="grid gap-1">
                  <Input
                    id="fmt-header"
                    class="mono h-8 w-64"
                    value={d.header}
                    aria-invalid={!!headerError}
                    aria-describedby="fmt-header-hint"
                    oninput={(e) => edit((p) => (p.header = (e.currentTarget as HTMLInputElement).value))}
                  />
                  <span id="fmt-header-hint" class={cn('text-[12px]', headerError ? 'text-destructive' : 'text-muted-foreground')} role={headerError ? 'alert' : undefined}>
                    {headerError ?? 'Plain text; the game shows it above the stats.'}
                  </span>
                </div>
              </div>
            </Card.Content>
          </Card.Root>
          <Card.Root>
            <Card.Header><Card.Title>Fixes and stamp</Card.Title></Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow
                id="fix-bugs"
                label="Fix known CIG text bugs"
                hint={fixesOff && fixesOff < builtin.length ? `${fixesOff} of ${builtin.length} fixes are off; see Patches.` : 'Built-in patches; each can be turned off under Patches.'}
                checked={fixesOff < builtin.length}
                onchange={setFixes}
              />
              <SwitchRow id="branding" label="Add a small StarOrg Codex stamp" hint="One line at the end of the main menu version text. Off by default." checked={d.branding} onchange={(v) => edit((p) => (p.branding = v))} />
            </Card.Content>
          </Card.Root>
          <Card.Root>
            <Card.Header>
              <Card.Title>Colour test</Card.Title>
              <Card.Description>Shows what each text style looks like in the game, so better colours can be picked. For testing; turn it off when done.</Card.Description>
            </Card.Header>
            <Card.Content class="grid gap-3">
              <SwitchRow id="colour-test" label="Add a colour test line" hint="Appends one line to the Contracts Manager's “Please select a contract.” prompt. Turning it off and applying again removes it." checked={d.colour_test} onchange={(v) => edit((p) => (p.colour_test = v))} />
              <ol class="text-muted-foreground grid list-decimal gap-1 pl-5 text-[12.5px]">
                <li>Save &amp; apply, then start the game.</li>
                <li>Open mobiGlas, then Contracts Manager. With no contract selected the detail pane says “Please select a contract.”, followed by <span class="text-foreground">Colour test: EM1 EM2 EM3 EM4 EM5</span>.</li>
                <li>Note how each word looks: its colour, and whether it is underlined, bold or larger. If you see the tags themselves, such as &lt;EM1&gt;, that pane does not read styles.</li>
                <li>
                  Report what you saw through
                  <button type="button" class="text-primary underline-offset-2 hover:underline" title={LINKS.support.title} onclick={() => openLink('support')}>Support on Discord</button>
                  (also the Support link in the footer); EM1, EM2 and EM5 then get real names and colours here.
                </li>
              </ol>
              <div class="grid gap-1">
                <span class="text-muted-foreground text-[11px] font-medium uppercase">Our guess, not the game</span>
                <InGamePreview text={COLOUR_TEST} note={false} />
              </div>
            </Card.Content>
          </Card.Root>
        {:else if selected === 'name-tags'}
          <Card.Root>
            <Card.Header><Card.Title>Tag Builder</Card.Title><Card.Description>Short tags in item names, such as [IND-S2-A], built per kind of item.</Card.Description></Card.Header>
            <Card.Content class="grid gap-3">
              {#each schema.tag_kinds as k (k.kind)}
                <TagBuilder schema={k} tag={d.tags[TAG_FIELD[k.kind]]} onchange={(nt: TagKindSettings) => edit((p) => (p.tags[TAG_FIELD[k.kind]] = nt))} />
              {/each}
              <div class="grid gap-1 border-t pt-2">
                <SwitchRow id="tag-short" label="Also tag short names" hint="Short names show in tight spots such as the power triangle list. One switch for every kind." checked={d.tags.short_names} onchange={(v) => edit((p) => (p.tags.short_names = v))} />
                <SwitchRow id="tag-tools" label="Size tag on tools" hint="Tractor beams, salvage heads, mining lasers and EMPs get a size-only tag, such as [S2]." checked={d.tags.tool_size_tag} onchange={(v) => edit((p) => (p.tags.tool_size_tag = v))} />
              </div>
            </Card.Content>
          </Card.Root>
        {:else if selected === 'quest-items'}
          <Card.Root>
            <Card.Header>
              <Card.Title>Quest items</Card.Title>
              <Card.Description>Says what mission items are for, in their description.</Card.Description>
            </Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow
                id="quest-hints"
                label="Add “Used in” to items missions use"
                hint="Names the contract and its star system, as “Used in: contract · Stanton”. Only for items the game data ties to a mission; ores, food, armour and Wikelo trades are left alone."
                checked={d.quest_hints}
                onchange={(v) => edit((p) => (p.quest_hints = v))}
              />
              {#if questPack}
                <SwitchRow
                  id="quest-pack"
                  label="Add mission hints the game data does not have"
                  hint="Hand-written, starting with Vanduul-Tech Smugglers in Nyx (the access card, decryption key and passcode disk). Each hint is skipped when CIG changes that item's text; see Patches."
                  checked={questPackOn}
                  onchange={setQuestPack}
                />
              {/if}
            </Card.Content>
          </Card.Root>
        {:else if selected === 'obtain' && cat}
          {@const off = !d.obtain}
          {@const shopLine = cat.lines.find((l) => l.key === 'obtain.shops')}
          <Card.Root>
            <Card.Header>
              <Card.Title>Where to get</Card.Title>
              <Card.Description>{cat.blurb} Names are never changed; the block goes after the stats.</Card.Description>
            </Card.Header>
            <Card.Content class="grid gap-3">
              <SwitchRow
                id="cat-on"
                label={SWITCH_LABEL.obtain}
                hint="From the game files: loot containers, blueprints, contract rewards, mining places and ore quality. The first apply per game build extracts a few datasets (seconds, cached after that)."
                checked={d.obtain}
                onchange={(v) => edit((p) => (p.obtain = v))}
              />
              <div class="grid gap-2 border-t pt-3">
                <div class="flex flex-wrap items-center gap-3">
                  <span class="w-28" id="obtain-mode">Detail</span>
                  <Segmented
                    ariaLabel="How much WHERE TO GET says"
                    options={OBTAIN_MODES.map((m) => ({ id: m.id, label: m.label }))}
                    value={d.obtain_mode}
                    disabled={off}
                    onchange={(v: ObtainMode) => edit((p) => (p.obtain_mode = v))}
                  />
                </div>
                <ul class="grid gap-1 text-[12.5px]" aria-label="Examples per mode">
                  {#each OBTAIN_MODES as m (m.id)}
                    <li class={cn('flex flex-wrap gap-x-2', m.id === d.obtain_mode ? 'text-foreground' : 'text-muted-foreground')}>
                      <span class="w-16 shrink-0 font-medium">{m.label}</span>
                      <span class="mono min-w-0 break-words">{m.example}</span>
                    </li>
                  {/each}
                </ul>
              </div>
              <div class="grid gap-0.5 border-t pt-2 @min-[560px]:grid-cols-2" role="group" aria-label="Where to get: lines">
                {#each cat.lines.filter((l) => l.key !== 'obtain.shops') as l (l.key)}
                  <SwitchRow id="line-{l.key}" size="sm" label={l.label} disabled={off} checked={lineOn(d, l.key)} onchange={(v) => setLine(l.key, v)} />
                {/each}
              </div>
              <p class="text-muted-foreground text-[12px]">A line with nothing to say is left out; an item with no source gets no block. Odds are estimates and show only in Detailed.</p>
            </Card.Content>
          </Card.Root>
          <Card.Root>
            <Card.Header>
              <Card.Title>Shops</Card.Title>
              <Card.Description>Shop stock is in no game file. Shop names come from UEX files: downloaded when you turn it on in Settings, or put there yourself. Names only, never prices: prices go stale between applies.</Card.Description>
            </Card.Header>
            <Card.Content class="grid gap-2">
              <SwitchRow
                id="obtain-shops"
                label="Add shops from my UEX files"
                hint="Off by default. Without the files the Buy line is left out."
                disabled={off}
                checked={d.obtain_shops && (!shopLine || lineOn(d, shopLine.key))}
                onchange={(v) =>
                  edit((p) => {
                    p.obtain_shops = v;
                    if (v && shopLine) p.lines_off = p.lines_off.filter((k) => k !== shopLine.key);
                  })}
              />
              <div class="bg-muted/40 grid gap-1.5 rounded-md border p-3 text-[12.5px]" role="status" aria-live="polite">
                {#if uexError}
                  <span class="text-destructive">{uexError}</span>
                {:else if !uex}
                  <Skeleton class="h-4 w-64" />
                {:else}
                  <span>
                    UEX folder: <span class="mono select-all break-all">{uex.uex_folder}</span>
                  </span>
                  <ul class="grid gap-0.5">
                    {#each uex.files as f (f.name)}
                      <li class="flex flex-wrap items-center gap-x-2">
                        {#if f.present}<CircleCheck class="text-success size-3" aria-hidden="true" />{:else}<Circle class="text-muted-foreground size-3" aria-hidden="true" />{/if}
                        <span class="mono">{f.name}</span>
                        <span class="text-muted-foreground">{f.present ? (fileAge(f.modified) ?? 'present') : 'missing'}</span>
                      </li>
                    {/each}
                  </ul>
                  <span class="text-muted-foreground">
                    {uex.shops_ready ? 'Both files are there; apply again after you replace them.' : uexOn ? 'Fetch now downloads both, then apply. The Explorer Sources card reads the same files.' : 'Turn on "Download UEX market data" in Settings, or put both files (the sc-gamedata fetch output) in this folder, then apply. The Explorer Sources card reads the same files.'}
                  </span>
                  <div class="flex flex-wrap gap-1.5">
                    {#if uexOn}
                      <Button variant="outline" size="sm" disabled={uexFetching} onclick={fetchUex}><Download class={cn(uexFetching && 'animate-pulse')} aria-hidden="true" /> {uexFetching ? 'Downloading…' : 'Fetch now'}</Button>
                    {:else}
                      <Button variant="outline" size="sm" onclick={uexSettings}>Open Network settings</Button>
                    {/if}
                    <Button variant="ghost" size="sm" onclick={() => uexSeq++}>Check again</Button>
                  </div>
                {/if}
              </div>
            </Card.Content>
          </Card.Root>
        {:else if cat}
          <Card.Root>
            <Card.Header>
              <Card.Title>{cat.label}</Card.Title>
              <Card.Description>{cat.blurb}</Card.Description>
            </Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow id="cat-on" label={SWITCH_LABEL[cat.id]} checked={isOn(d, cat.id)} onchange={(v) => edit((p) => (p[CAT_FIELD[cat.id]] = v))} />
              {#if cat.lines.length}
                <div class="mt-2 grid gap-0.5 border-t pt-2 @min-[560px]:grid-cols-2" role="group" aria-label={`${cat.label}: stat lines`}>
                  {#each cat.lines as l (l.key)}
                    <SwitchRow id="line-{l.key}" size="sm" label={l.label} disabled={!isOn(d, cat.id)} checked={lineOn(d, l.key)} onchange={(v) => setLine(l.key, v)} />
                  {/each}
                </div>
              {/if}
            </Card.Content>
          </Card.Root>
          {#if cat.id === 'missions'}
            {@const off = !d.missions}
            <div class="bg-muted/40 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border p-3 text-[12.5px]" role="note">
              <span>[BP], [BP Owned], [Owned], [Missing] and the blueprint list are under <span class="font-medium">Blueprints</span>.</span>
              <button type="button" class="text-primary text-[12px] underline-offset-2 hover:underline" onclick={() => jumpTab('blueprints')}>Open blueprint markers</button>
            </div>
            <Card.Root>
              <Card.Header><Card.Title>Mission titles</Card.Title><Card.Description>Tags after a contract's title in the mobiGlas list.</Card.Description></Card.Header>
              <Card.Content class="grid gap-1">
                <SwitchRow id="m-rep" size="sm" disabled={off} label="Reputation reward, such as [150 Rep]" checked={d.mission.title_reputation} onchange={(v) => edit((p) => (p.mission.title_reputation = v))} />
                <SwitchRow id="m-em" size="sm" disabled={off} label="Highlight title tags" hint="Wraps the tags (reputation and blueprint) in an EM tag so the game draws them in that style. Blue (EM4) by default." checked={d.mission.title_em} onchange={(v) => edit((p) => (p.mission.title_em = v))} />
                <div class="grid gap-2 pt-1 pl-11">
                  {@render emRow('title_rep_em', 'Reputation tag', off || !d.mission.title_em || !d.mission.title_reputation)}
                </div>
              </Card.Content>
            </Card.Root>
            <Card.Root>
              <Card.Header><Card.Title>Mission descriptions</Card.Title><Card.Description>Sections added below the contract text.</Card.Description></Card.Header>
              <Card.Content class="grid gap-1">
                <div class="grid gap-0.5 @min-[560px]:grid-cols-2" role="group" aria-label="Description sections">
                  <SwitchRow id="m-sec-items" size="sm" disabled={off} label="Item rewards" checked={d.mission.item_rewards} onchange={(v) => edit((p) => (p.mission.item_rewards = v))} />
                  <SwitchRow id="m-sec-details" size="sm" disabled={off} label="Mission details" checked={d.mission.details} onchange={(v) => edit((p) => (p.mission.details = v))} />
                </div>
              </Card.Content>
            </Card.Root>
            <Card.Root>
              <Card.Header>
                <Card.Title>Colours</Card.Title>
                <Card.Description>EM3 (underline) and EM4 (blue) are what the game uses in contracts. EM1, EM2 and EM5 exist but their look is not verified: try them with the Colour test under General. Blueprint colours are under Blueprints.</Card.Description>
              </Card.Header>
              <Card.Content class="grid gap-2 @min-[640px]:grid-cols-2">
                {@render emRow('header_em', 'Section headers', off)}
                {@render emRow('label_em', 'Detail labels', off || !d.mission.details)}
                {@render emRow('reward_em', 'Item reward names', off || !d.mission.item_rewards)}
              </Card.Content>
            </Card.Root>
          {/if}
        {:else if selected === 'blueprints'}
          {@const off = !d.missions}
          {@const pool = !off && d.mission.blueprints}
          {#if off}
            <div class="bg-muted/40 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md border p-3 text-[12.5px]" role="status">
              <span>Blueprint markers are part of the contract texts, and contracts are off for {app.target?.channel ?? 'this channel'}.</span>
              <Button variant="outline" size="sm" onclick={() => edit((p) => (p.missions = true))}>Turn on contracts</Button>
            </div>
          {/if}
          <Card.Root>
            <Card.Header>
              <Card.Title>[Owned] and [Missing]</Card.Title>
              <Card.Description>Marks on the blueprint lines of a contract, from the owned set of the character picked under Blueprints.</Card.Description>
              <Card.Action><Button variant="outline" size="sm" onclick={() => app.go('blueprints')}>Choose the character</Button></Card.Action>
            </Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow id="m-owned" size="sm" disabled={!pool} label="Mark blueprints you own with [Owned]" checked={d.mission.owned_marker} onchange={(v) => edit((p) => (p.mission.owned_marker = v))} />
              <div class="grid gap-1 pb-1 pl-11">
                {@render emRow('owned_em', '[Owned] colour', !pool || !d.mission.owned_marker)}
                <SwitchRow id="m-owned-line" size="sm" disabled={!pool || !d.mission.owned_marker || d.mission.owned_em === 0} label="Colour the whole line, not only [Owned]" hint={d.mission.owned_em === 0 ? 'Pick an [Owned] colour first.' : null} checked={d.mission.owned_line} onchange={(v) => edit((p) => (p.mission.owned_line = v))} />
              </div>
              <SwitchRow id="m-missing" size="sm" disabled={!pool} label="Mark blueprints you don't own with [Missing]" hint="Only when an owned set is known; without one nothing is marked." checked={d.mission.missing_marker} onchange={(v) => edit((p) => (p.mission.missing_marker = v))} />
              <div class="grid gap-1 pl-11">
                {@render emRow('missing_em', '[Missing] colour', !pool || !d.mission.missing_marker)}
              </div>
              {#if !off && !d.mission.blueprints}<p class="text-muted-foreground pt-1 text-[12px]">Turn on the blueprint list below first: the marks go on its lines.</p>{/if}
            </Card.Content>
          </Card.Root>
          <Card.Root>
            <Card.Header><Card.Title>Blueprint list</Card.Title><Card.Description>The BLUEPRINTS section below a contract's text: what it can reward.</Card.Description></Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow id="m-sec-bp" size="sm" disabled={off} label="Add the blueprint list to contracts" checked={d.mission.blueprints} onchange={(v) => edit((p) => (p.mission.blueprints = v))} />
              <SwitchRow id="m-bullets" size="sm" disabled={!pool} label="Name tags on blueprint lines" hint="The Tag Builder's tags, such as [IND-S2-A]." checked={d.mission.bullet_tags} onchange={(v) => edit((p) => (p.mission.bullet_tags = v))} />
              <div class="grid gap-2 pt-1">
                {@render emRow('variant_em', 'Variant labels, [Easy]', !pool)}
                <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
                  <label for="m-max" class="w-40 text-[12.5px]">Longest blueprint list</label>
                  <Input
                    id="m-max"
                    type="number"
                    min="1"
                    max="200"
                    class="mono h-8 w-20"
                    disabled={!pool}
                    value={String(d.mission.max_list)}
                    aria-describedby="m-max-hint"
                    oninput={(e) => {
                      const n = Math.round(Number((e.currentTarget as HTMLInputElement).value));
                      if (n >= 1 && n <= 200) edit((p) => (p.mission.max_list = n));
                    }}
                  />
                  <span id="m-max-hint" class="text-muted-foreground text-[12px]">Longer lists end with “and N more”.</span>
                </div>
              </div>
              <p class="text-muted-foreground pt-1 text-[12px]">The BLUEPRINTS header uses the section header colour under Missions.</p>
            </Card.Content>
          </Card.Root>
          <Card.Root>
            <Card.Header><Card.Title>In contract titles</Card.Title><Card.Description>Tags after a contract's title in the mobiGlas list.</Card.Description></Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow id="m-bp" size="sm" disabled={off} label="[BP] when the contract can reward a blueprint" hint="[BP?] when only some of its variants can." checked={d.mission.title_blueprint} onchange={(v) => edit((p) => (p.mission.title_blueprint = v))} />
              <SwitchRow id="m-bp-owned" size="sm" disabled={off || !d.mission.title_blueprint} label="[BP Owned] when you own every blueprint it can award" hint="In place of [BP] or [BP?]; uses the owned set from Blueprints." checked={d.mission.title_owned} onchange={(v) => edit((p) => (p.mission.title_owned = v))} />
              <div class="grid gap-1 pt-1 pl-11">
                {@render emRow('title_bp_em', '[BP] / [BP Owned] colour', off || !d.mission.title_em || !d.mission.title_blueprint)}
                {#if !off && !d.mission.title_em}
                  <p class="text-muted-foreground flex flex-wrap items-center gap-x-2 text-[12px]">
                    Title tags are not highlighted (Missions → Highlight title tags).
                    <button type="button" class="text-primary underline-offset-2 hover:underline" onclick={() => edit((p) => (p.mission.title_em = true))}>Highlight them</button>
                  </p>
                {/if}
              </div>
            </Card.Content>
          </Card.Root>
        {/if}
      </div>

      <div class="sticky top-0 hidden max-h-[calc(100dvh-9rem)] min-w-0 self-start overflow-y-auto @min-[880px]:block"><PreviewPanel items={preview} loading={previewing} section={selected} {sectionLabel} canRead={!!app.channel?.set_up} {reading} onread={readGameData} /></div>
    </div>
  </div>
{/if}
