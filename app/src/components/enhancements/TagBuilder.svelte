<script lang="ts">
  /**
   * Tag Builder (spec 9.4 Enhancements) for one tag kind of M2's profile: the parts as chips
   * (ticked parts first, in output order, then the parts that are off), a style per part,
   * separator, brackets and placement. Reordering works by keyboard (the arrow buttons, or
   * Alt+Left / Alt+Right on a focused chip) and keeps focus on the moved chip.
   */
  import { tick } from 'svelte';
  import ChevronLeft from '@lucide/svelte/icons/chevron-left';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { Button } from '@/components/ui/button';
  import { Checkbox } from '@/components/ui/checkbox';
  import SelectField from '@/components/common/SelectField.svelte';
  import Segmented from '@/components/common/Segmented.svelte';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import type { TagEnclosing, TagKindSchema, TagKindSettings, TagSeparator, TagStyle } from '@/lib/contract/types';
  import { cn } from '@/lib/utils';

  let { schema, tag, onchange }: { schema: TagKindSchema; tag: TagKindSettings; onchange: (t: TagKindSettings) => void } = $props();

  const seps: { id: TagSeparator; label: string; ch: string }[] = [
    { id: 'dash', label: 'Dash  -', ch: '-' },
    { id: 'none', label: 'None', ch: '' },
    { id: 'space', label: 'Space', ch: ' ' },
    { id: 'underscore', label: 'Underscore  _', ch: '_' },
    { id: 'dot', label: 'Dot  .', ch: '.' },
    { id: 'slash', label: 'Slash  /', ch: '/' },
    { id: 'pipe', label: 'Pipe  |', ch: '|' },
  ];
  const encl: { id: TagEnclosing; label: string; lr: [string, string] }[] = [
    { id: 'square', label: '[ ]', lr: ['[', ']'] },
    { id: 'round', label: '( )', lr: ['(', ')'] },
    { id: 'curly', label: '{ }', lr: ['{', '}'] },
    { id: 'angle', label: '< >', lr: ['<', '>'] },
    { id: 'none', label: 'None', lr: ['', ''] },
  ];
  const styles: { id: TagStyle; label: string }[] = [
    { id: 'short', label: 'Short' },
    { id: 'medium', label: 'Medium' },
    { id: 'long', label: 'Long' },
  ];

  let list = $state<HTMLElement | null>(null);
  const label = (id: string) => schema.elements.find((e) => e.id === id)?.label ?? id;
  /** Ticked parts in output order, then the rest in schema order. */
  const chips = $derived([...tag.elements.filter((id) => schema.elements.some((e) => e.id === id)), ...schema.elements.map((e) => e.id).filter((id) => !tag.elements.includes(id))]);
  const gid = $derived(`tag-${schema.kind}`);

  function edit(f: (t: TagKindSettings) => void) {
    const t = structuredClone($state.snapshot(tag)) as TagKindSettings;
    f(t);
    onchange(t);
  }

  function setOn(id: string, on: boolean) {
    edit((t) => {
      t.elements = on ? [...t.elements.filter((x) => x !== id), id] : t.elements.filter((x) => x !== id);
    });
  }

  function setStyle(id: string, st: TagStyle) {
    edit((t) => {
      if (st === 'medium') delete t.styles[id];
      else t.styles[id] = st;
    });
  }

  async function move(id: string, d: -1 | 1) {
    const i = tag.elements.indexOf(id);
    const j = i + d;
    if (i < 0 || j < 0 || j >= tag.elements.length) return;
    edit((t) => {
      const [x] = t.elements.splice(i, 1);
      t.elements.splice(j, 0, x!);
    });
    await tick();
    list?.querySelector<HTMLElement>(`[data-chip="${id}"]`)?.focus();
  }

  function chipKey(e: KeyboardEvent, id: string) {
    if (!e.altKey) return;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      void move(id, -1);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      void move(id, 1);
    }
  }

  /** The shape of the tag, with placeholders, so the order is visible before the preview reloads. */
  const shape = $derived.by(() => {
    if (!tag.enabled) return 'No tag';
    const parts = tag.elements.map((id) => label(id).toUpperCase());
    if (!parts.length) return 'No tag';
    const sep = seps.find((s) => s.id === tag.separator)?.ch ?? '-';
    const [l, r] = encl.find((x) => x.id === tag.enclosing)?.lr ?? ['[', ']'];
    const t = `${l}${parts.join(sep)}${r}`;
    return tag.placement === 'prefix' ? `${t} Item name` : `Item name ${t}`;
  });
