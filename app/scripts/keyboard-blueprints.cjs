// Headless keyboard check of the Blueprints finder (addendum §7.19): `/` focuses the search,
// Escape clears it, facet chips toggle with Space/Enter (aria-pressed), the owned toggle moves
// with arrow keys, column headers sort from the keyboard (aria-sort), Clear filters, and the
// prefs survive a reload per channel. Mock backend. Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '?state=ready&delay=0&theme=dark&screen=blueprints';
const results = [];
const check = (name, ok, info = '') => results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${info ? ` (${info})` : ''}`);

(async () => {
  process.on('exit', () => console.log(results.join('\n')));
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.setDefaultTimeout(6000);
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const count = async () => Number((await page.locator('#bp-count').textContent()) ?? -1);
  await page.goto(BASE);
  await page.getByRole('search', { name: 'Find blueprints' }).waitFor();
  check('starts on Owned (LIVE: 8)', (await count()) === 8, String(await count()));

  await page.locator('body').click({ position: { x: 5, y: 790 } });
  await page.keyboard.press('/');
  check('/ focuses the search', await page.evaluate(() => document.activeElement?.getAttribute('aria-label') === 'Search blueprints'));
  await page.keyboard.type('rifle');
  check('search narrows', (await count()) === 1, String(await count()));
  await page.keyboard.press('Escape');
  check('Escape clears the search', (await page.getByRole('searchbox', { name: 'Search blueprints' }).inputValue()) === '' && (await count()) === 8);

  const armor = page.locator('[aria-labelledby="bp-f-kind"]').getByRole('button', { name: /^Armor/ });
  await armor.focus();
  await page.keyboard.press('Space');
  check('Space toggles a chip', (await armor.getAttribute('aria-pressed')) === 'true' && (await count()) === 3, String(await count()));
  await page.keyboard.press('Enter');
  check('Enter toggles it back', (await armor.getAttribute('aria-pressed')) === 'false' && (await count()) === 8);

  const owned = page.getByRole('radio', { name: /^Owned/ });
  await owned.focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Space');
  check('arrow keys reach Missing', (await page.getByRole('radio', { name: /^Missing/ }).getAttribute('aria-checked')) === 'true' && (await count()) === 29, String(await count()));

  const size = page.getByRole('columnheader', { name: /^Size/ });
  await size.getByRole('button').focus();
  await page.keyboard.press('Enter');
  check('header sorts ascending', (await size.getAttribute('aria-sort')) === 'ascending');
  await page.keyboard.press('Enter');
  check('again: descending', (await size.getAttribute('aria-sort')) === 'descending');

  await page.reload();
  await page.getByRole('search', { name: 'Find blueprints' }).waitFor();
  check('prefs remembered after reload', (await page.getByRole('columnheader', { name: /^Size/ }).getAttribute('aria-sort')) === 'descending' && (await count()) === 29);

  await page.getByRole('radio', { name: /^HOTFIX/ }).first().click();
  await page.getByRole('search', { name: 'Find blueprints' }).waitFor();
  await page.waitForTimeout(200);
  check('HOTFIX has its own prefs (Owned, 1)', (await page.getByRole('radio', { name: /^Owned/ }).getAttribute('aria-checked')) === 'true' && (await count()) === 1, String(await count()));
  await page.getByRole('radio', { name: /^LIVE/ }).first().click();
  await page.waitForTimeout(200);
  const clear = page.getByRole('button', { name: 'Clear filters' }).first();
  await clear.focus();
  await page.keyboard.press('Enter');
  check('Clear filters back to Owned, 8', (await count()) === 8 && (await page.getByRole('columnheader', { name: /^Size/ }).getAttribute('aria-sort')) === 'descending', String(await count()));

  await browser.close();
  if (errors.length) console.log(`errors:\n${errors.join('\n')}`);
  if (errors.length || results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})();
