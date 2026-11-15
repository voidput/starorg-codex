<script lang="ts">
  /**
   * Enhancements (spec 9.4): a list (General, the generator categories, Name tags), the switches of
   * M2's profile with labels from `profile_schema`, the Tag Builder, mission options and a live
   * preview. Settings are per channel; Save writes profile.toml, Save & apply also applies.
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
  import { api, toContractError, type ContractError } from '@/lib/contract/client';
  import type { CategoryId, ChannelRef, PatchView, PreviewItem, Preset, Profile, ProfileSchema, TagKind, TagKindSettings } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  type Tab = 'general' | CategoryId | 'name-tags';

  let schema = $state<ProfileSchema | null>(null);
  let saved = $state<string>('');
  let draft = $state<Profile | null>(null);
  let patches = $state<PatchView[]>([]);
  let error = $state<ContractError | null>(null);
  let preview = $state<PreviewItem[] | null>(null);
  let previewing = $state(false);
  let selected = $state<Tab>('general');
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
  $effect(() => {
    const j = draftJson;
    const k = app.targetKey;
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
      if (andApply) await app.apply();
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
  const CAT_FIELD: Record<CategoryId, 'ships' | 'components' | 'ship_weapons' | 'fps_weapons' | 'missiles' | 'missions'> = {
    ships: 'ships',
    components: 'components',
    'ship-weapons': 'ship_weapons',
    'fps-weapons': 'fps_weapons',
    missiles: 'missiles',
    missions: 'missions',
  };
  const SWITCH_LABEL: Record<CategoryId, string> = {
    ships: 'Add a stats block to ship descriptions',
    components: 'Add stats to component descriptions',
    'ship-weapons': 'Add stats to ship weapon descriptions',
    'fps-weapons': 'Add stats to personal weapon descriptions',
    missiles: 'Add stats to missile and bomb descriptions',
    missions: 'Add tags and sections to contracts',
  };
  const TAG_FIELD: Record<TagKind, 'components' | 'ship_weapons' | 'missiles'> = { components: 'components', 'ship-weapons': 'ship_weapons', missiles: 'missiles' };
  const isOn = (p: Profile, c: CategoryId) => p[CAT_FIELD[c]];
  const lineOn = (p: Profile, key: string) => !p.lines_off.includes(key);
  const setLine = (key: string, on: boolean) =>
    edit((p) => {
      p.lines_off = on ? p.lines_off.filter((k) => k !== key) : [...new Set([...p.lines_off, key])].sort();
    });

  // "Fix known text bugs" = no built-in patch in patches_disabled (spec 4.5; Patches lists each).
  const builtin = $derived(patches.filter((p) => p.origin === 'built-in').map((p) => p.id));
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
      ...schema.categories.map((c) => ({ id: c.id as Tab, label: c.label, on: isOn(d, c.id) })),
      { id: 'name-tags' as Tab, label: 'Name tags', on: d.tags.components.enabled || d.tags.ship_weapons.enabled || d.tags.missiles.enabled },
    ];
  });
  const cat = $derived(schema?.categories.find((c) => c.id === selected) ?? null);
  const previewFocus = $derived(selected === 'name-tags' || selected === 'general' ? 'components' : selected);

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
  const em = [
    { id: 3, label: 'Underline (EM3)' },
    { id: 4, label: 'Colour (EM4)' },
  ];
</script>

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

    <div class="grid grid-cols-[164px_minmax(0,1fr)] gap-4 @min-[880px]:grid-cols-[164px_minmax(0,1fr)_minmax(260px,330px)]">
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
        <div class="sticky top-0 z-10 @min-[880px]:hidden"><PreviewPanel items={preview} loading={previewing} focus={previewFocus} strip /></div>

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
              <SwitchRow id="branding" label="Add a small StarOrg Strings stamp" hint="One line at the end of the main menu version text. Off by default." checked={d.branding} onchange={(v) => edit((p) => (p.branding = v))} />
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
            <Card.Root>
              <Card.Header><Card.Title>Mission titles</Card.Title><Card.Description>Tags after a contract's title in the mobiGlas list.</Card.Description></Card.Header>
              <Card.Content class="grid gap-1">
                <SwitchRow id="m-bp" size="sm" disabled={off} label="[BP] when the contract can reward a blueprint" hint="[BP?] when only some of its variants can." checked={d.mission.title_blueprint} onchange={(v) => edit((p) => (p.mission.title_blueprint = v))} />
                <SwitchRow id="m-rep" size="sm" disabled={off} label="Reputation reward, such as [150 Rep]" checked={d.mission.title_reputation} onchange={(v) => edit((p) => (p.mission.title_reputation = v))} />
                <SwitchRow id="m-em" size="sm" disabled={off} label="Highlight title tags in colour" hint="Wraps the tags in <EM4> so the game draws them in its highlight colour." checked={d.mission.title_em} onchange={(v) => edit((p) => (p.mission.title_em = v))} />
              </Card.Content>
            </Card.Root>
            <Card.Root>
              <Card.Header><Card.Title>Mission descriptions</Card.Title><Card.Description>Sections added below the contract text.</Card.Description></Card.Header>
              <Card.Content class="grid gap-1">
                <div class="grid gap-0.5 @min-[560px]:grid-cols-2" role="group" aria-label="Description sections">
                  <SwitchRow id="m-sec-bp" size="sm" disabled={off} label="Blueprint pool" checked={d.mission.blueprints} onchange={(v) => edit((p) => (p.mission.blueprints = v))} />
                  <SwitchRow id="m-sec-items" size="sm" disabled={off} label="Item rewards" checked={d.mission.item_rewards} onchange={(v) => edit((p) => (p.mission.item_rewards = v))} />
                  <SwitchRow id="m-sec-details" size="sm" disabled={off} label="Mission details" checked={d.mission.details} onchange={(v) => edit((p) => (p.mission.details = v))} />
                  <SwitchRow id="m-bullets" size="sm" disabled={off || !d.mission.blueprints} label="Name tags on blueprint lines" checked={d.mission.bullet_tags} onchange={(v) => edit((p) => (p.mission.bullet_tags = v))} />
                </div>
                <SwitchRow id="m-owned" size="sm" disabled={off || !d.mission.blueprints} label="Mark blueprints you own with [Owned]" checked={d.mission.owned_marker} onchange={(v) => edit((p) => (p.mission.owned_marker = v))}>
                  <button type="button" class="text-primary text-[12px] underline-offset-2 hover:underline" onclick={() => app.go('blueprints')}>Choose the character</button>
                </SwitchRow>
                <div class="mt-1 flex flex-wrap items-center gap-3">
                  <label for="m-max" class="text-[12.5px]">Longest blueprint list</label>
                  <Input
                    id="m-max"
                    type="number"
                    min="1"
                    max="200"
                    class="mono h-8 w-20"
                    disabled={off}
                    value={String(d.mission.max_list)}
                    aria-describedby="m-max-hint"
                    oninput={(e) => {
                      const n = Math.round(Number((e.currentTarget as HTMLInputElement).value));
                      if (n >= 1 && n <= 200) edit((p) => (p.mission.max_list = n));
                    }}
                  />
                  <span id="m-max-hint" class="text-muted-foreground text-[12px]">Longer lists end with “and N more”.</span>
                </div>
                <div class="mt-2 grid gap-2 border-t pt-2">
                  <div class="flex flex-wrap items-center gap-3"><span class="w-36 text-[12.5px]">Section headers</span><Segmented size="sm" ariaLabel="Section header style" disabled={off} options={em} value={d.mission.header_em} onchange={(v) => edit((p) => (p.mission.header_em = v))} /></div>
                  <div class="flex flex-wrap items-center gap-3"><span class="w-36 text-[12.5px]">Detail labels</span><Segmented size="sm" ariaLabel="Detail label style" disabled={off} options={em} value={d.mission.label_em} onchange={(v) => edit((p) => (p.mission.label_em = v))} /></div>
                </div>
              </Card.Content>
            </Card.Root>
          {/if}
        {/if}
      </div>

      <div class="sticky top-0 hidden self-start @min-[880px]:block"><PreviewPanel items={preview} loading={previewing} focus={previewFocus} /></div>
    </div>
  </div>
{/if}
