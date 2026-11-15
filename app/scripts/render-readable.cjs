// Headless renders of readable ids in Developer mode (addendum 7.12): Explorer record, Datasets
// table and row, Diff records and field changes, readable and with "Show IDs" on; mock backend
// (`?dev=1`), dark and light, 1280x800. Writes PNGs to .tmp/renders/readable/. Never opens a
// window. Run against `vite preview`: BASE_URL=http://127.0.0.1:4317/ node scripts/render-readable.cjs
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/readable';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
require('fs').mkdirSync(OUT, { recursive: true });

const openShip = async (p, name) => {
  await p.getByText(/300 records/).waitFor();
  await p.getByRole('searchbox', { name: /Find records/ }).fill(name);
  await p.getByRole('option', { name: new RegExp(name) }).click();
  await p.getByRole('heading', { name }).waitFor();
};
const diffRecords = async (p) => {
  await p.getByRole('combobox', { name: 'Source B' }).click();
  await p.getByRole('option', { name: 'PTU build' }).click();
  await p.getByRole('button', { name: 'Compare' }).click();
  await p.getByRole('listbox', { name: 'Changed records' }).getByRole('option').first().click();
  await p.getByRole('columnheader', { name: 'Δ' }).waitFor();
};

const shots = [
  ['explorer-record', 'explorer', (p) => openShip(p, 'SYN_Ship_002')],
  ['explorer-component', 'explorer', (p) => openShip(p, 'SYN_cooler_004')],
  ['datasets-table', 'datasets', async (p) => {
    await p.getByRole('button', { name: /gamedata.json/ }).first().click();
    await p.locator('[data-row="0"]').waitFor();
  }],
  ['datasets-search-name', 'datasets', async (p) => {
    await p.getByRole('button', { name: /gamedata.json/ }).first().click();
    await p.locator('[data-row="0"]').waitFor();
    await p.getByRole('searchbox', { name: 'Search the shown columns' }).fill('beta works');
    await p.getByText(/^8 rows|of 24/).first().waitFor().catch(() => {});
    await p.waitForTimeout(400);
  }],
  ['datasets-row', 'datasets', async (p) => {
    await p.getByRole('button', { name: /gamedata.json/ }).first().click();
    await p.locator('[data-row="0"]').click();
    await p.getByRole('tree', { name: 'row' }).waitFor();
  }],
  ['diff-records', 'diff', diffRecords],
  ['diff-ship', 'diff', async (p) => {
    await diffRecords(p);
    await p.getByRole('listbox', { name: 'Tables' }).getByRole('option', { name: /ships/ }).click();
    await p.getByRole('listbox', { name: 'Changed records' }).getByRole('option').first().click();
    await p.getByRole('columnheader', { name: 'Δ' }).waitFor();
  }],
  ['diff-filter', 'diff', async (p) => {
    await diffRecords(p);
    await p.getByRole('searchbox', { name: 'Filter records by name or id' }).fill('cooler 9');
    await p.waitForTimeout(500);
  }],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  for (const theme of ['dark', 'light']) {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, colorScheme: theme, reducedMotion: 'reduce' });
    for (const ids of [false, true]) {
      for (const [name, screen, prep] of shots) {
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}${ids ? '-ids' : ''}-${theme}-1280x800`;
        await page.addInitScript((on) => localStorage.setItem('starorg.dev.showIds', on ? '1' : '0'), ids);
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0&dev=1`);
        await page.waitForSelector('#page-title');
        try {
          await prep(page);
        } catch (e) {
          errors.push(`${tag}: prep failed: ${e.message.split('\n')[0]}`);
        }
        await page.waitForTimeout(300);
        const pressed = await page.getByRole('button', { name: 'Show IDs' }).first().getAttribute('aria-pressed').catch(() => null);
        if (pressed !== String(ids)) errors.push(`${tag}: Show IDs is ${pressed}`);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          return { doc: document.documentElement.scrollWidth - innerWidth, main: m ? m.scrollWidth - m.clientWidth : 0 };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        await page.close();
      }
    }
    await ctx.close();
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
