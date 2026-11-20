<script lang="ts">
  /**
   * "3D print (experimental)" in the model export dialog (Print shell with an STL or 3MF):
   * every option of `PrintOptions`, off by default, each marked experimental. The options only
   * change the STL/3MF (the GLB stays as exported), except "No weapons". Options that need
   * another are off until it is on; turning that one off turns them off too. The bed size is
   * remembered per viewer (localStorage).
   */
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import FlaskConical from '@lucide/svelte/icons/flask-conical';
  import { Badge } from '@/components/ui/badge';
  import { Button } from '@/components/ui/button';
  import { Input } from '@/components/ui/input';
  import SwitchRow from '@/components/common/SwitchRow.svelte';
  import type { PrintOptions } from '@/lib/contract/assets';
  import { DEFAULT_DRAIN_MM, DEFAULT_DROP_SMALL_MM, DEFAULT_FIT_MM, DEFAULT_PEG_MM, DEFAULT_SIMPLIFY_TRIS, DEFAULT_THICKEN_MM, DEFAULT_WALL_MM, PEG_DEPTH_MM, PEG_MM, SOLID_RES_MM, printProblem, saveBed } from '@/lib/printoptions';

  let { value = $bindable() }: { value: PrintOptions } = $props();

  let open = $state(false);
  // The last value of each number, kept while its switch is off.
  let fitMm = $state(DEFAULT_FIT_MM);
  let ratio = $state(100);
  let dropMm = $state(DEFAULT_DROP_SMALL_MM);
  let thickenMm = $state(DEFAULT_THICKEN_MM);
  let wallMm = $state(DEFAULT_WALL_MM);
  let drainMm = $state(DEFAULT_DRAIN_MM);
  let pegMm = $state(DEFAULT_PEG_MM);
  let simplifyTris = $state(DEFAULT_SIMPLIFY_TRIS);

  const problem = $derived(printProblem(value));
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);

  function setScale(mode: string) {
    value.scale = mode === 'fit-mm' ? { mode, value: fitMm } : mode === 'ratio' ? { mode, value: ratio } : { mode: 'none' };
  }
  function setScaleValue(v: number) {
    if (value.scale.mode === 'fit-mm') value.scale = { mode: 'fit-mm', value: (fitMm = v) };
    else if (value.scale.mode === 'ratio') value.scale = { mode: 'ratio', value: (ratio = Math.round(v)) };
  }
  function setBed(i: number, v: number) {
    const b: [number, number, number] = [...value.bed_mm];
    b[i] = v;
    value.bed_mm = b;
    if (b.every((x) => Number.isFinite(x) && x > 0)) saveBed(b);
  }
  function setSolid(on: boolean) {
    value.solid = on;
    if (on && value.scale.mode === 'none' && !value.shrink_to_bed) setScale('fit-mm');
    if (!on) {
      value.thicken_mm = null;
      value.hollow = null;
      value.split = false;
      value.peg_mm = null;
    }
  }
  function setSplit(on: boolean) {
    value.split = on;
  }
  const on = $derived(
    [value.scale.mode !== 'none', value.shrink_to_bed, value.drop_small_mm !== null, value.solid, value.multicolor, value.simplify_tris !== null, value.no_weapons].filter(Boolean).length,
  );
</script>

