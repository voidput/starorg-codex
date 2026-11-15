// Headless renders and checks of the Datasets and Diff group trees (addendum 7.15), mock
// backend (`?dev=1`), dark and light, at 1280x800 and 1024x700. Writes PNGs to
// .tmp/renders/tree/ and fails when a check fails (picking a node filters the grid to the
// node's count; the Diff tree filters the records) or the page overflows. Never opens a window.
// Run against `vite preview`: BASE_URL=http://127.0.0.1:4318/ node scripts/render-tree.cjs [filter]
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/tree';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4318/';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

const fails = [];
const expect = (tag, what, ok) => ok || fails.push(`${tag}: ${what}`);
const groups = (p) => p.getByRole('tree', { name: /^Groups of / });

async function weapons(p) {
  await p.getByRole('button', { name: /gamedata.json/ }).first().click();
  await p.locator('[data-row="0"]').waitFor();
  await p.getByRole('combobox', { name: 'Table' }).click();
  await p.getByRole('option', { name: /^weapons \(/ }).click();
  await groups(p).getByRole('treeitem', { name: /^WeaponGun,/ }).waitFor();
}

const shots = [
  ['datasets-tree', 'datasets', async (p, tag) => {
    await weapons(p);
    const tree = groups(p);
    // Keyboard: focus the tree, open WeaponGun, then Gun, select Gun.
    await tree.getByRole('treeitem', { name: /^All rows,/ }).focus();
    await p.keyboard.press('ArrowDown');
    await p.keyboard.press('ArrowRight');
    await tree.getByRole('treeitem', { name: /^Gun,/ }).waitFor();
    await p.keyboard.press('ArrowRight');
    await p.keyboard.press('ArrowRight');
    await tree.getByRole('treeitem', { name: /^Size 1,/ }).waitFor();
    await p.keyboard.press('Enter');
    await p.getByText('5 / 10 rows').waitFor();
    expect(tag, 'Gun selected', (await tree.getByRole('treeitem', { name: /^Gun, 5 rows/ }).getAttribute('aria-selected')) === 'true');
    // Click a node: the grid shows its count.
    await tree.getByRole('treeitem', { name: /^WeaponMining, 2 rows/ }).click();
    await p.getByText('2 / 10 rows').waitFor();
    // Search narrows the counts; the structure stays.
    await p.getByRole('searchbox', { name: 'Search the shown columns' }).fill('Repeater 9');
    await tree.getByRole('treeitem', { name: /^WeaponGun, 0 rows/ }).waitFor();
    await p.getByText('1 / 10 rows').waitFor();
    await p.getByRole('searchbox', { name: 'Search the shown columns' }).fill('');
    await tree.getByRole('treeitem', { name: /^WeaponGun, 8 rows/ }).waitFor();
  }],
  ['datasets-tree-folders', 'datasets', async (p, tag) => {
    await weapons(p);
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'path folders' }).click();
    const tree = groups(p);
    await tree.getByRole('treeitem', { name: /^entities\/scitem\/ships\/weapons, 10 rows/ }).click();
    await p.getByText('10 / 10 rows').waitFor();
    await tree.getByRole('treeitem', { name: /^entities\/scitem\/ships\/weapons,/ }).press('ArrowRight');
    await tree.getByRole('treeitem', { name: /^mining, 2 rows/ }).click();
    await p.getByText('2 / 10 rows').waitFor();
    // Remembered per table: back to the default and in again.
    await p.reload();
    await p.waitForSelector('#page-title');
    await weapons(p).catch(() => {});
    expect(tag, 'group-by remembered', (await p.getByRole('combobox', { name: 'Group by' }).textContent()).includes('path folders'));
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: /^type › subType › size/ }).click();
    await groups(p).getByRole('treeitem', { name: /^WeaponGun,/ }).waitFor();
  }],
  ['datasets-tree-maker', 'datasets', async (p) => {
    await weapons(p);
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'manufacturer', exact: true }).click();
    // Readable labels (7.14): codes read as names; the node still filters by the code.
    await groups(p).getByRole('treeitem', { name: /^Beta Works, 5 rows/ }).click();
    await p.getByText('5 / 10 rows').waitFor();
    // The group-by is remembered per table: put the default back for the next shot.
    await p.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('starorg.dev.groupby.')).forEach((k) => localStorage.removeItem(k)));
  }],
  ['datasets-tree-railed', 'datasets', async (p, tag) => {
    await weapons(p);
    // F6 from the sources lands on the tree's item, not the pane's header button.
    await p.locator('[data-pane=sources] button').first().focus();
    await p.keyboard.press('F6');
    expect(tag, 'F6 lands on a tree item', (await p.evaluate(() => document.activeElement?.getAttribute('role'))) === 'treeitem');
    await p.getByRole('button', { name: 'Hide groups' }).click();
    await p.getByRole('button', { name: 'Show groups' }).waitFor();
  }, true],
  ['diff-tree', 'diff', async (p, tag) => {
    await p.getByRole('combobox', { name: 'Source B' }).click();
    await p.getByRole('option', { name: 'PTU build' }).click();
    await p.getByRole('button', { name: 'Compare' }).click();
    const tree = p.getByRole('tree', { name: 'Changed records by group' });
    await tree.getByRole('treeitem', { name: /^weapons, 4 records, 1 added, 1 removed, 2 changed/ }).click();
    await p.getByText(/^4 of 4$/).waitFor();
    await tree.getByRole('treeitem', { name: /^weapons,/ }).press('ArrowRight');
    await tree.getByRole('treeitem', { name: /^WeaponMining, 1 record,/ }).click();
    await p.getByText(/^1 of 1$/).waitFor();
    expect(tag, 'weapons table picked', (await p.getByRole('option', { name: /weapons/ }).first().getAttribute('aria-selected')) === 'true');
    await p.getByRole('listbox', { name: 'Changed records' }).getByRole('option').first().click();
    await p.getByRole('columnheader', { name: 'Δ' }).waitFor();
    // The field table fits its pane (no clipped delta column), tree open or not.
    const fit = await p.evaluate(() => {
      const t = document.querySelector('section[aria-label="Field changes"] table');
      return t ? t.parentElement.scrollWidth - t.parentElement.clientWidth : -1;
    });
    expect(tag, `field table fits (+${fit}px)`, fit === 0);
  }],
  ['diff-tree-match', 'diff', async (p, tag) => {
    await p.getByRole('combobox', { name: 'Source B' }).click();
    await p.getByRole('option', { name: 'PTU build' }).click();
    await p.getByRole('button', { name: 'Compare' }).click();
    const tree = p.getByRole('tree', { name: 'Changed records by group' });
    await tree.getByRole('treeitem', { name: /^weapons, 4 records/ }).click();
    // 'Match by' re-diffs the table: the tree and the Tables row follow at once.
    await p.getByRole('combobox', { name: 'Match records by' }).click();
    await p.getByRole('option', { name: 'Match by type' }).click();
    await p.getByText(/^5 of 5$/).waitFor();
    await tree.getByRole('treeitem', { name: /^weapons, 5 records, 5 changed/ }).waitFor();
    expect(tag, 'All changes follows', (await tree.getByRole('treeitem', { name: /^All changes, 13 records/ }).count()) === 1);
  }],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const errors = [];
  const overflow = [];
  const sizes = process.env.SIZES ? process.env.SIZES.split(',').map((s) => s.split('x').map(Number)) : [[1280, 800], [1024, 700]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, prep, keepRail] of shots) {
        if (only && !only.test(name)) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        const tag = `${name}-${theme}-${w}x${h}`;
        page.on('pageerror', (e) => errors.push(`${tag}: ${e.message}`));
        page.on('console', (m) => m.type() === 'error' && errors.push(`${tag} console: ${m.text()}`));
        await page.goto(`${BASE}?state=ready&screen=${screen}&theme=${theme}&delay=0&dev=1`);
        await page.waitForSelector('#page-title');
        try {
          await prep(page, tag);
        } catch (e) {
          errors.push(`${tag}: prep failed: ${e.message.split('\n')[0]}`);
        }
        await page.waitForTimeout(300);
        const o = await page.evaluate(() => {
          const m = document.getElementById('main');
          return { doc: document.documentElement.scrollWidth - innerWidth, main: m ? m.scrollWidth - m.clientWidth : 0, mainH: m ? m.scrollHeight - m.clientHeight : 0 };
        });
        if (o.doc > 0 || o.main > 0) overflow.push(`${tag}: doc+${o.doc} main+${o.main}`);
        if (o.mainH > 2) overflow.push(`${tag}: main grows by ${o.mainH}px`);
        await page.screenshot({ path: `${OUT}/${tag}.png` });
        // The pane state is per viewer: put it back for the next shot.
        if (keepRail) await page.evaluate(() => localStorage.removeItem('starorg.dev.tree.railed'));
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(errors.length ? errors.join('\n') : 'no page errors');
  console.log(overflow.length ? 'overflow:\n' + overflow.join('\n') : 'no overflow');
  console.log(fails.length ? 'failed checks:\n' + fails.join('\n') : 'all checks passed');
  if (errors.length || overflow.length || fails.length) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
