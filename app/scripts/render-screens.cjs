// Headless renders of every v1 screen (mock backend, ?state=ready). Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/m3-screens';
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const shots = [
  ['home', 'home', async (p) => p.getByRole('heading', { name: 'LIVE' }).waitFor()],
  ['home-hotfix-stale', 'home', async (p) => {
    await p.getByRole('heading', { name: 'LIVE' }).waitFor();
    await p.getByRole('button', { name: 'Open HOTFIX' }).click();
    await p.getByText('The game was updated').waitFor();
  }],
  ['enhancements', 'enhancements', async (p) => p.getByText('Ship description').first().waitFor()],
  ['enhancements-tags', 'enhancements', async (p) => {
    await p.getByRole('tab', { name: /Name tags/ }).click();
    await p.getByText('Tag Builder').waitFor();
  }],
  ['enhancements-missions', 'enhancements', async (p) => {
    await p.getByRole('tab', { name: /Missions/ }).click();
    await p.getByText('Mission titles').waitFor();
  }],
  ['strings', 'strings', async (p) => p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor()],
  ['strings-filtered', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    await p.getByRole('button', { name: /^Edited/ }).click();
    await p.getByText(/^2 of /).waitFor();
  }],
  ['strings-detail-rejected', 'strings', async (p) => {
    await p.getByRole('searchbox', { name: 'Search keys and text' }).fill('Cooler_S2_00001');
    await p.getByText(/^1 of /).waitFor();
    await p.locator('[data-row="0"]').click();
    const ta = p.locator('#sd-value');
    await ta.waitFor();
    await ta.fill('My cooler <EM4>open');
    await p.getByRole('button', { name: 'Save edit' }).click();
    await p.getByText(/Not saved: rule/).waitFor();
  }],
  ['patches', 'patches', async (p) => p.getByText('Cargo grid typo').waitFor()],
  ['patches-detail', 'patches', async (p) => {
    await p.getByRole('button', { name: 'Details of Moved description key' }).click();
    await p.getByText('With the patch').first().waitFor();
  }],
  ['blueprints', 'blueprints', async (p) => p.getByText('Synthetic Armor Core').waitFor()],
  ['blueprints-import-report', 'blueprints', async (p) => {
    await p.getByRole('button', { name: 'Import from Smart Citizen' }).click();
    await p.getByRole('button', { name: 'Choose file and check' }).click();
    await p.getByText('Will be added to').waitFor();
  }],
  ['blueprints-starorg-connected', 'blueprints', async (p) => p.getByText(/Signed in as/).waitFor(), '&starorg=1'],
  ['backups', 'backups', async (p) => p.getByText('Timeline · LIVE').waitFor()],
  ['backups-foreign-restore', 'backups', async (p) => {
    await p.getByRole('button', { name: 'Restore it' }).click();
    await p.getByText('Made for another game build').last().waitFor();
  }],
  ['activity', 'activity', async (p) => p.getByText(/postexit: 2 new blueprints/).waitFor()],
  ['activity-diagnostics', 'activity', async (p) => {
    await p.getByRole('button', { name: 'Export diagnostics' }).click();
    await p.getByText(/personal values replaced/).waitFor();
  }],
  ['settings', 'settings', async (p) => p.getByText('Default channel').waitFor()],
  ['settings-starorg-hooks', 'settings', async (p) => {
    await p.getByText('Default channel').waitFor();
    await p.locator('#starorg').scrollIntoViewIfNeeded();
    await p.evaluate(() => document.getElementById('starorg').scrollIntoView({ block: 'start' }));
    await p.getByText('Characters to sync').waitFor();
  }, '&starorg=1'],
  ['settings-diagnostics', 'settings', async (p) => {
    await p.getByText('Default channel').waitFor();
    await p.getByRole('button', { name: 'Run doctor' }).click();
    await p.getByText('Smart Citizen stamp in LIVE global.ini').waitFor();
    await p.evaluate(() => document.getElementById('diagnostics').scrollIntoView({ block: 'start' }));
  }],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  for (const [w, h] of [[1280, 800], [1024, 700]]) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, prep, extra] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0${extra ?? ''}`);
        await page.waitForSelector('#page-title');
        try {
          await prep(page);
        } catch (e) {
          errors.push(`${tag}: prep failed: ${e.message.split('\n')[0]}`);
        }
        await page.waitForTimeout(350);
        // Horizontal overflow of the page or main column (responsive check).
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
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no horizontal overflow');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