{#snippet experimental()}
  <Badge variant="warning" class="shrink-0"><FlaskConical aria-hidden="true" /> Experimental</Badge>
{/snippet}

<div class="grid gap-1.5 rounded-md border p-2" data-print-options>
  <Button variant="ghost" size="xs" class="justify-start" aria-expanded={open} aria-controls="model-print" onclick={() => (open = !open)}>
    <ChevronDown aria-hidden="true" class={open ? 'rotate-180' : ''} /> 3D print (experimental){on ? ` · ${on} on` : ''}
    <span class="flex-1"></span>
    {@render experimental()}
  </Button>
  {#if open}
    <div id="model-print" class="grid gap-1.5 text-[12px]">
      <p class="text-muted-foreground">These change the STL/3MF only (the GLB stays as exported), except "No weapons". The model always sits on the bed. Results may need repair in the slicer.</p>
      <div class="flex flex-wrap items-center gap-2">
        <label class="flex items-center gap-2">
          <span>Size</span>
          <select class="border-input bg-background h-7 rounded-md border px-2" aria-label="Print size" value={value.scale.mode} onchange={(e) => setScale(e.currentTarget.value)} data-print-scale>
            <option value="none">Real size (metres)</option>
            <option value="fit-mm">Longest side (mm)</option>
            <option value="ratio">Scale 1:N</option>
          </select>
        </label>
        {#if value.scale.mode !== 'none'}
          <Input type="number" min="1" step={value.scale.mode === 'ratio' ? 1 : 0.5} class="h-7 w-24" aria-label={value.scale.mode === 'ratio' ? 'Scale ratio N (1:N)' : 'Longest side in mm'} value={value.scale.value} onchange={(e: Event) => setScaleValue(num(e))} />
        {/if}
        {@render experimental()}
      </div>
      <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Bed size in mm">
        <span>Bed (mm)</span>
        {#each ['X', 'Y', 'Z'] as axis, i (axis)}
          <Input type="number" min="1" class="h-7 w-20" aria-label={`Bed ${axis} in mm`} value={value.bed_mm[i]} onchange={(e: Event) => setBed(i, num(e))} />
        {/each}
        {@render experimental()}
      </div>
      <SwitchRow id="print-shrink" size="sm" label="Shrink to fit the bed" hint="Scale down when the model is larger than the bed (otherwise only a note)." checked={value.shrink_to_bed} onchange={(v) => (value.shrink_to_bed = v)}>
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-drop" size="sm" label="Leave out tiny parts" hint="Loose parts whose bounding-box diagonal is under this at print size (mm)." checked={value.drop_small_mm !== null} onchange={(v) => (value.drop_small_mm = v ? dropMm : null)}>
        {#if value.drop_small_mm !== null}<Input type="number" min="0.1" step="0.1" class="h-7 w-20" aria-label="Smallest part in mm" value={value.drop_small_mm} onchange={(e: Event) => (value.drop_small_mm = dropMm = num(e))} />{/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-solid" size="sm" label="Solid" hint="Rebuild as one closed solid from voxels (needs a size or Shrink to fit). Slow and memory hungry at 0.1 mm." checked={value.solid} onchange={setSolid}>
        {#if value.solid}
          <select class="border-input bg-background h-7 rounded-md border px-2" aria-label="Voxel size" value={value.solid_res_mm} onchange={(e) => (value.solid_res_mm = Number(e.currentTarget.value))}>
            {#each SOLID_RES_MM as r (r)}<option value={r}>{r} mm</option>{/each}
          </select>
        {/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-thicken" size="sm" label="Thicken thin parts" hint="Grow walls to at least this (mm). Needs Solid." checked={value.thicken_mm !== null} disabled={!value.solid} onchange={(v) => (value.thicken_mm = v ? thickenMm : null)}>
        {#if value.thicken_mm !== null}<Input type="number" min="0.1" step="0.1" class="h-7 w-20" aria-label="Thinnest wall in mm" value={value.thicken_mm} onchange={(e: Event) => (value.thicken_mm = thickenMm = num(e))} />{/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-hollow" size="sm" label="Hollow" hint="Walls this thick (mm) and a drain hole (mm, 0 = none) for resin. Needs Solid." checked={value.hollow !== null} disabled={!value.solid} onchange={(v) => (value.hollow = v ? { wall_mm: wallMm, drain_mm: drainMm } : null)}>
        {#if value.hollow}
          <Input type="number" min="0.1" step="0.1" class="h-7 w-16" aria-label="Wall in mm" value={value.hollow.wall_mm} onchange={(e: Event) => value.hollow && (value.hollow = { ...value.hollow, wall_mm: (wallMm = num(e)) })} />
          <Input type="number" min="0" step="0.5" class="h-7 w-16" aria-label="Drain hole in mm" value={value.hollow.drain_mm} onchange={(e: Event) => value.hollow && (value.hollow = { ...value.hollow, drain_mm: (drainMm = num(e)) })} />
        {/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-split" size="sm" label="Split to fit the bed" hint="Cut into pieces that each fit the bed (one STL per piece), with alignment pin holes across each cut. Needs Solid." checked={value.split} disabled={!value.solid} onchange={setSplit}>
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-peg" size="sm" label="Peg holes" hint={`Holes for pegs of this diameter, ${PEG_DEPTH_MM} mm deep each side of a cut, plus a stand hole underneath. Needs Solid.`} checked={value.peg_mm !== null} disabled={!value.solid} onchange={(v) => (value.peg_mm = v ? pegMm : null)}>
        {#if value.peg_mm !== null}
          <select class="border-input bg-background h-7 rounded-md border px-2" aria-label="Peg diameter" value={value.peg_mm} onchange={(e) => (value.peg_mm = pegMm = Number(e.currentTarget.value))}>
            {#each PEG_MM as p (p)}<option value={p}>{p} mm</option>{/each}
          </select>
        {/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-multicolor" size="sm" label="Multicolor" hint="One 3MF object per part group (hull, glass, ...), each with a colour, for multi-filament printers." checked={value.multicolor} onchange={(v) => (value.multicolor = v)}>
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-simplify" size="sm" label="Simplify" hint="Reduce to about this many triangles in all." checked={value.simplify_tris !== null} onchange={(v) => (value.simplify_tris = v ? simplifyTris : null)}>
        {#if value.simplify_tris !== null}<Input type="number" min="1000" step="10000" class="h-7 w-28" aria-label="Triangles" value={value.simplify_tris} onchange={(e: Event) => (value.simplify_tris = simplifyTris = Math.round(num(e)))} />{/if}
        {@render experimental()}
      </SwitchRow>
      <SwitchRow id="print-no-weapons" size="sm" label="No weapons" hint="Export the ship without its guns and missiles (changes the GLB too)." checked={value.no_weapons} onchange={(v) => (value.no_weapons = v)}>
        {@render experimental()}
      </SwitchRow>
      {#if problem}<p class="text-destructive" role="alert" data-print-problem>{problem}</p>{/if}
    </div>
  {:else if problem}
    <p class="text-destructive text-[12px]" role="alert" data-print-problem>{problem}</p>
  {/if}
</div>
