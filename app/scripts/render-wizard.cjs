// Headless renders of the setup wizard and app frame (mock backend). Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/m3-wizard';
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '';
const steps = ['install', 'channels', 'source', 'enhancements', 'prefs', 'review', 'apply'];
(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  for (const theme of ['dark', 'light']) {
    const ctx = await browser.newContext({ viewport: { width: 1200, height: 780 }, colorScheme: theme, reducedMotion: 'reduce' });
    const shot = async (name, url, prep) => {
      const page = await ctx.newPage();
      page.on('pageerror', (e) => errors.push(`${name}/${theme}: ${e.message}`));
      page.on('console', (m) => m.type() === 'error' && errors.push(`${name}/${theme} console: ${m.text()}`));
      await page.goto(BASE + url + `&theme=${theme}&delay=0`);
      await page.waitForSelector('#wizard-title, #page-title');
      if (prep) await prep(page);
      await page.waitForTimeout(400);
      await page.screenshot({ path: `${OUT}/${name}-${theme}.png` });
      await page.close();
    };
    for (const [i, s] of steps.entries()) {
      await shot(`${i + 1}-${s}`, `?wizard=${s}`, async (p) => {
        if (s === 'apply') await p.getByRole('button', { name: 'Open StarOrg Codex' }).waitFor({ timeout: 10000 });
        if (s === 'review') await p.getByText('strings changed').first().waitFor();
        if (s === 'enhancements') await p.getByText('Ship description').waitFor();
      });
    }
    await shot('2b-channels-language-question', '?wizard=channels', async (p) => {
      await p.locator('#ch-PTU').click();
      await p.getByRole('button', { name: /^Next/ }).click();
    });
    await shot('8-close-guard', '?wizard=source', async (p) => {
      await p.getByRole('button', { name: /Set up later/ }).click();
      await p.getByRole('heading', { name: 'Leave setup for now?' }).waitFor();
    });
    await shot('9-home-frame', '?state=ready', async (p) => {
      await p.getByRole('heading', { name: 'LIVE' }).waitFor();
    });
    await ctx.close();
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
})().catch((e) => { console.error(e); process.exit(1); });
