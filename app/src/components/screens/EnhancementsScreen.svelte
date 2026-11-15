<script lang="ts">
  /**
   * Enhancements (spec 9.4): category list, switches per stat line, format options, Tag Builder,
   * mission title options and a live preview. Settings are per channel; Save writes profile.toml,
   * Save & apply also applies, "Copy settings from" takes another channel's profile.
   */
  import Save from '@lucide/svelte/icons/save';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Copy from '@lucide/svelte/icons/copy';
  import Check from '@lucide/svelte/icons/check';
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
  import type { CategoryId, CategorySettings, PreviewItem, Preset, Profile } from '@/lib/contract/types';
  import { app } from '@/lib/stores/app.svelte';
  import { toast } from '@/lib/stores/toast.svelte';
  import { cn } from '@/lib/utils';

  const key = $derived(app.target ? `${app.target.install}/${app.target.channel}` : '');
  let saved = $state<string>('');
  let draft = $state<Profile | null>(null);
  let error = $state<ContractError | null>(null);
  let preview = $state<PreviewItem[] | null>(null);
  let previewing = $state(false);
  let selected = $state<CategoryId>('ships');
  let saving = $state(false);

  const dirty = $derived(draft !== null && JSON.stringify(draft) !== saved);
  const cat = $derived(draft?.categories.find((c) => c.id === selected) ?? null);
  const otherChannels = $derived(app.channels.filter((c) => c.set_up && c.channel !== app.channel?.channel).map((c) => ({ id: c.channel, label: c.channel })));

  $effect(() => {
    const k = key;
    if (!k || !app.target) return;
    const target = $state.snapshot(app.target);
    let stale = false;
    error = null;
    api
      .getProfile(target)
      .then((p) => {
        if (stale) return;
        draft = p;
        saved = JSON.stringify(p);
      })
      .catch((e) => !stale && (error = toContractError(e)));
    return () => (stale = true);
  });

  // Preview follows the draft (debounced); the backend renders it with the real generators.
  const draftJson = $derived(draft ? JSON.stringify(draft) : '');
  $effect(() => {
    const j = draftJson;
    if (!j || !app.target) return;
    const target = $state.snapshot(app.target);
    let stale = false;
    previewing = true;
    const t = setTimeout(() => {
      api
        .buildPreview({ target, profile: JSON.parse(j) as Profile, preset: null })
        .then((r) => !stale && (preview = r))
        .catch((e) => !stale && toast('error', 'Preview failed', toContractError(e).message))
        .finally(() => !stale && (previewing = false));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  });

  function edit(f: (p: Profile) => void) {
    if (!draft) return;
    const p = structuredClone($state.snapshot(draft)) as Profile;
    f(p);
    p.preset = null;
    draft = p;
  }
  const editCat = (f: (c: CategorySettings) => void) => edit((p) => f(p.categories.find((c) => c.id === selected)!));

  async function save(andApply: boolean) {
    if (!draft || !app.target) return;
    saving = true;
    try {
      const p = await api.saveProfile({ target: app.target, profile: $state.snapshot(draft) as Profile });
      draft = p;
      saved = JSON.stringify(p);
      toast('success', `Saved settings for ${app.target.channel}`, andApply ? 'Applying now.' : 'Apply to write them into the game.');
      await app.refresh();
      if (andApply) await app.apply();
    } catch (e) {
      toast('error', 'Settings not saved', toContractError(e).message);
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

  async function copyFrom(channel: string) {
    if (!app.target) return;
    try {
      const p = await api.copyProfile({ from: { install: app.target.install, channel }, to: app.target });
      draft = p;
      saved = JSON.stringify(p);
      toast('success', `Copied settings from ${channel}`, `Saved for ${app.target.channel}. Apply to write them into the game.`);
      await app.refresh();
    } catch (e) {
      toast('error', 'Copy failed', toContractError(e).message);
    }
  }

  function catKey(e: KeyboardEvent, i: number) {
    if (!draft) return;
    const n = draft.categories.length;
    const to = e.key === 'ArrowDown' ? (i + 1) % n : e.key === 'ArrowUp' ? (i - 1 + n) % n : e.key === 'Home' ? 0 : e.key === 'End' ? n - 1 : -1;
    if (to < 0) return;
    e.preventDefault();
    selected = draft.categories[to]!.id;
    (e.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLElement>('[role=tab]')[to]?.focus();
  }

  const presets: { id: Preset; label: string }[] = [
    { id: 'recommended', label: 'Recommended' },
    { id: 'minimal', label: 'Minimal' },
    { id: 'everything', label: 'Everything' },
  ];
  const blurb: Record<CategoryId, string> = {
    ships: 'A stats block in ship descriptions.',
    components: 'Stats in coolers, shields, power plants and quantum drives.',
    'ship-weapons': 'Damage, fire rate and range in ship weapon descriptions.',
    missiles: 'Damage, lock and speed in missile and bomb descriptions.',
    'fps-weapons': 'Damage, fire rate and magazine in personal weapon descriptions.',
    'name-tags': 'Short tags in item names, such as [IND-S2-A].',
    missions: 'Tags in contract titles and sections in their descriptions.',
    blueprints: 'Marks blueprints your active character owns.',
  };
</script>

{#if error}
  <ErrorState title="Could not read the enhancement settings" {error} onretry={() => app.refresh()} />
{:else if !draft || !cat}
  <div class="grid gap-3"><Skeleton class="h-10 w-full" /><Skeleton class="h-64 w-full" /></div>
{:else}
  <div class="grid gap-4">
    <div class="flex flex-wrap items-center gap-2" role="toolbar" aria-label="Enhancement settings">
      <span class="text-muted-foreground text-[12px]">Start from</span>
      <SelectField ariaLabel="Start from a preset" options={presets} value={draft.preset} placeholder="Custom" onchange={(v) => usePreset(v)} />
      {#if otherChannels.length}
        <SelectField ariaLabel="Copy settings from another channel" options={otherChannels} value={null} placeholder="Copy settings from…" onchange={(v) => copyFrom(v)} class="min-w-40" />
      {/if}
      <span class="flex-1"></span>
      <span class="text-muted-foreground text-[12px]" aria-live="polite">{dirty ? 'Unsaved changes' : 'Saved'}</span>
      <Button variant="ghost" size="sm" disabled={!dirty || saving} onclick={() => (draft = JSON.parse(saved) as Profile)}><Undo2 aria-hidden="true" /> Revert</Button>
      <Button variant="outline" size="sm" disabled={!dirty || saving} onclick={() => save(false)}><Save aria-hidden="true" /> Save</Button>
      <Button size="sm" disabled={saving || app.busy || !app.channel?.set_up} onclick={() => save(true)}><Check aria-hidden="true" /> Save &amp; apply</Button>
    </div>

    <div class="grid grid-cols-[168px_minmax(0,1fr)] gap-4 xl:grid-cols-[176px_minmax(0,1fr)_minmax(300px,360px)]">
      <div role="tablist" aria-orientation="vertical" aria-label="Categories" class="flex flex-col gap-0.5 self-start">
        {#each draft.categories as c, i (c.id)}
          <button
            type="button"
            role="tab"
            id="cat-tab-{c.id}"
            aria-selected={selected === c.id}
            aria-controls="cat-panel"
            tabindex={selected === c.id ? 0 : -1}
            class={cn(
              'focus-visible:ring-ring/60 flex h-8 items-center justify-between gap-2 rounded-md px-2.5 text-left outline-none focus-visible:ring-2',
              selected === c.id ? 'bg-accent text-foreground font-medium' : 'hover:bg-muted text-foreground/85',
            )}
            onclick={() => (selected = c.id)}
            onkeydown={(e) => catKey(e, i)}
          >
            <span class="truncate">{c.label}</span>
            <span class={cn('text-[11px]', c.enabled ? 'text-success' : 'text-muted-foreground')}>{c.enabled ? 'On' : 'Off'}</span>
          </button>
        {/each}
      </div>

      <div id="cat-panel" role="tabpanel" aria-labelledby="cat-tab-{cat.id}" class="grid min-w-0 gap-4 self-start">
        <Card.Root>
          <Card.Header>
            <Card.Title>{cat.label}</Card.Title>
            <Card.Description>{blurb[cat.id]}</Card.Description>
          </Card.Header>
          <Card.Content class="grid gap-1">
            <SwitchRow id="cat-on" label={`Add to ${cat.label.toLowerCase()}`} checked={cat.enabled} onchange={(v) => editCat((c) => (c.enabled = v))} />
            {#if cat.lines.length}
              <div class="mt-2 grid gap-0.5 border-t pt-2 sm:grid-cols-2" role="group" aria-label={`${cat.label} lines`}>
                {#each cat.lines as l, i (l.id)}
                  <SwitchRow id="line-{cat.id}-{l.id}" size="sm" label={l.label} disabled={!cat.enabled} checked={l.enabled} onchange={(v) => editCat((c) => (c.lines[i]!.enabled = v))} />
                {/each}
              </div>
            {/if}
            {#if cat.id === 'blueprints'}
              <p class="text-muted-foreground mt-1 text-[12px]">
                Uses the active character's owned list.
                <button type="button" class="text-primary underline-offset-2 hover:underline" onclick={() => app.go('blueprints')}>Choose the character</button>
              </p>
            {/if}
          </Card.Content>
        </Card.Root>

        {#if cat.id === 'name-tags'}
          <Card.Root>
            <Card.Header><Card.Title>Tag Builder</Card.Title><Card.Description>How tags are built for each kind of item.</Card.Description></Card.Header>
            <Card.Content class="grid gap-3">
              {#each draft.tags as t, i (t.kind)}
                <TagBuilder tag={t} disabled={!cat.enabled} onchange={(nt) => edit((p) => (p.tags[i] = nt))} />
              {/each}
            </Card.Content>
          </Card.Root>
        {:else if cat.id === 'missions'}
          <Card.Root>
            <Card.Header><Card.Title>Mission titles</Card.Title><Card.Description>Tags after a contract's title in the mobiGlas list.</Card.Description></Card.Header>
            <Card.Content class="grid gap-1">
              <SwitchRow id="m-bp" size="sm" disabled={!cat.enabled} label="[BP] when the contract can reward a blueprint" checked={draft.missions.tag_bp} onchange={(v) => edit((p) => (p.missions.tag_bp = v))} />
              <SwitchRow id="m-bp-maybe" size="sm" disabled={!cat.enabled || !draft.missions.tag_bp} label="[BP?] when only some variants can" checked={draft.missions.tag_bp_maybe} onchange={(v) => edit((p) => (p.missions.tag_bp_maybe = v))} />
              <SwitchRow id="m-rep" size="sm" disabled={!cat.enabled} label="Reputation reward, such as [150 Rep]" checked={draft.missions.tag_rep} onchange={(v) => edit((p) => (p.missions.tag_rep = v))} />
              <SwitchRow id="m-em4" size="sm" disabled={!cat.enabled} label="Highlight tags in colour" hint="Wraps tags in <EM4> so the game draws them in its highlight colour." checked={draft.missions.em4_title_tags} onchange={(v) => edit((p) => (p.missions.em4_title_tags = v))} />
              <SwitchRow id="m-owned" size="sm" disabled={!cat.enabled} label="Mark blueprints you own in the pool" checked={draft.missions.mark_owned} onchange={(v) => edit((p) => (p.missions.mark_owned = v))} />
              <div class="mt-2 border-t pt-2">
                <div class="text-muted-foreground mb-1 text-[12px]" id="m-body">Description sections</div>
                <div class="grid gap-0.5 sm:grid-cols-2" role="group" aria-labelledby="m-body">
                  {#each draft.missions.body_sections as b, i (b.id)}
                    <SwitchRow id="m-body-{b.id}" size="sm" disabled={!cat.enabled} label={b.label} checked={b.enabled} onchange={(v) => edit((p) => (p.missions.body_sections[i]!.enabled = v))} />
                  {/each}
                </div>
              </div>
            </Card.Content>
          </Card.Root>
        {/if}

        <Card.Root>
          <Card.Header><Card.Title>Format</Card.Title><Card.Description>Shared by every category that adds a stats block.</Card.Description></Card.Header>
          <Card.Content class="grid gap-3">
            <div class="flex flex-wrap items-center gap-3">
              <span class="w-28" id="fmt-place">Stats block</span>
              <Segmented
                ariaLabel="Stats block placement"
                options={[{ id: 'below-prose', label: 'Below the description' }, { id: 'above-prose', label: 'Above it' }]}
                value={draft.format.placement}
                onchange={(v) => edit((p) => (p.format.placement = v as Profile['format']['placement']))}
              />
            </div>
            <div class="flex flex-wrap items-center gap-3">
              <label for="fmt-header" class="w-28">Header line</label>
              <Input id="fmt-header" class="mono h-8 max-w-64" value={draft.format.header} oninput={(e) => edit((p) => (p.format.header = (e.currentTarget as HTMLInputElement).value))} />
            </div>
            <SwitchRow id="fix-bugs" label="Fix known CIG text bugs" hint="Built-in patches; each can be turned off under Patches." checked={draft.fix_known_bugs} onchange={(v) => edit((p) => (p.fix_known_bugs = v))} />
            <SwitchRow id="branding" label="Add a small StarOrg Strings stamp" hint="One line at the end of the main menu version text. Off by default." checked={draft.branding} onchange={(v) => edit((p) => (p.branding = v))} />
          </Card.Content>
        </Card.Root>
        <div class="xl:hidden"><PreviewPanel items={preview} loading={previewing} focus={cat.id === 'name-tags' ? 'components' : cat.id} /></div>
      </div>

      <div class="sticky top-0 hidden self-start xl:block"><PreviewPanel items={preview} loading={previewing} focus={cat.id === 'name-tags' ? 'components' : cat.id} /></div>
    </div>
  </div>
{/if}
