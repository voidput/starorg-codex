// Headless renders of the Developer mode assets (addendum 7.16): the Files browser, a record's
// Sounds panel and the model export dialog in the Explorer, and the asset cache row in
// Settings > Advanced; mock backend (`?dev=1`), dark and light, at 1280x800 and 1000x700.
// Writes PNGs to .tmp/renders/assets/. Never opens a window. Also checks that opening the Dev
// record shows no sounds until its Sounds button is pressed (lazy loading; the real backend's
// tests check that no worker starts).
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4317/ node scripts/render-assets.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/assets';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const openShip = async (p, name) => {
  await p.getByText(/357 records/).waitFor();
  await p.getByRole('searchbox', { name: /Find records/ }).fill(name);
  await p.getByRole('option', { name: new RegExp(name) }).first().click();
  await p.getByRole('heading', { name }).waitFor();
};
const files = (p) => p.getByRole('list', { name: 'Folder contents' });

const shots = [
  ['files-root', 'files', async (p) => files(p).getByRole('button', { name: /^Data/ }).waitFor()],
  ['files-folder', 'files', async (p) => {
    await files(p).getByRole('button', { name: /^Data/ }).click();
    await files(p).getByRole('button', { name: /^Textures/ }).click();
    await files(p).getByRole('button', { name: /^ships/ }).click();
    await files(p).getByRole('button', { name: /^syn/ }).click();
    await files(p).getByRole('button', { name: /syn_ship_001_diff\.dds 174 B/ }).click();
    await p.getByRole('img', { name: /Texture syn_ship_001_diff.dds/ }).waitFor();
  }],
  ['files-text', 'files', async (p) => {
    await p.getByRole('textbox', { name: 'Search the game files' }).fill('defaultProfile');
    await p.keyboard.press('Enter');
    await files(p).getByRole('button', { name: /defaultProfile\.xml/ }).click();
    await p.getByText('<Config path=').waitFor();
  }],
  ['explorer-sounds', 'explorer', async (p) => {
    await openShip(p, 'SYN_Ship_002');
    await p.getByRole('button', { name: /^Sounds/ }).click();
    await p.getByText(/3 triggers/).waitFor();
    await p.getByRole('button', { name: /^Play Play_SYN_Ship_002_Engine_Loop 700020/ }).click();
    await p.getByRole('button', { name: /^Stop Play_SYN_Ship_002_Engine_Loop/ }).waitFor();
  }],
  ['explorer-model', 'explorer', async (p) => {
    await openShip(p, 'SYN_Ship_003');
    await p.getByRole('button', { name: 'Export model…' }).click();
    await p.getByRole('dialog').getByText(/For personal use/).waitFor();
  }],
  ['settings-asset-cache', 'settings', async (p) => {
    await p.getByText('Asset cache').first().waitFor();
    await p.getByRole('button', { name: 'Browse…' }).click();
    await p.getByText(/still holds|Asset cache/).first().waitFor();
    await p.evaluate(() => document.querySelector('[data-asset-cache]').scrollIntoView({ block: 'center' }));
  }],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  // Lazy loading: the Dev screens open without a single asset_* call.
  {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    await ctx.addInitScript(() => localStorage.setItem('starorg.dev.finder.mode', '"raw"')); // the Raw explorer (7.4); Items/Assets: render-finder.cjs
    const page = await ctx.newPage();
    await page.goto(`${BASE}?state=ready&screen=explorer&delay=0&dev=1`);
    await page.waitForSelector('#page-title');
    await openShip(page, 'SYN_Ship_001');
    // The record view shows only buttons; no sounds list until clicked.
    const listed = await page.getByRole('list', { name: 'Audio triggers' }).count();
    if (listed !== 0) errors.push('lazy: sounds listed before the Sounds button was pressed');
    await ctx.close();
  }
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1280, 800], [1000, 700]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      await ctx.addInitScript(() => localStorage.setItem('starorg.dev.finder.mode', '"raw"')); // the Raw explorer (7.4); Items/Assets: render-finder.cjs
      for (const [name, screen, prep] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
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
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          return { doc: document.documentElement.scrollWidth - innerWidth, main: m ? m.scrollWidth - m.clientWidth : 0 };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
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
