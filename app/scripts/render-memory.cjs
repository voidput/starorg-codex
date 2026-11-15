// Headless renders of the memory limit (addendum 7.18): the warning before Developer mode and
// the MCP server are turned on, Settings > Advanced > Memory limit (default and a small limit),
// the model export dialog's limit line and the Files reminder; mock backend, dark and light,
// at 1280x800 and 1024x700. Writes PNGs to .tmp/renders/memory/. Never opens a window.
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4317/ node scripts/render-memory.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/memory';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const center = (p, sel) => p.evaluate((s) => document.querySelector(s).scrollIntoView({ block: 'center' }), sel);
// [name, screen, extra query, prep]
const shots = [
  ['warning-dev', 'settings', '', async (p) => {
    await p.locator('#dev-mode').click();
    await p.getByRole('dialog').getByText(/This computer:/).waitFor();
  }],
  ['warning-mcp', 'settings', '&dev=1', async (p) => {
    await p.getByRole('switch', { name: /Local MCP server/ }).click();
    await p.getByRole('dialog').getByText(/This computer:/).waitFor();
  }],
  ['settings-limit', 'settings', '&dev=1', async (p) => {
    await p.locator('[data-memory-limit]').getByText('Memory limit').waitFor();
    await center(p, '[data-memory-limit]');
  }],
  ['settings-limit-small', 'settings', '&dev=1&memory=1.5', async (p) => {
    await p.locator('[data-memory-limit] [data-fits="false"]').first().waitFor();
    await center(p, '[data-memory-limit]');
  }],
  ['model-limit', 'explorer', '&dev=1', async (p) => {
    await p.getByText(/300 records/).waitFor();
    await p.getByRole('searchbox', { name: /Find records/ }).fill('SYN_Ship_003');
    await p.getByRole('option', { name: /SYN_Ship_003/ }).first().click();
    await p.getByRole('button', { name: 'Export model…' }).click();
    await p.getByRole('radio', { name: /Full/ }).click().catch(() => p.getByText('Full (textures)').click());
    await p.getByRole('dialog').getByText(/Memory limit 5 GB: this preset needs/).waitFor();
  }],
  ['files-reminder', 'files', '&dev=1', async (p) => p.getByText(/capped at 5 GB/).waitFor()],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1280, 800], [1024, 700]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, extra, prep] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0${extra}`);
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
