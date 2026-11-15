// Headless keyboard checks of the Developer mode screens (addendum 7.8): Alt+9 / Alt+0, the
// explorer tree keys, F6 / Shift+F6 between panes, the JSON tree keys and reference
// following, history (Alt+Left / Alt+Right), `/`, Ctrl+K "Find record…", the dataset grid and
// the diff lists. Mock backend (`?dev=1`). Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '?state=ready&delay=0&theme=dark';
const results = [];
const check = (name, ok, info = '') => results.push(`${ok ? 'PASS' : 'FAIL'} ${name}${info ? ` (${info})` : ''}`);

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const active = () =>
    page.evaluate(() => {
      const a = document.activeElement;
      const d = a?.getAttribute('aria-activedescendant');
      const cur = d ? document.getElementById(d) : null;
      return { role: a?.getAttribute('role'), label: a?.getAttribute('aria-label'), cur: cur?.getAttribute('aria-label') ?? cur?.textContent?.trim() ?? null, pane: a?.closest('[data-pane]')?.getAttribute('data-pane') ?? null };
    });
  // The h2's title is the raw record name; its text may lead with a readable name (7.14).
  const heading = () => page.locator('[data-pane=detail] h2').getAttribute('title');

  // Developer mode off: Alt+9 is Settings and the Dev group is not rendered.
  await page.goto(`${BASE}&screen=home`);
  await page.getByRole('heading', { name: 'LIVE' }).waitFor();
  await page.keyboard.press('Alt+9');
  check('dev off: Alt+9 opens Settings', (await page.locator('#page-title').textContent()).includes('Settings'));
  check('dev off: no Dev group', (await page.getByRole('button', { name: 'Explorer' }).count()) === 0);

  // On: Alt+9 Explorer, Alt+0 Settings.
  await page.goto(`${BASE}&screen=home&dev=1`);
  await page.getByRole('heading', { name: 'LIVE' }).waitFor();
  await page.keyboard.press('Alt+0');
  check('dev on: Alt+0 opens Settings', (await page.locator('#page-title').textContent()).includes('Settings'));
  await page.keyboard.press('Alt+9');
  await page.getByText(/300 records/).waitFor();
  check('dev on: Alt+9 opens Explorer', (await page.locator('#page-title').textContent()).includes('Explorer'));

  // Tree keys.
  const tree = page.getByRole('tree', { name: /DataCore/ });
  await tree.focus();
  check('tree starts on the first type', /^SynComponent/.test((await active()).cur ?? ''), JSON.stringify(await active()));
  await page.keyboard.press('ArrowDown');
  check('ArrowDown moves to SynShip', /^SynShip/.test((await active()).cur ?? ''));
  await page.keyboard.press('ArrowRight');
  await tree.getByRole('treeitem', { name: 'SYN_Ship_000' }).waitFor();
  check('ArrowRight opens the type', (await tree.getByRole('treeitem', { name: /^SynShip,/ }).getAttribute('aria-expanded')) === 'true');
  await page.keyboard.press('ArrowRight');
  check('ArrowRight again goes to the first record', (await active()).cur === 'SYN_Ship_000');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('[data-pane=detail] h2')?.title === 'SYN_Ship_001');
  check('Enter opens the record', (await heading()) === 'SYN_Ship_001');
  await page.keyboard.press('ArrowLeft');
  check('ArrowLeft goes to the parent type', /^SynShip/.test((await active()).cur ?? ''));
  await page.keyboard.press('ArrowLeft');
  check('ArrowLeft closes it', (await tree.getByRole('treeitem', { name: /^SynShip,/ }).getAttribute('aria-expanded')) === 'false');
  await page.keyboard.press('s');
  await page.keyboard.press('y');
  await page.keyboard.press('n');
  await page.keyboard.press('t');
  check('typeahead finds SynTag', /^SynTag/.test((await active()).cur ?? ''), JSON.stringify(await active()));

  // F6 cycles the panes: tree -> list -> record -> tree; Shift+F6 goes back.
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Enter');
  await page.getByRole('listbox', { name: 'Records' }).getByRole('option').first().waitFor();
  await tree.focus();
  await page.keyboard.press('F6');
  check('F6: tree -> list', (await active()).pane === 'list', JSON.stringify(await active()));
  await page.keyboard.press('F6');
  check('F6: list -> record', (await active()).pane === 'detail');
  await page.keyboard.press('F6');
  check('F6: record -> tree', (await active()).pane === 'tree');
  await page.keyboard.press('Shift+F6');
  check('Shift+F6: tree -> record', (await active()).pane === 'detail');

  // JSON tree: keys, open siblings, follow a reference, history.
  const json = page.locator('[data-pane=detail] [role=tree]');
  await json.focus();
  await page.keyboard.press('End');
  check('JSON End lands on the last line', /^Components/.test((await active()).cur ?? ''), JSON.stringify(await active()));
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  check('JSON Right opens, then goes to the first child', /reference to .*SYN_/.test((await active()).cur ?? ''), JSON.stringify(await active()));
  const before = await heading();
  await page.keyboard.press('Enter');
  await page.waitForFunction((b) => document.querySelector('[data-pane=detail] h2')?.title !== b, before);
  const followed = await heading();
  check('Enter on a reference follows it', /^SYN_(cooler|shield|powerplant|quantumdrive)_/.test(followed), followed);
  await page.keyboard.press('Alt+ArrowLeft');
  await page.waitForFunction((b) => document.querySelector('[data-pane=detail] h2')?.title === b, before);
  check('Alt+Left goes back', (await heading()) === before);
  await page.keyboard.press('Alt+ArrowRight');
  await page.waitForFunction((f) => document.querySelector('[data-pane=detail] h2')?.title === f, followed);
  check('Alt+Right goes forward', (await heading()) === followed);
  // `*` on _Type_ of a ship opens its sibling containers (Speed, Components).
  await page.keyboard.press('Alt+ArrowLeft');
  await page.waitForFunction((b) => document.querySelector('[data-pane=detail] h2')?.title === b, before);
  await json.focus();
  await page.keyboard.press('Home');
  for (let i = 0; i < 4; i++) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('*');
  const openCount = await json.locator('[role=treeitem][aria-expanded=true]').count();
  check('* opens all siblings', openCount >= 3, String(openCount));

  // `/` focuses the search; Escape returns to the list.
  await page.locator('[data-pane=list] [role=listbox]').focus();
  await page.keyboard.press('/');
  check('/ focuses the search', (await active()).label === 'Find records by name, path or GUID');
  await page.keyboard.type('ship_00');
  await page.locator('[data-pane=list]').getByRole('option', { name: /SYN_Ship_000/ }).waitFor();
  await page.keyboard.press('Escape');
  check('Escape returns to the list', (await active()).role === 'listbox');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => document.querySelector('[data-pane=detail] h2')?.title === 'SYN_Ship_001');
  check('list Enter opens the hit', (await heading()) === 'SYN_Ship_001');

  // Ctrl+K "Find record…" carries the typed text.
  await page.keyboard.press('Control+k');
  // Type once the palette's input has focus (typing earlier loses keys now and then).
  const cmd = page.getByRole('combobox', { name: 'Command' });
  await cmd.waitFor();
  await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Command');
  await cmd.fill('cooler 01');
  await page.getByRole('option', { name: /Find record “cooler 01”/ }).click();
  await page.getByText(/^2 of 2$/).waitFor();
  check('Ctrl+K Find record searches the explorer', (await page.getByRole('searchbox', { name: /Find records/ }).inputValue()) === 'cooler 01');

  // Datasets grid: arrows and Enter open the row.
  await page.goto(`${BASE}&screen=datasets&dev=1`);
  await page.getByRole('button', { name: /game-weapons.json/ }).click();
  await page.locator('[data-row="0"]').focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.getByText(/Open .* in Explorer/).first().waitFor({ state: 'attached' }).catch(() => {});
  // The title leads with the row's readable name (7.14): `shipWeapons · <name> (row 2)`.
  const sheet = page.getByRole('dialog', { name: /shipWeapons · .*row 2/ });
  check('datasets: Enter opens row 2', (await sheet.count()) === 1);
  await page.keyboard.press('Escape');

  // Diff: compare, then arrows + Enter in the record list.
  await page.goto(`${BASE}&screen=diff&dev=1`);
  await page.getByRole('combobox', { name: 'Source B' }).click();
  await page.getByRole('option', { name: 'PTU build' }).click();
  await page.getByRole('button', { name: 'Compare' }).click();
  const recs = page.getByRole('listbox', { name: 'Changed records' });
  await recs.waitFor();
  await recs.focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await page.getByRole('columnheader', { name: 'Δ' }).waitFor();
  check('diff: Enter shows the field diff of record 2', ((await page.locator('[aria-label="Field changes"] h2').textContent()) ?? '').includes('SYN_shield_009'));

  await browser.close();
  console.log(results.join('\n'));
  console.log(errors.length ? `page errors:\n${errors.join('\n')}` : 'no page errors');
  if (results.some((r) => r.startsWith('FAIL')) || errors.length) process.exit(1);
})().catch((e) => {
  console.error(results.join('\n'));
  console.error(e);
  process.exit(1);
});
