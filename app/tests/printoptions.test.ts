// The experimental 3D print options (src/lib/printoptions.ts) against the Rust side's
// defaults and checks (samples.gen.json, PrintOptions::validate). Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { defaultPrintOptions, isDefaultPrint, printProblem } from '../src/lib/printoptions.ts';
import samples from '../src/lib/contract/samples.gen.json' with { type: 'json' };
import type { PrintOptions } from '../src/lib/contract/assets.ts';

const rustDefault = samples.print_options_default as PrintOptions;
const example = samples.print_options_example as PrintOptions;

test('the defaults are the Rust defaults (f32 values to 1e-6)', () => {
  const d = defaultPrintOptions();
  assert.deepEqual({ ...d, solid_res_mm: 0 }, { ...rustDefault, solid_res_mm: 0 });
  assert.ok(Math.abs(d.solid_res_mm - rustDefault.solid_res_mm) < 1e-6);
  assert.ok(isDefaultPrint(d) && isDefaultPrint(rustDefault));
  assert.ok(!isDefaultPrint(example));
});

test('every option on is a valid combination, as in Rust', () => {
  assert.equal(printProblem(example), null);
  assert.equal(printProblem(defaultPrintOptions()), null);
  // Pegs without split: only the stand hole, as in Rust.
  assert.equal(printProblem({ ...defaultPrintOptions(), solid: true, scale: { mode: 'fit-mm', value: 150 }, peg_mm: 5 }), null);
});

test('options without the one they need are refused with the Rust wording', () => {
  const d = defaultPrintOptions();
  const fit = { mode: 'fit-mm', value: 150 } as const;
  const cases: [Partial<PrintOptions>, string][] = [
    [{ split: true }, '--split needs --solid (experimental)'],
    [{ thicken_mm: 0.8 }, '--thicken needs --solid (experimental)'],
    [{ hollow: { wall_mm: 2, drain_mm: 3 } }, '--hollow needs --solid (experimental)'],
    [{ solid: true }, '--solid needs --fit-mm, --scale-ratio or --shrink-to-bed (a real-size ship is metres long)'],
    [{ solid: true, scale: fit, split: true, peg_mm: 4 }, 'peg size 4 mm must be 3, 5 or 6'],
    [{ solid_res_mm: 0.3 }, 'solid resolution 0.3 mm must be 0.1, 0.2 or 0.4'],
    [{ scale: { mode: 'ratio', value: 0 } }, 'scale ratio must be 1 or more (1:N)'],
    [{ simplify_tris: 10 }, 'simplify to 10 triangles: at least 1000'],
    [{ drop_small_mm: 0 }, 'drop-small size 0 mm must be more than 0'],
  ];
  for (const [p, want] of cases) assert.equal(printProblem({ ...d, ...p }), want, JSON.stringify(p));
});

test('the remembered bed size reads back, bad or blocked storage gives the default', async () => {
  const { loadBed, parseBed, saveBed, BED_KEY } = await import('../src/lib/printoptions.ts');
  assert.deepEqual(parseBed('220x220x250'), [220, 220, 250]);
  assert.equal(parseBed('220x220'), null);
  assert.equal(parseBed('0x1x1'), null);
  assert.equal(parseBed(null), null);
  const g = globalThis as { localStorage?: unknown };
  const had = g.localStorage;
  const store = new Map<string, string>();
  g.localStorage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
  saveBed([180, 180, 180]);
  assert.equal(store.get(BED_KEY), '180x180x180');
  assert.deepEqual(loadBed(), [180, 180, 180]);
  g.localStorage = {
    getItem: () => {
      throw new Error('blocked');
    },
    setItem: () => {
      throw new Error('blocked');
    },
  };
  assert.deepEqual(loadBed(), [256, 256, 256]);
  saveBed([1, 1, 1]);
  g.localStorage = had;
});
