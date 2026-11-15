// Headless keyboard checks of the v1 screens (mock backend). Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '?state=ready&delay=0&theme=dark';
const results = [];
const check = (name, ok, info = '') => results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${info ? ` (${info})` : ''}`);

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const active = () => page.evaluate(() => {
    const a = document.activeElement;
    return { id: a?.id, row: a?.getAttribute('aria-rowindex'), label: a?.getAttribute('aria-label'), chip: a?.getAttribute('data-chip'), tag: a?.tagName };
  });

  // Alt+n navigation moves focus to <main>.
  await page.goto(`${BASE}&screen=home`);
  await page.getByRole('heading', { name: 'LIVE' }).waitFor();
  await page.keyboard.press('Alt+3');
  await page.getByRole('grid', { name: 'Strings' }).waitFor();
  check('Alt+3 opens Strings and focuses main', (await active()).id === 'main');

  // Strings grid: arrows, Page Down, End, Enter, Escape.
  await page.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
  await page.locator('[data-row="0"]').focus();
  for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowDown');
  check('ArrowDown x3 lands on row 4', (await active()).row === '5', JSON.stringify(await active()));
  await page.keyboard.press('PageDown');
  const afterPg = Number((await active()).row);
  check('PageDown moves a page', afterPg > 10, `row ${afterPg}`);
  await page.keyboard.press('End');
  await page.waitForTimeout(300);
  const end = await active();
  check('End reaches the last of 91,000 rows', end.row === String(91000 + 1), `row ${end.row}`);
  const rowsInDom = await page.locator('[role=row][data-row]').count();
  check('DOM holds only the visible window', rowsInDom < 60, `${rowsInDom} rows`);
  await page.locator('[data-row="90999"]').getByRole('gridcell').first().waitFor();
  const t0 = Date.now();
  await page.keyboard.press('Enter');
  await page.locator('#sd-value').waitFor();
  check('Enter opens the detail sheet', true, `${Date.now() - t0} ms incl. render`);
  await page.keyboard.press('Escape');
  await page.locator('#sd-value').waitFor({ state: 'detached' });
  check('Escape closes the sheet and focus returns to the row', (await active()).row === String(91000 + 1), JSON.stringify(await active()));

  // Tab order: scroll the active row out of the window with the wheel, then Tab back into the grid.
  await page.locator('[data-row="90999"]').focus();
  await page.evaluate(() => {
    const g = document.querySelector('[role=grid] [role=rowgroup]:last-child');
    g.scrollTop = 0;
    g.dispatchEvent(new Event('scroll'));
  });
  await page.waitForTimeout(200);
  await page.getByRole('searchbox', { name: 'Search keys and text' }).focus();
  let inGrid = false;
  for (let i = 0; i < 12 && !inGrid; i++) {
    await page.keyboard.press('Tab');
    inGrid = !!(await active()).row;
  }
  check('Tab reaches a row after the active one scrolled away', inGrid, JSON.stringify(await active()));

  // Override: add, reject, reset.
  await page.getByRole('searchbox', { name: 'Search keys and text' }).fill('Laser_S3_00002');
  await page.getByText(/^1 of /).waitFor();
  await page.locator('[data-row="0"]').click();
  await page.locator('#sd-value').fill('My laser');
  await page.getByRole('button', { name: 'Save edit' }).click();
  await page.getByRole('button', { name: 'Reset to built text' }).waitFor();
  check('Override saved', true);
  await page.locator('#sd-value').fill('My laser\nsecond');
  check('Save is off while a live check fails', await page.getByRole('button', { name: 'Save edit' }).isDisabled());
  await page.locator('#sd-value').fill('My laser ~ second');
  await page.getByRole('button', { name: 'Save edit' }).click();
  const rej = await page.getByRole('alert').filter({ hasText: 'Not saved' }).textContent();
  check('Rejected override names the rule in words', /Game variable missing/.test(rej ?? '') && /variable-tokens/.test(rej ?? ''), rej?.trim());
  await page.locator('#sd-value').fill('My laser ~ third');
  check('Rejection clears on input', (await page.getByRole('alert').filter({ hasText: 'Not saved' }).count()) === 0);
  await page.locator('#sd-value').fill('My laser');
  await page.getByRole('button', { name: 'Reset to built text' }).click();
  await page.getByRole('button', { name: /Remove edit\? Click again/ }).click();
  await page.getByRole('button', { name: 'Reset to built text' }).waitFor({ state: 'detached' });
  check('Override reset', true);
  await page.keyboard.press('Escape');
  await page.locator('#sd-value').waitFor({ state: 'detached' });

  // Tag builder: Alt+Right on a focused part moves it and keeps focus.
  await page.keyboard.press('Alt+2');
  await page.getByRole('tab', { name: /Name tags/ }).click();
  await page.getByText('Tag Builder').waitFor();
  const chip = page.locator('[data-chip="class"]').first();
  await chip.focus();
  await page.keyboard.press('Alt+ArrowRight');
  await page.waitForTimeout(100);
  const order = await page.locator('fieldset').first().locator('[data-chip]').evaluateAll((els) => els.map((e) => e.getAttribute('data-chip')));
  check('Alt+Right moves Class after Size', order.join() === 'size,class,grade', order.join());
  check('Focus stays on the moved part', (await active()).chip === 'class');
  check('Draft shows unsaved', await page.getByText('Unsaved changes').isVisible());
  // Category tabs with arrow keys.
  await page.getByRole('tab', { name: /Name tags/ }).focus();
  await page.keyboard.press('ArrowUp');
  check('ArrowUp in categories selects Missions', (await page.getByRole('tab', { selected: true }).textContent())?.includes('Missions'));

  // Unsaved draft: leaving asks (Save / Discard / Stay); a refresh keeps the draft.
  await page.getByRole('main').focus();
  await page.keyboard.press('Alt+3');
  await page.getByRole('heading', { name: 'Save your changes first?' }).waitFor();
  check('Leaving with unsaved edits asks first', true);
  await page.getByRole('button', { name: 'Stay' }).click();
  check('Stay keeps the screen and the draft', (await page.locator('#page-title').textContent()) === 'Enhancements' && (await page.getByText('Unsaved changes').isVisible()));
  await page.getByRole('radio', { name: /HOTFIX/ }).click();
  await page.getByRole('heading', { name: 'Save your changes first?' }).waitFor();
  check('Channel switch with unsaved edits asks first', true);
  await page.getByRole('button', { name: 'Stay' }).click();
  await page.getByRole('button', { name: /^Apply to LIVE/ }).click();
  await page.getByRole('heading', { name: 'Apply without your unsaved changes?' }).waitFor();
  check('Header Apply with unsaved edits offers Save & apply', await page.getByRole('dialog').getByRole('button', { name: 'Save & apply' }).isVisible());
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  await page.keyboard.press('Alt+1');
  await page.getByRole('dialog').getByRole('button', { name: 'Discard changes' }).click();
  await page.getByRole('heading', { name: 'LIVE' }).waitFor();
  check('Discard leaves the screen', (await page.locator('#page-title').textContent()) === 'Home');

  // Apply can be stopped while it reads game data.
  const slow = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await slow.goto(BASE.replace('delay=0', 'delay=700') + '&screen=home');
  await slow.getByRole('heading', { name: 'LIVE' }).waitFor();
  await slow.getByRole('button', { name: /^Apply to LIVE/ }).click();
  await slow.getByRole('button', { name: /^Stop:/ }).click();
  await slow.getByText('Stopped. Nothing was written to the game.').waitFor({ timeout: 8000 });
  check('Stop cancels the apply before it writes', true);
  await slow.close();

  // Game running: Apply is off and says why.
  const run = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  await run.goto(BASE + '&screen=backups&running=LIVE');
  await run.getByText('Timeline · LIVE').waitFor();
  check('Game running: header Apply off with a reason', (await run.getByRole('button', { name: /^Apply to LIVE/ }).isDisabled()) && (await run.locator('#apply-blocked').isVisible()));
  check('Game running: Backups actions off', await run.getByRole('button', { name: 'Undo last apply' }).isDisabled());
  await run.close();

  // Backups: Undo dialog focus trap + Escape.
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  await page.keyboard.press('Alt+7');
  await page.getByText('Timeline · LIVE').waitFor();
  await page.getByRole('button', { name: 'Undo last apply' }).click();
  await page.getByRole('heading', { name: /Undo the last apply/ }).waitFor();
  await page.waitForTimeout(250);
  const inDialog = await page.evaluate(() => !!document.activeElement?.closest('[role=dialog]'));
  check('Undo dialog takes focus', inDialog);
  await page.keyboard.press('Escape');

  console.log(results.join('\n'));
  console.log(errors.length ? 'page errors:\n' + errors.join('\n') : 'no page errors');
  await browser.close();
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
