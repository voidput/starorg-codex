// Headless renders of the Ships screen (addendum §6.6): empty, no game data, imported, grouped,
// import report, unmatched review, All ships, inline rename. Mock backend (?state=ready&fleet=...),
// dark and light, SIZES=WxH,... (default 1280x800 and 2560x1180). Never opens a window.
// Output: $RENDER_DIR/fleet (default ../../.tmp/renders/fleet).
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/fleet';
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const list = (p) => p.getByRole('listbox', { name: 'My ships, in order' });
const shots = [
  ['ships-empty', async (p) => p.getByText('Make your own ship list').waitFor(), ''],
  ['ships-nodata', async (p) => p.getByText(/Read the game data of LIVE first/).waitFor(), '&fleet=nodata'],
  ['ships-imported', async (p) => list(p).waitFor(), '&fleet=imported'],
  ['ships-grouped', async (p) => p.getByRole('option', { name: /^Group Combat/ }).waitFor(), '&fleet=grouped'],
  ['ships-import-report', async (p) => {
    await list(p).waitFor();
    await p.getByRole('button', { name: 'Import fleet' }).click();
    await p.getByRole('button', { name: 'Choose file and check' }).click();
    await p.getByText('Pick the right ship').waitFor();
  }, '&fleet=imported'],
  ['ships-review', async (p) => {
    await p.getByRole('button', { name: 'Review' }).click();
    await p.getByText('Match ships by hand').waitFor();
  }, '&fleet=review'],
  ['ships-all', async (p) => {
    await p.getByRole('tab', { name: 'All ships' }).click();
    await p.getByRole('list', { name: 'All ships of this build' }).waitFor();
  }, '&fleet=grouped'],
  ['ships-rename', async (p) => {
    const row = p.getByRole('option', { name: /Orbit Works Lantern|Rusty/ });
    await row.focus();
    await p.keyboard.press('F2');
    await p.getByRole('textbox', { name: /New name for/ }).waitFor();
  }, '&fleet=grouped'],
  ['ships-paste', async (p) => {
    await p.getByRole('button', { name: 'Paste a list' }).click();
    await p.getByText('One ship per line').waitFor();
  }, ''],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = (process.env.SIZES || '1280x800,2560x1180').split(',').map((s) => s.split('x').map(Number));
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, prep, extra] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=ships&theme=${theme}&delay=0${extra}`);
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
        await page.screenshot({ path: `${OUT}/${tag}.png` });
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
