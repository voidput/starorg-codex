// Headless renders of the Developer mode screens (addendum 7.11) and the Strings generator
// filter, mock backend (`?dev=1`), dark and light, at 1280x800, 2560x1180 and 1000x700 (the
// explorer's drawer form). Writes PNGs to .tmp/renders/devmode/. Never opens a window.
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4317/ node scripts/render-dev.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/devmode';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const tree = (p) => p.getByRole('tree', { name: /DataCore/ });
/** Below 1,100 px the tree is a drawer: open it first. */
const openTree = async (p) => {
  await p.getByText(/357 records/).waitFor();
  const b = p.getByRole('button', { name: 'Tree', exact: true });
  if ((await b.count()) && (await b.getAttribute('aria-expanded')) === 'false') await b.click();
  await tree(p).waitFor();
};
const openShip = async (p, name) => {
  await openTree(p);
  await tree(p).getByRole('treeitem', { name: /^SynShip,/ }).click();
  // Close the drawer (narrow windows) so the record list is reachable.
  if (await p.getByRole('button', { name: 'Tree', exact: true }).count()) await p.keyboard.press('Escape');
  await p.getByRole('listbox', { name: 'Records' }).getByRole('option', { name: new RegExp(name) }).click();
  await p.getByRole('heading', { name }).waitFor();
};

const shots = [
  ['explorer', 'explorer', async (p) => {
    await openTree(p);
    await tree(p).getByRole('treeitem', { name: /^SynComponent,/ }).waitFor();
  }],
  ['explorer-record', 'explorer', async (p) => openShip(p, 'SYN_Ship_002')],
  ['explorer-search', 'explorer', async (p) => {
    await p.getByText(/357 records/).waitFor();
    await p.getByRole('searchbox', { name: /Find records/ }).fill('cooler 01');
    await p.getByText(/^2 of 2$/).waitFor();
    await p.getByRole('option', { name: /SYN_cooler_012/ }).click();
    await p.getByRole('heading', { name: 'SYN_cooler_012' }).waitFor();
  }],
  ['explorer-huge', 'explorer', async (p) => {
    await p.getByText(/357 records/).waitFor();
    await p.getByRole('searchbox', { name: /Find records/ }).fill('SYN_Ship_039');
    await p.getByRole('option', { name: /SYN_Ship_039/ }).click();
    await p.getByRole('heading', { name: 'SYN_Ship_039' }).waitFor();
    await p.getByText('Large record: deeper parts load when you open them.').waitFor();
  }],
  ['explorer-path', 'explorer', async (p) => {
    await openTree(p);
    await p.getByRole('radio', { name: 'By path' }).click();
    await tree(p).getByRole('treeitem', { name: /^entities,/ }).click();
    await tree(p).getByRole('treeitem', { name: /^scitem,/ }).waitFor();
  }],
  ['explorer-export-type', 'explorer', async (p) => {
    await openShip(p, 'SYN_Ship_001');
    await p.getByRole('button', { name: 'Export type…' }).click();
    await p.getByRole('dialog').getByText(/for your own use/).waitFor();
  }],
  ['explorer-stopped', 'explorer', async (p) => p.getByText('The explorer stopped').waitFor(), '&fail=dev_open'],
  ['datasets', 'datasets', async (p) => p.getByRole('button', { name: /gamedata.json/ }).first().waitFor()],
  ['datasets-build-table', 'datasets', async (p) => {
    await p.getByRole('button', { name: /gamedata.json/ }).first().click();
    await p.locator('[data-row="0"]').waitFor();
  }],
  ['datasets-folder-filter', 'datasets', async (p) => {
    await p.getByRole('button', { name: /game-weapons.json/ }).click();
    await p.locator('[data-row="0"]').waitFor();
    await p.getByRole('button', { name: /^Filter/ }).click();
    await p.getByRole('combobox', { name: 'Column to filter' }).click();
    await p.getByRole('option', { name: /^type \(text\)/ }).click();
    await p.getByRole('button', { name: /^Cannon/ }).click();
    await p.getByRole('button', { name: 'Remove filter type' }).waitFor();
  }],
  ['datasets-columns', 'datasets', async (p) => {
    await p.getByRole('button', { name: /game-weapons.json/ }).click();
    await p.locator('[data-row="0"]').waitFor();
    await p.getByRole('button', { name: /^Columns/ }).click();
    await p.getByRole('dialog', { name: 'Columns' }).waitFor();
  }],
  ['datasets-row', 'datasets', async (p) => {
    await p.getByRole('button', { name: /game-weapons.json/ }).click();
    await p.locator('[data-row="1"]').click();
    await p.getByText(/Open .* in Explorer/).first().waitFor();
  }, '', 'open'],
  ['diff', 'diff', async (p) => p.getByRole('button', { name: 'Compare' }).waitFor()],
  ['diff-records', 'diff', async (p) => {
    await p.getByRole('combobox', { name: 'Source B' }).click();
    await p.getByRole('option', { name: 'PTU build' }).click();
    await p.getByRole('button', { name: 'Compare' }).click();
    await p.getByRole('listbox', { name: 'Changed records' }).getByRole('option').first().click();
    await p.getByRole('columnheader', { name: 'Δ' }).waitFor();
  }],
  ['settings-advanced', 'settings', async (p) => {
    await p.getByText('Developer mode').first().waitFor();
    await p.evaluate(() => document.getElementById('advanced').scrollIntoView({ block: 'start' }));
  }],
  ['strings-generator', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    await p.getByRole('combobox', { name: 'Generator' }).click();
    await p.getByRole('option', { name: /^Components/ }).click();
    await p.getByText(/ of 91,000 keys/).waitFor();
  }, '', null],
  ['dev-off-falls-back', 'explorer', async (p) => p.getByRole('heading', { name: 'LIVE' }).waitFor(), '', null],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1280, 800], [2560, 1180], [1000, 700]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      await ctx.addInitScript(() => localStorage.setItem('starorg.dev.finder.mode', '"raw"')); // the Raw explorer (7.4); Items/Assets: render-finder.cjs
      for (const [name, screen, prep, extra = '', devMode = '1'] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && !/dev_open/.test(m.text()) && errors.push(`${tag} console: ${m.text()}`));
        const dev = devMode ? `&dev=${devMode}` : '';
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0${dev}${extra}`);
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
        // The Dev screens scroll inside their panes; <main> itself must not grow.
        if (['explorer', 'datasets', 'diff'].includes(screen) && devMode && o.mainH > 2) overflow.push(`${tag}: main grows by ${o.mainH}px`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
