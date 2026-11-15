// Headless keyboard check of the Blueprints finder (addendum §7.19): `/` focuses the search,
// Escape clears it, facet chips toggle with Space/Enter (aria-pressed), the owned toggle moves
// with arrow keys, column headers sort from the keyboard (aria-sort), Clear filters, and the
// prefs survive a reload per channel; group headers and cards collapse by keyboard (Enter/Space,
// Left/Right, Up/Down between groups) and stay collapsed after a reload; "Edit blueprint markers"
// opens Enhancements on its Blueprints tab; `/` opens a collapsed finder card; Collapse all
// under a search merges; hidden matches are counted; the name notice stays in view. Mock backend. Never opens a window.
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
  check('starts on Owned (LIVE: 10)', (await count()) === 10, String(await count()));

  await page.locator('body').click({ position: { x: 5, y: 790 } });
  await page.keyboard.press('/');
  check('/ focuses the search', await page.evaluate(() => document.activeElement?.getAttribute('aria-label') === 'Search blueprints'));
  await page.keyboard.type('rifle');
  check('search narrows', (await count()) === 1, String(await count()));
  await page.keyboard.press('Escape');
  check('Escape clears the search', (await page.getByRole('searchbox', { name: 'Search blueprints' }).inputValue()) === '' && (await count()) === 10);

  const armor = page.locator('[aria-labelledby="bp-f-kind"]').getByRole('button', { name: /^Armor/ });
  await armor.focus();
  await page.keyboard.press('Space');
  check('Space toggles a chip', (await armor.getAttribute('aria-pressed')) === 'true' && (await count()) === 4, String(await count()));
  await page.keyboard.press('Enter');
  check('Enter toggles it back', (await armor.getAttribute('aria-pressed')) === 'false' && (await count()) === 10);

  const owned = page.getByRole('radio', { name: /^Owned/ });
  await owned.focus();
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Space');
  check('arrow keys reach Missing', (await page.getByRole('radio', { name: /^Missing/ }).getAttribute('aria-checked')) === 'true' && (await count()) === 30, String(await count()));

  const size = page.getByRole('columnheader', { name: /^Size/ });
  await size.getByRole('button').focus();
  await page.keyboard.press('Enter');
  check('header sorts ascending', (await size.getAttribute('aria-sort')) === 'ascending');
  await page.keyboard.press('Enter');
  check('again: descending', (await size.getAttribute('aria-sort')) === 'descending');

  await page.reload();
  await page.getByRole('search', { name: 'Find blueprints' }).waitFor();
  check('prefs remembered after reload', (await page.getByRole('columnheader', { name: /^Size/ }).getAttribute('aria-sort')) === 'descending' && (await count()) === 30);

  await page.getByRole('radio', { name: /^HOTFIX/ }).first().click();
  await page.getByRole('search', { name: 'Find blueprints' }).waitFor();
  await page.waitForTimeout(200);
  check('HOTFIX has its own prefs (Owned, 1)', (await page.getByRole('radio', { name: /^Owned/ }).getAttribute('aria-checked')) === 'true' && (await count()) === 1, String(await count()));
  await page.getByRole('radio', { name: /^LIVE/ }).first().click();
  await page.waitForTimeout(200);
  const clear = page.getByRole('button', { name: 'Clear filters' }).first();
  await clear.focus();
  await page.keyboard.press('Enter');
  check('Clear filters back to Owned, 10', (await count()) === 10 && (await page.getByRole('columnheader', { name: /^Size/ }).getAttribute('aria-sort')) === 'descending', String(await count()));

  // Groups: collapse by keyboard, remembered per channel + group-by.
  await page.getByRole('combobox', { name: 'Group by' }).click();
  await page.getByRole('option', { name: 'Category' }).click();
  const heads = page.locator('[data-group-toggle]');
  await heads.first().waitFor();
  const expanded = async (i) => heads.nth(i).getAttribute('aria-expanded');
  const rowsNow = async () => page.locator('#main tbody tr:not(.bg-muted\\/40)').count();
  const before = await rowsNow();
  await heads.first().focus();
  await page.keyboard.press('ArrowLeft');
  check('Left collapses a group', (await expanded(0)) === 'false' && (await rowsNow()) < before, `${before} -> ${await rowsNow()}`);
  await page.keyboard.press('ArrowLeft');
  check('Left again keeps it collapsed', (await expanded(0)) === 'false');
  await page.keyboard.press('ArrowDown');
  check('Down moves to the next group header', await page.evaluate(() => document.activeElement === document.querySelectorAll('[data-group-toggle]')[1]));
  await page.keyboard.press('Enter');
  check('Enter collapses it', (await expanded(1)) === 'false');
  await page.keyboard.press('Space');
  check('Space expands it again', (await expanded(1)) === 'true');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('ArrowRight');
  check('Up + Right expands the first', (await expanded(0)) === 'true');
  await page.keyboard.press('ArrowLeft');
  await page.reload();
  await heads.first().waitFor();
  check('collapsed group remembered after reload', (await expanded(0)) === 'false' && (await expanded(1)) === 'true');
  await page.getByRole('button', { name: 'Collapse all' }).click();
  const n = await heads.count();
  const states = async () => Promise.all([...Array(n).keys()].map(expanded));
  check('Collapse all', (await states()).every((x) => x === 'false') && (await rowsNow()) === 0, `${n} groups`);
  await page.getByRole('button', { name: 'Expand all' }).click();
  check('Expand all', (await states()).every((x) => x === 'true'));
  await page.getByRole('combobox', { name: 'Group by' }).click();
  await page.getByRole('option', { name: 'Manufacturer' }).click();
  await heads.first().waitFor();
  check('another group-by has its own state', (await expanded(0)) === 'true');

  // Collapse all under a search merges with what was collapsed outside it; matches only in
  // collapsed groups are counted, with an Expand link.
  const labelOf = async (h) => ((await h.textContent()) ?? '').replace(/\d+\s*(\(collapsed\))?\s*$/, '').trim();
  const search = page.getByRole('searchbox', { name: 'Search blueprints' });
  await search.fill('rifle');
  const rifleGroup = await labelOf(heads.first());
  await search.fill('');
  let label0 = '';
  for (let i = 0; i < (await heads.count()); i++) {
    const l = await labelOf(heads.nth(i));
    if (l !== rifleGroup) {
      label0 = l;
      await heads.nth(i).click();
      break;
    }
  }
  await search.fill('rifle');
  await page.getByRole('button', { name: 'Collapse all' }).click();
  const hint = page.getByText(/^1 match in collapsed groups/);
  check('one group: Collapse all shown, and the hidden match is counted', await hint.isVisible());
  await page.getByRole('button', { name: /^Expand it$/ }).click();
  check('Expand it shows the match', (await rowsNow()) === 1 && !(await hint.isVisible()), String(await rowsNow()));
  await search.fill('');
  const head0 = heads.filter({ hasText: label0 }).first();
  check('Collapse all under a search kept the group collapsed outside it', (await head0.getAttribute('aria-expanded')) === 'false', label0);
  await page.getByRole('button', { name: 'Expand all' }).click();
  await page.getByRole('combobox', { name: 'Group by' }).click();
  await page.getByRole('option', { name: 'No groups' }).click();

  // `/` with the finder card collapsed opens it and focuses the search.
  const finder = page.locator('#main [data-slot=card-toggle]', { hasText: /^\s*Blueprints\s*$/ });
  await finder.click();
  check('finder card collapsed', (await finder.getAttribute('aria-expanded')) === 'false' && !(await search.isVisible()));
  await page.locator('body').click({ position: { x: 5, y: 790 } });
  await page.keyboard.press('/');
  await page.waitForTimeout(100);
  check('/ expands the finder card and focuses the search', (await finder.getAttribute('aria-expanded')) === 'true' && (await page.evaluate(() => document.activeElement?.getAttribute('aria-label') === 'Search blueprints')));
  await page.keyboard.press('Escape');

  // Cards: the title is the toggle.
  const starorg = page.locator('#main [data-slot=card-toggle]', { hasText: /^\s*StarOrg Tools\s*$/ });
  await starorg.focus();
  await page.keyboard.press('Space');
  const pullHidden = async () => !(await page.getByRole('button', { name: 'Pull', exact: true }).isVisible());
  check('Space collapses a card', (await starorg.getAttribute('aria-expanded')) === 'false' && (await pullHidden()));
  await page.reload();
  await starorg.waitFor();
  check('collapsed card remembered after reload', (await starorg.getAttribute('aria-expanded')) === 'false' && (await pullHidden()));
  await starorg.focus();
  await page.keyboard.press('ArrowRight');
  check('Right expands it', (await starorg.getAttribute('aria-expanded')) === 'true' && !(await pullHidden()));

  // Markers in game -> Enhancements -> Blueprints.
  check('markers card shows the state', /\[Owned\][\s\S]*blue/.test((await page.locator('#bp-markers').textContent()) ?? ''));
  await page.getByRole('button', { name: 'Edit blueprint markers' }).focus();
  await page.keyboard.press('Enter');
  const tab = page.getByRole('tab', { name: /^Blueprints/ });
  await tab.waitFor();
  check('Edit blueprint markers opens Enhancements → Blueprints', (await tab.getAttribute('aria-selected')) === 'true' && (await page.getByRole('switch', { name: /^Mark blueprints you own/ }).count()) === 1);
  check('no [Owned] switch left under Missions', await (async () => {
    await page.getByRole('tab', { name: /^Missions/ }).click();
    return (await page.getByRole('switch', { name: /\[Owned\]|\[Missing\]|\[BP/ }).count()) === 0 && (await page.getByText('Open blueprint markers').isVisible());
  })());

  // "Two characters, one name" sits above the cards: visible with Fix owned blueprints collapsed.
  await page.goto(BASE + '&bpnotice=1');
  const fix = page.locator('#main [data-slot=card-toggle]', { hasText: /Fix owned blueprints/ });
  await fix.waitFor();
  if ((await fix.getAttribute('aria-expanded')) === 'true') await fix.click();
  check('name notice in view with Fix owned blueprints collapsed', (await fix.getAttribute('aria-expanded')) === 'false' && (await page.getByText('Two characters, one name').isVisible()));
  await fix.click();
  check('notice shown once (not also inside the card)', (await page.getByText('Two characters, one name').count()) === 1);

  await browser.close();
  if (errors.length) console.log(`errors:\n${errors.join('\n')}`);
  if (errors.length || results.some((r) => r.startsWith('FAIL'))) process.exit(1);
})();
