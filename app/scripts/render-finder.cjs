// Headless renders of the Explorer's item finder (addendum 7.17): Items (sections tree, list
// with stats, search with prefixes, grouping, facet sidebar, compare), Assets (models, sound
// events, a trigger), Raw (unchanged), and Files' "Open owning record". Mock backend, dark and
// light, at 1024x700 and 1280x800 (SIZES=WxH,... to change). Checks that a picked facet value
// gives the count it showed, that no pane grows the window and that nothing overflows.
// Writes PNGs to .tmp/renders/finder/. Never opens a window.
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4391/ node scripts/render-finder.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/finder';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4391/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const items = (p) => p.getByRole('listbox', { name: 'Items' });
const ready = async (p) => {
  await p.getByText(/357 records/).waitFor();
  await items(p).getByRole('option').first().waitFor();
};
/** Below 1,100 px the side pane is a drawer. */
const side = async (p) => {
  const b = p.getByRole('button', { name: /& filters/ });
  if ((await b.count()) && (await b.getAttribute('aria-expanded')) === 'false') await b.click();
};
const closeSide = async (p) => {
  if (await p.getByRole('button', { name: /& filters/ }).count()) await p.keyboard.press('Escape');
};
const search = (p) => p.getByRole('combobox', { name: /Find (items|sound events)/ });
const checks = [];

const shots = [
  ['items', 'explorer', async (p) => {
    await ready(p);
    await side(p);
    await p.getByRole('tree', { name: 'Sections' }).getByRole('treeitem', { name: /^Ships & vehicles,/ }).click();
    await closeSide(p);
    await p.getByText('Alpha Kestrel', { exact: true }).first().waitFor();
  }],
  ['items-record', 'explorer', async (p) => {
    await ready(p);
    await search(p).fill('kestrel');
    await items(p).getByRole('option', { name: /Alpha Kestrel/ }).first().click();
    await p.getByRole('heading', { name: /Alpha Kestrel|SYN_Alpha_Kestrel/ }).first().waitFor();
  }],
  ['items-search', 'explorer', async (p) => {
    await ready(p);
    await search(p).fill('type:cooler maker:alpha');
    await p.getByText('type:cooler', { exact: true }).waitFor();
    await items(p).getByText('Frost Cooler').waitFor();
  }],
  ['items-grouped', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('combobox', { name: /Group/ }).selectOption('section');
    await items(p).getByRole('option', { name: /^Ships & vehicles, \d+ records/ }).waitFor();
    // Fold a group: its rows leave, the header stays.
    await items(p).getByRole('option', { name: /^Ships & vehicles, \d+ records, open/ }).click();
    await items(p).getByRole('option', { name: /^Ships & vehicles, \d+ records, folded/ }).waitFor();
  }],
  ['items-filters', 'explorer', async (p) => {
    await ready(p);
    await side(p);
    await p.getByRole('radio', { name: /^Filters/ }).click();
    const box = p.getByRole('group', { name: 'Filters' });
    const row = box.locator('label', { hasText: 'Ship components' });
    const want = Number((await row.locator('span.mono').innerText()).replace(/,/g, ''));
    await row.getByRole('checkbox').check();
    await closeSide(p);
    await p.getByText(new RegExp(` of ${want}$`)).waitFor();
    checks.push(`facet count ${want} = rows ${want}`);
    await p.getByRole('button', { name: /^Player-usable/ }).click();
    await p.getByRole('button', { name: /Remove filter Player-usable only/ }).waitFor();
  }],
  ['items-compare', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('combobox', { name: 'Compare with channel' }).selectOption('PTU');
    await p.getByRole('button', { name: /^Changed vs PTU/ }).click();
    await items(p).getByText('Changed').first().waitFor();
  }],
  ['items-help', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('button', { name: 'Search help' }).click();
    await p.getByText('has:model · has:sounds').waitFor();
  }],
  ['assets', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: 'Assets' }).click();
    await side(p);
    await p.getByRole('tree', { name: /Models and sounds/ }).getByRole('treeitem', { name: /^Models,/ }).waitFor();
    await closeSide(p);
    await items(p).getByRole('option').first().waitFor();
  }],
  ['assets-sounds', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: 'Assets' }).click();
    await p.getByRole('radio', { name: 'Sound events' }).click();
    await search(p).fill('ship_001 engine');
    await p.getByRole('listbox', { name: 'Sound events' }).getByRole('option').first().click();
    await p.getByRole('button', { name: /^Play Play_SYN_Ship_001_Engine_Loop/ }).first().waitFor();
  }],
  ['raw', 'explorer', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: 'Raw' }).click();
    const b = p.getByRole('button', { name: 'Tree', exact: true });
    if ((await b.count()) && (await b.getAttribute('aria-expanded')) === 'false') await b.click();
    await p.getByRole('tree', { name: /DataCore/ }).getByRole('treeitem', { name: /^SynComponent,/ }).waitFor();
  }],
  ['files-owner', 'files', async (p) => {
    const files = p.getByRole('list', { name: 'Folder contents' });
    await files.getByRole('button', { name: /^Data/ }).click();
    await files.getByRole('button', { name: /^Objects/ }).click();
    await files.getByRole('button', { name: /^Spaceships/ }).click();
    await files.getByRole('button', { name: /^Ships/ }).click();
    await files.getByRole('button', { name: /^SYN/ }).click();
    await files.getByRole('button', { name: /^syn_ship_001/ }).first().click();
    await files.getByRole('button', { name: /^syn_ship_001\.skin / }).click();
    await p.getByRole('button', { name: 'Open owning record' }).click();
    await p.getByRole('heading', { name: /Alpha Kestrel|SYN_Alpha_Kestrel/ }).first().waitFor();
  }],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1024, 700], [1280, 800]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, prep] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        // Each shot starts from the defaults (no remembered mode, filters or presets).
        await page.addInitScript(() => {
          for (const k of Object.keys(localStorage)) if (k.startsWith('starorg.dev.finder')) localStorage.removeItem(k);
        });
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0&dev=1`);
        await page.waitForSelector('#page-title');
        try {
          await prep(page);
        } catch (e) {
          errors.push(`${tag}: prep failed: ${e.message.split('\n')[0]}`);
        }
        await page.waitForTimeout(300);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          return { doc: document.documentElement.scrollWidth - innerWidth, main: m ? m.scrollWidth - m.clientWidth : 0, mainH: m ? m.scrollHeight - m.clientHeight : 0 };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
        if (screen === 'explorer' && o.mainH > 2) overflow.push(`${tag}: main grows by ${o.mainH}px`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow');
  console.log(checks.length ? checks.join('\n') : 'no checks ran');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
