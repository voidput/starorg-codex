// Headless keyboard check of the Ships screen (addendum §6.6): roving focus, Alt+Up/Down,
// Alt+Home/End, Alt+Shift+Up/Down, Space pick-up / Escape, F2 rename (empty = stock), Delete with
// Undo, the live announcements, Save, Alt+6 and the Ctrl+K entry. Mock backend. Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '?state=ready&delay=0&theme=dark';
const results = [];
const check = (name, ok, info = '') => results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${info ? ` (${info})` : ''}`);

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.setDefaultTimeout(6000);
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const order = () => page.$$eval('[role=listbox] [data-hull]', (els) => els.map((e) => e.getAttribute('data-hull').replace('vehicle_Name', '')));
  const focused = () => page.evaluate(() => document.activeElement?.getAttribute('data-hull')?.replace('vehicle_Name', '') ?? document.activeElement?.getAttribute('data-key') ?? document.activeElement?.tagName);
  const said = () => page.evaluate(() => [...document.querySelectorAll('[aria-live=polite]')].map((e) => e.textContent).find((t) => /position|picked|put back|removed|dropped|Renamed|stock name/.test(t ?? '')) ?? '');

  // Alt+6 and the palette reach the screen.
  await page.goto(`${BASE}&screen=home&fleet=imported`);
  await page.getByRole('heading', { name: 'LIVE' }).waitFor();
  await page.keyboard.press('Alt+6');
  await page.getByRole('listbox', { name: 'My ships, in order' }).waitFor();
  check('Alt+6 opens Ships', (await page.locator('#page-title').textContent()).includes('Ships'));
  await page.keyboard.press('Control+k');
  check('Ctrl+K lists "Go to Ships"', await page.getByText('Go to Ships').isVisible());
  await page.keyboard.press('Escape');

  const start = await order();
  check('imported order', start.join(',') === 'KSTL_Kite,ORBW_Lantern,RDGE_Hauler,VLMR_Pike', start.join(','));
  const tabStops = await page.$$eval('[role=listbox] [tabindex="0"]', (e) => e.length);
  check('the list is one tab stop', tabStops === 1, `${tabStops}`);
  await page.locator('[data-hull="vehicle_NameKSTL_Kite"]').focus();

  await page.keyboard.press('Alt+ArrowDown');
  check('Alt+Down moves one place', (await order())[1] === 'KSTL_Kite', (await order()).join(','));
  check('focus stays on the moved ship', (await focused()) === 'KSTL_Kite', await focused());
  check('move is announced', /Kestrel Kite, position 2 of 4/.test(await said()), await said());
  await page.keyboard.press('Alt+End');
  check('Alt+End: bottom', (await order())[3] === 'KSTL_Kite', (await order()).join(','));
  await page.keyboard.press('Alt+Home');
  check('Alt+Home: top', (await order())[0] === 'KSTL_Kite', (await order()).join(','));
  await page.keyboard.press('ArrowDown');
  check('ArrowDown moves the focus, not the ship', (await focused()) === 'ORBW_Lantern' && (await order())[0] === 'KSTL_Kite', await focused());

  // Space: pick up, arrows, Escape puts it back; Space drops.
  await page.keyboard.press('Space');
  check('Space picks up', /picked up/.test(await said()), await said());
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  check('arrows move the picked ship', (await order())[3] === 'ORBW_Lantern', (await order()).join(','));
  await page.keyboard.press('Escape');
  check('Escape puts it back', (await order())[1] === 'ORBW_Lantern' && /put back/.test(await said()), (await order()).join(','));
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Space');
  check('Space drops it', (await order())[2] === 'ORBW_Lantern' && /dropped/.test(await said()), (await order()).join(','));

  // F2 rename; Enter saves; empty = stock.
  await page.keyboard.press('F2');
  const box = page.getByRole('textbox', { name: /New name for Orbit Works Lantern/ });
  await box.waitFor();
  await box.fill('Rusty');
  await page.keyboard.press('Enter');
  const lantern = () => page.locator('[data-hull="vehicle_NameORBW_Lantern"]').getAttribute('aria-label');
  check('F2 + Enter renames', /^Rusty, prefix 03/.test(await lantern()), await lantern());
  check('preview shows the new name with its prefix', await page.getByRole('list', { name: /as the game would sort/ }).getByText('03 Rusty').isVisible());
  check('focus returns to the row', (await focused()) === 'ORBW_Lantern', await focused());
  await page.keyboard.press('F2');
  await box.fill('50% <off>');
  await page.keyboard.press('Enter');
  check('a bad name is refused inline', await page.getByRole('alert').filter({ hasText: 'Not allowed' }).isVisible());
  await page.keyboard.press('Escape');
  await page.keyboard.press('Enter');
  await box.fill('');
  await page.keyboard.press('Enter');
  check('empty = stock name', /^Orbit Works Lantern, prefix 03/.test(await lantern()), await lantern());

  // Delete with Undo.
  await page.keyboard.press('Delete');
  check('Delete removes', !(await order()).includes('ORBW_Lantern'), (await order()).join(','));
  await page.getByRole('button', { name: 'Undo' }).click();
  check('Undo brings it back', (await order()).includes('ORBW_Lantern'), (await order()).join(','));

  // Groups: a new group, then Alt+Shift+Down moves a ship into it.
  await page.getByRole('button', { name: 'New group' }).click();
  const gname = page.getByRole('textbox', { name: 'Group name' });
  await gname.waitFor();
  await gname.fill('Combat');
  await page.keyboard.press('Enter');
  await page.locator('[data-hull="vehicle_NameKSTL_Kite"]').focus();
  await page.keyboard.press('Alt+Shift+ArrowDown');
  check('Alt+Shift+Down: into the next group', /group Combat/.test(await said()), await said());
  await page.keyboard.press('Alt+Shift+ArrowUp');
  check('Alt+Shift+Up: back to no group', /no group/.test(await said()), await said());

  // Drag (pointer): drop Pike on Kite = before it.
  await page.locator('[data-hull="vehicle_NameVLMR_Pike"]').dragTo(page.locator('[data-hull="vehicle_NameKSTL_Kite"]'));
  check('drag and drop reorders', (await order())[0] === 'VLMR_Pike' || (await order()).indexOf('VLMR_Pike') < (await order()).indexOf('KSTL_Kite'), (await order()).join(','));

  // Save.
  check('edits are unsaved until Save', await page.getByText('Unsaved changes').isVisible());
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByText('Saved', { exact: true }).waitFor();
  check('Save stores the list', true);

  console.log(results.join('\n'));
  console.log(errors.length ? 'page errors:\n' + errors.join('\n') : 'no page errors');
  await browser.close();
  process.exit(results.some((r) => r.startsWith('FAIL')) ? 1 : 0);
})().catch((e) => {
  console.log(results.join('\n'));
  console.error(e);
  process.exit(1);
});
