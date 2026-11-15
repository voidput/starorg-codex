// Headless renders of Settings -> StarOrg Tools (addendum §5): environment, sign-in state, What to
// sync, the environment-switch confirm and the Privacy dialog with the switches. Mock backend
// (?state=ready&screen=settings&starorg=1&starorgenv=dev&sync=all), dark and light,
// SIZES=WxH,... (default 1280x800). Never opens a window.
// Output: $RENDER_DIR/starorg (default ../../.tmp/renders/starorg).
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/starorg';
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const toCard = async (p) => {
  await p.getByText('Default channel').waitFor();
  await p.evaluate(() => document.getElementById('starorg').scrollIntoView({ block: 'start' }));
  await p.getByText('What to sync', { exact: true }).waitFor();
};
// [name, screen, prep, query, element to crop or null]
const shots = [
  ['starorg-off', 'settings', toCard, '', '#starorg'],
  ['starorg-signed-out-prod', 'settings', async (p) => {
    await toCard(p);
    await p.getByRole('switch', { name: /Connect StarOrg Tools/ }).click();
    await p.getByRole('button', { name: 'Sign in with Discord' }).waitFor();
  }, '', '#starorg'],
  ['starorg-dev-all-switches', 'settings', async (p) => {
    await toCard(p);
    await p.getByText('dev.starorg.tools').first().waitFor();
    await p.getByText('Signed in', { exact: true }).waitFor();
  }, '&starorg=1&starorgenv=dev&sync=all', '#starorg'],
  ['starorg-switch-confirm', 'settings', async (p) => {
    await toCard(p);
    await p.getByRole('radio', { name: 'Production' }).click();
    await p.getByText('Switch to starorg.tools?').waitFor();
  }, '&starorg=1&starorgenv=dev&sync=all', null],
  ['starorg-privacy', 'home', async (p) => {
    await p.getByRole('button', { name: 'Privacy' }).click();
    await p.getByText('Push owned blueprints').waitFor();
  }, '&starorg=1&starorgenv=dev&sync=all', null],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = (process.env.SIZES || '1280x800').split(',').map((s) => s.split('x').map(Number));
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, prep, extra, crop] of shots) {
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
        await page.waitForTimeout(350);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          const d = document.querySelector('[data-slot=dialog-content][data-state=open]');
          const r = d?.getBoundingClientRect();
          return {
            doc: document.documentElement.scrollWidth - innerWidth,
            main: m ? m.scrollWidth - m.clientWidth : 0,
            dialog: r ? Math.max(0, r.bottom - innerHeight, -r.top) : 0,
          };
        });
        if (o.doc > 0 || o.main > 0 || o.dialog > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main} dialog+${o.dialog}`);
        if (crop) await page.locator(crop).screenshot({ path: `${OUT}/${tag}.png` });
        else await page.screenshot({ path: `${OUT}/${tag}.png` });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow (page, main column, dialogs fit the window)');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
