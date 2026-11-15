// Headless renders of the Explorer's Model tab (addendum 7.20): an entity's Print shell in 3D with
// the detail slider, on the mock backend (`?dev=1`; the mock's GLB is a small synthetic ship),
// dark and light, at 1024x700 and 1280x800. WebGL through SwiftShader. Writes PNGs to
// .tmp/renders/model/. Never opens a window. Also checks that three.js (the ModelViewer chunk) is
// not fetched until the Model tab opens, and that nothing exports before "Load".
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4317/ node scripts/render-model.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/model';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const openShip = async (p, name) => {
  await p.getByText(/357 records/).waitFor();
  await p.getByRole('searchbox', { name: /Find records/ }).fill(name);
  await p.getByRole('option', { name: new RegExp(name) }).first().click();
  await p.getByRole('heading', { name }).waitFor();
};
const modelTab = async (p) => {
  await p.getByRole('tab', { name: 'Model' }).click();
  await p.locator('[data-model-viewer]').waitFor();
  await p.locator('[data-step="0"]').waitFor();
};
const load = async (p) => {
  await p.locator('[data-load]').click();
  await p.locator('[data-model-viewer][data-phase="ready"]').waitFor();
  await p.locator('[data-triangles]').waitFor();
};
const slide = (p, lod) => p.locator('#model-detail').fill(String(4 - lod));

const shots = [
  ['model-start', '', async (p) => {
    await openShip(p, 'SYN_Ship_003');
    await modelTab(p);
  }],
  ['model-lod3', '', async (p) => {
    await openShip(p, 'SYN_Ship_003');
    await modelTab(p);
    await load(p);
  }],
  ['model-lod0-wire', '', async (p) => {
    await openShip(p, 'SYN_Ship_003');
    await modelTab(p);
    await slide(p, 0);
    await load(p);
    await p.getByRole('switch', { name: 'Wireframe' }).click();
    await p.getByRole('switch', { name: 'Wireframe', checked: true }).waitFor();
  }],
  ['model-limit', '&memory=1.5', async (p) => {
    await openShip(p, 'SYN_Ship_003');
    await modelTab(p);
    await slide(p, 0);
    await p.locator('[data-step-note="refused"]').waitFor();
  }],
];

const GL = ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'];

(async () => {
  const browser = await chromium.launch({ headless: true, args: GL });
  const errors = [];
  const overflow = [];
  const stats = [];
  // three.js loads with the Model tab, not before; nothing is exported before "Load".
  {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    await ctx.addInitScript(() => localStorage.setItem('starorg.dev.finder.mode', '"raw"'));
    const page = await ctx.newPage();
    const chunks = [];
    page.on('request', (r) => /ModelViewer-.*\.js/.test(r.url()) && chunks.push(r.url()));
    await page.goto(`${BASE}?state=ready&screen=explorer&delay=0&dev=1`);
    await page.waitForSelector('#page-title');
    await openShip(page, 'SYN_Ship_001');
    if (chunks.length) errors.push('lazy: the viewer chunk loaded before the Model tab opened');
    await modelTab(page);
    if (!chunks.length) errors.push('lazy: no separate viewer chunk was fetched');
    if (await page.locator('[data-triangles]').count()) errors.push('lazy: a model showed before Load');
    await ctx.close();
  }
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1024, 700], [1280, 800]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      await ctx.addInitScript(() => localStorage.setItem('starorg.dev.finder.mode', '"raw"'));
      for (const [name, extra, prep] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(10000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=explorer&theme=${theme}&delay=0&dev=1${extra}`);
        await page.waitForSelector('#page-title');
        await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
        try {
          await prep(page);
        } catch (e) {
          errors.push(`${tag}: prep failed: ${e.message.split('\n')[0]}`);
        }
        await page.waitForTimeout(400);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          const tri = document.querySelector('[data-triangles]');
          const size = document.querySelector('[data-model-size]');
          const canvas = document.querySelector('[data-model-viewer] canvas');
          return {
            doc: document.documentElement.scrollWidth - innerWidth,
            main: m ? m.scrollWidth - m.clientWidth : 0,
            tri: tri?.textContent?.trim() ?? null,
            size: size?.textContent?.trim() ?? null,
            canvas: canvas ? `${canvas.width}x${canvas.height}` : null,
          };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
        if (o.tri) stats.push(`${tag}: ${o.size} · ${o.tri} · canvas ${o.canvas}`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(stats.join('\n'));
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow');
  if (errors.length) process.exitCode = 1;
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