</script>

<fieldset class="grid gap-3 rounded-lg border p-3">
  <legend class="px-1 font-semibold">{schema.label}</legend>
  <SwitchRow id="{gid}-on" label={`Tag ${schema.label.toLowerCase()} in their names`} checked={tag.enabled} onchange={(v) => edit((t) => (t.enabled = v))} />
  <div class={cn('grid gap-3', !tag.enabled && 'opacity-60')}>
    <div>
      <div id="{gid}-parts" class="text-muted-foreground mb-1.5 text-[12px]">Parts, in order. Alt+Left / Alt+Right moves a focused part.</div>
      <ul bind:this={list} aria-labelledby="{gid}-parts" class="flex flex-wrap gap-2">
        {#each chips as id (id)}
          {@const on = tag.elements.includes(id)}
          {@const at = tag.elements.indexOf(id)}
          <li class={cn('bg-muted/40 flex items-center gap-1 rounded-md border py-1 pr-1 pl-2', !on && 'border-dashed')}>
            <Checkbox
              id="{gid}-{id}"
              data-chip={id}
              checked={on}
              disabled={!tag.enabled}
              aria-label={on ? `${label(id)}, part ${at + 1} of ${tag.elements.length}` : `${label(id)}, off`}
              onkeydown={(e: KeyboardEvent) => chipKey(e, id)}
              onCheckedChange={(v) => setOn(id, v === true)}
            />
            <label for="{gid}-{id}" class="cursor-pointer pr-1 text-[12.5px]">{label(id)}</label>
            <SelectField ariaLabel={`${label(id)} style`} options={styles} value={tag.styles[id] ?? 'medium'} disabled={!tag.enabled || !on} onchange={(v) => setStyle(id, v)} class="h-6 min-w-20 text-[12px]" />
            {#if on}
              <Button variant="ghost" size="icon-xs" aria-label={`Move ${label(id)} earlier`} disabled={!tag.enabled || at === 0} onclick={() => move(id, -1)}><ChevronLeft /></Button>
              <Button variant="ghost" size="icon-xs" aria-label={`Move ${label(id)} later`} disabled={!tag.enabled || at === tag.elements.length - 1} onclick={() => move(id, 1)}><ChevronRight /></Button>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
    <div class="flex flex-wrap items-end gap-x-4 gap-y-2">
      <div class="grid gap-1">
        <span class="text-muted-foreground text-[12px]">Separator</span>
        <SelectField ariaLabel="{schema.label} separator" options={seps} value={tag.separator} disabled={!tag.enabled} onchange={(v) => edit((t) => (t.separator = v))} />
      </div>
      <div class="grid gap-1">
        <span class="text-muted-foreground text-[12px]">Brackets</span>
        <SelectField ariaLabel="{schema.label} brackets" options={encl} value={tag.enclosing} disabled={!tag.enabled} onchange={(v) => edit((t) => (t.enclosing = v))} class="min-w-20" />
      </div>
      <div class="grid gap-1">
        <span class="text-muted-foreground text-[12px]">Placement</span>
        <Segmented
          size="sm"
          ariaLabel="{schema.label} placement"
          disabled={!tag.enabled}
          options={[{ id: 'prefix', label: 'Before name' }, { id: 'suffix', label: 'After name' }]}
          value={tag.placement}
          onchange={(v) => edit((t) => (t.placement = v as TagKindSettings['placement']))}
        />
      </div>
      <div class="grid min-w-0 gap-1">
        <span class="text-muted-foreground text-[12px]">Shape</span>
        <span class="mono bg-background/60 truncate rounded border px-2 py-1">{shape}</span>
      </div>
    </div>
  </div>
</fieldset>
