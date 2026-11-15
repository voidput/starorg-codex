<script lang="ts">
  /**
   * Tag Builder (spec 9.4 Enhancements): the parts of a name tag as reorderable chips, with style,
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
  import type { TagEnclosing, TagSeparator, TagSettings, TagStyle } from '@/lib/contract/types';
  import { cn } from '@/lib/utils';

  let { tag, disabled = false, onchange }: { tag: TagSettings; disabled?: boolean; onchange: (t: TagSettings) => void } = $props();

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

  function edit(f: (t: TagSettings) => void) {
    const t = structuredClone($state.snapshot(tag)) as TagSettings;
    f(t);
    onchange(t);
  }

  async function move(i: number, d: -1 | 1) {
    const j = i + d;
    if (j < 0 || j >= tag.elements.length) return;
    const id = tag.elements[i]!.id;
    edit((t) => {
      const [x] = t.elements.splice(i, 1);
      t.elements.splice(j, 0, x!);
    });
    await tick();
    list?.querySelector<HTMLElement>(`[data-chip="${id}"]`)?.focus();
  }

  function chipKey(e: KeyboardEvent, i: number) {
    if (!e.altKey) return;
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      void move(i, -1);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      void move(i, 1);
    }
  }

  /** The shape of the tag, with placeholders, so the order is visible before the preview reloads. */
  const shape = $derived.by(() => {
    const parts = tag.elements.filter((e) => e.enabled).map((e) => e.label.toUpperCase());
    if (!parts.length) return 'No tag';
    const sep = seps.find((s) => s.id === tag.separator)?.ch ?? '-';
    const [l, r] = encl.find((x) => x.id === tag.enclosing)?.lr ?? ['[', ']'];
    const t = `${l}${parts.join(sep)}${r}`;
    return tag.placement === 'prefix' ? `${t} Item name` : `Item name ${t}`;
  });
  const gid = $derived(`tag-${tag.kind}`);
</script>

<fieldset class={cn('grid gap-3 rounded-lg border p-3', disabled && 'opacity-60')} {disabled}>
  <legend class="px-1 font-semibold">{tag.label}</legend>
  <div>
    <div id="{gid}-parts" class="text-muted-foreground mb-1.5 text-[12px]">Parts, in order. Alt+Left / Alt+Right moves a focused part.</div>
    <ul bind:this={list} aria-labelledby="{gid}-parts" class="flex flex-wrap gap-2">
      {#each tag.elements as el, i (el.id)}
        <li class={cn('bg-muted/40 flex items-center gap-1 rounded-md border py-1 pr-1 pl-2', !el.enabled && 'border-dashed')}>
          <Checkbox
            id="{gid}-{el.id}"
            data-chip={el.id}
            checked={el.enabled}
            aria-label={`${el.label}, part ${i + 1} of ${tag.elements.length}`}
            onkeydown={(e: KeyboardEvent) => chipKey(e, i)}
            onCheckedChange={(v) => edit((t) => (t.elements[i]!.enabled = v === true))}
          />
          <label for="{gid}-{el.id}" class="cursor-pointer pr-1 text-[12.5px]">{el.label}</label>
          <SelectField ariaLabel={`${el.label} style`} options={styles} value={el.style} onchange={(v) => edit((t) => (t.elements[i]!.style = v))} class="h-6 min-w-20 text-[12px]" />
          <Button variant="ghost" size="icon-xs" aria-label={`Move ${el.label} earlier`} disabled={i === 0} onclick={() => move(i, -1)}><ChevronLeft /></Button>
          <Button variant="ghost" size="icon-xs" aria-label={`Move ${el.label} later`} disabled={i === tag.elements.length - 1} onclick={() => move(i, 1)}><ChevronRight /></Button>
        </li>
      {/each}
    </ul>
  </div>
  <div class="flex flex-wrap items-end gap-x-4 gap-y-2">
    <div class="grid gap-1">
      <span class="text-muted-foreground text-[12px]" id="{gid}-sep">Separator</span>
      <SelectField ariaLabel="{tag.label} separator" options={seps} value={tag.separator} onchange={(v) => edit((t) => (t.separator = v))} />
    </div>
    <div class="grid gap-1">
      <span class="text-muted-foreground text-[12px]">Brackets</span>
      <SelectField ariaLabel="{tag.label} brackets" options={encl} value={tag.enclosing} onchange={(v) => edit((t) => (t.enclosing = v))} class="min-w-20" />
    </div>
    <div class="grid gap-1">
      <span class="text-muted-foreground text-[12px]">Placement</span>
      <Segmented
        size="sm"
        ariaLabel="{tag.label} placement"
        options={[{ id: 'prefix', label: 'Before name' }, { id: 'suffix', label: 'After name' }]}
        value={tag.placement}
        onchange={(v) => edit((t) => (t.placement = v as TagSettings['placement']))}
      />
    </div>
    <div class="grid min-w-0 gap-1">
      <span class="text-muted-foreground text-[12px]">Shape</span>
      <span class="mono bg-background/60 truncate rounded border px-2 py-1" aria-live="polite">{shape}</span>
    </div>
  </div>
  <SwitchRow id="{gid}-short" size="sm" label="Also tag short names" hint="Short names show in tight HUD spots such as the power triangle list." checked={tag.short_name_too} onchange={(v) => edit((t) => (t.short_name_too = v))} />
</fieldset>
