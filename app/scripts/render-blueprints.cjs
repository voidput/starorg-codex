// Headless renders of the Blueprints screen's finder (addendum §7.19): one channel, All channels
// with channel badges, Missing for one character, grouped by category, a search, and the Home
// cards and channel picker with the shared channel colours; collapsed groups and cards, the
// Markers in game card, and Enhancements → Blueprints (reached from that card). Mock backend (?state=ready), dark and
// light, SIZES=WxH,... (default 1024x700,1280x800). Never opens a window.
// Output: $RENDER_DIR/blueprints (default ../../.tmp/renders/blueprints).
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/blueprints';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const ready = async (p) => p.getByRole('search', { name: 'Find blueprints' }).waitFor();
const chip = (p, facet, name) => p.locator(`[aria-labelledby="bp-f-${facet}"]`).getByRole('button', { name: new RegExp(`^${name}`) });
// [name, screen, prep, query]
const shots = [
  ['bp-live-owned', 'blueprints', ready, ''],
  ['bp-all-channels', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: 'All channels' }).click();
    await p.locator('#bp-f-channel').waitFor();
  }, ''],
  ['bp-missing-alt', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^Missing/ }).click();
    await chip(p, 'character', 'Alt_B').click();
  }, ''],
  ['bp-grouped-all', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^All \d/ }).click();
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'Category' }).click();
  }, ''],
  ['bp-search-sort', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^All \d/ }).click();
    await p.keyboard.press('/');
    await p.keyboard.type('cooler');
    await p.getByRole('button', { name: /^Size/ }).click();
    await p.getByRole('button', { name: /^Size/ }).click();
  }, ''],
  ['bp-ambiguous', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^All \d/ }).click();
    await p.keyboard.press('/');
    await p.keyboard.type('visor');
  }, ''],
  ['bp-groups-collapsed', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^All \d/ }).click();
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'Category' }).click();
    await p.locator('[data-group-toggle]').first().click();
    await p.locator('[data-group-toggle]').nth(2).click();
  }, ''],
  ['bp-cards-collapsed', 'blueprints', async (p) => {
    await ready(p);
    for (const t of ['Blueprints', 'StarOrg Tools']) await p.locator('#main [data-slot=card-toggle]', { hasText: new RegExp(`^\\s*${t}\\s*$`) }).click();
    await p.getByText('Markers in game').waitFor();
  }, ''],
  // The owner warning sits outside the cards: still in view with them collapsed.
  ['bp-hint-collapsed', 'blueprints', async (p) => {
    await ready(p);
    for (const t of ['Blueprints', 'Character for \\[Owned\\] tags']) await p.locator('#main [data-slot=card-toggle]', { hasText: new RegExp(`^\\s*${t}`) }).click();
    await p.getByText('Check whose blueprints are marked').waitFor({ state: 'visible' });
  }, '&bphint=larger'],
  ['bp-notice-collapsed', 'blueprints', async (p) => {
    await ready(p);
    await p.locator('#main [data-slot=card-toggle]', { hasText: /Fix owned blueprints/ }).click();
    await p.getByText('Two characters, one name').waitFor({ state: 'visible' });
    await p.evaluate(() => document.getElementById('main')?.scrollTo({ top: 0 }));
  }, '&bpnotice=1'],
  ['bp-hidden-matches', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('radio', { name: /^All \d/ }).click();
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'Category' }).click();
    await p.getByRole('button', { name: 'Collapse all' }).click();
    await p.getByRole('searchbox', { name: 'Search blueprints' }).fill('armor');
    await p.getByText(/in collapsed groups/).waitFor();
  }, ''],
  ['enh-blueprints', 'blueprints', async (p) => {
    await ready(p);
    await p.getByRole('button', { name: 'Edit blueprint markers' }).click();
    await p.getByRole('tab', { name: /^Blueprints/, selected: true }).waitFor();
  }, ''],
  ['enh-missions', 'enhancements', async (p) => {
    await p.getByRole('tab', { name: /^Missions/ }).click();
    await p.getByText('Open blueprint markers').waitFor();
  }, ''],
  ['home-channels', 'home', async (p) => p.getByText('Other channels').waitFor(), ''],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = (process.env.SIZES || '1024x700,1280x800').split(',').map((s) => s.split('x').map(Number));
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      for (const [name, screen, prep, extra] of shots) {
        if (only && !only.test(name)) continue;
        // A fresh context per shot: the finder remembers its filters in localStorage.
        const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
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
        // Since "dark is the default theme" the mock's saved appearance wins over ?theme=; pin it.
        await page.evaluate((t) => document.documentElement.setAttribute('data-theme', t), theme);
        await page.waitForTimeout(350);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          return { doc: document.documentElement.scrollWidth - innerWidth, main: m ? m.scrollWidth - m.clientWidth : 0 };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        if (screen === 'blueprints') {
          // The table, scrolled into view inside the main scroller.
          await page.evaluate(() => {
            const t = document.querySelector('#main table');
            const m = document.getElementById('main');
            if (t && m) m.scrollTop += t.getBoundingClientRect().top - m.getBoundingClientRect().top - 60;
          });
          await page.waitForTimeout(150);
          await page.screenshot({ path: `${OUT}/${tag}-table.png` });
        }
        await ctx.close();
      }
    }
  }
  await browser.close();
  console.log(`renders in ${OUT}`);
  if (overflow.length) console.log(`overflow:\n${overflow.join('\n')}`);
  if (errors.length) {
    console.log(`errors:\n${errors.join('\n')}`);
    process.exit(1);
  }
})();
