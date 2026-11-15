// Headless renders of every v1 screen and the main states (mock backend, ?state=ready), at
// 1280x800, the default window 1200x780 and 1024x700, dark and light. Never opens a window.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/m3-screens';
const BASE = (process.env.BASE_URL || 'http://127.0.0.1:4317/') + '';
const only = process.argv[2] ? new RegExp(process.argv[2]) : null;
require('fs').mkdirSync(OUT, { recursive: true });

/** Scroll <main> (not the document) so Settings -> Network is at the top. */
const toNetwork = () => {
  const m = document.getElementById('main');
  m.scrollTop += document.getElementById('network').getBoundingClientRect().top - m.getBoundingClientRect().top - 12;
};

const shots = [
  ['home', 'home', async (p) => p.getByRole('heading', { name: 'LIVE' }).waitFor()],
  ['home-hotfix-stale', 'home', async (p) => {
    await p.getByRole('heading', { name: 'LIVE' }).waitFor();
    await p.getByRole('button', { name: 'Open HOTFIX' }).click();
    await p.getByText('The game was updated').waitFor();
  }],
  ['enhancements', 'enhancements', async (p) => p.getByText('Header line').waitFor()],
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
  // Strings groups (addendum 8.5): tree open with a selection, rail, manufacturer mode, the
  // approximate state while the game data is read, and the narrow popover (below 1,000 px).
  ['strings-groups', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    const tree = p.getByRole('tree', { name: 'Groups' });
    await tree.getByRole('treeitem', { name: /^Ship components,/ }).locator('button').click();
    await tree.getByRole('treeitem', { name: /^Coolers,/ }).click();
    await tree.getByRole('treeitem', { name: /^Shields,/ }).click({ modifiers: ['Control'] });
    await p.getByRole('button', { name: 'Remove filter Shields' }).waitFor();
  }],
  ['strings-groups-rail', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    await p.getByRole('button', { name: 'Hide groups' }).click();
    await p.getByRole('button', { name: 'Show groups' }).waitFor();
  }],
  ['strings-groups-manufacturer', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    await p.getByRole('combobox', { name: 'Group by' }).click();
    await p.getByRole('option', { name: 'Group by manufacturer' }).click();
    await p.getByRole('treeitem', { name: /^Beta Works,/ }).click();
    await p.getByRole('button', { name: 'Remove filter Beta Works' }).waitFor();
  }],
  ['strings-groups-approximate', 'strings', async (p) => {
    await p.getByText('Approximate groups – reading game data…').waitFor();
  }, '&groups=approximate&groupsread=slow'],
  ['strings-groups-narrow', 'strings', async (p) => {
    await p.locator('[data-row="0"]').getByRole('gridcell').first().waitFor();
    await p.getByRole('button', { name: /^Groups/ }).click();
    await p.getByRole('dialog', { name: 'Groups' }).getByRole('treeitem', { name: /^Armor,/ }).click();
    await p.getByRole('button', { name: 'Remove filter Armor' }).waitFor();
  }],
  ['strings-detail-rejected', 'strings', async (p) => {
    await p.getByRole('searchbox', { name: 'Search keys and text' }).fill('Cooler_S2_00001');
    await p.getByText(/^1 of /).waitFor();
    await p.locator('[data-row="0"]').click();
    const ta = p.locator('#sd-value');
    await ta.waitFor();
    await ta.fill('My cooler ~ open');
    await p.getByRole('button', { name: 'Save edit' }).click();
    await p.getByText(/Not saved: /).waitFor();
  }],
  ['patches', 'patches', async (p) => p.getByText('Cargo grid typo').waitFor()],
  ['patches-detail', 'patches', async (p) => {
    await p.getByRole('button', { name: 'Details of Moved description key' }).click();
    await p.getByText('Stock text now').first().waitFor();
  }],
  ['blueprints', 'blueprints', async (p) => p.getByText('Synthetic Armor Core').waitFor()],
  ['blueprints-import-report', 'blueprints', async (p) => {
    await p.getByRole('button', { name: 'Import from Smart Citizen' }).click();
    await p.getByRole('button', { name: 'Choose file and check' }).click();
    await p.getByText('Will be added to').waitFor();
  }],
  ['blueprints-starorg-connected', 'blueprints', async (p) => p.getByText(/Signed in as/).waitFor(), '&starorg=1'],
  ['ships', 'ships', async (p) => p.getByRole('listbox', { name: 'My ships, in order' }).waitFor(), '&fleet=grouped'],
  ['ships-empty', 'ships', async (p) => p.getByText('Make your own ship list').waitFor()],
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
  // States (mock URL switches; see mock.ts).
  ['home-running', 'home', async (p) => p.getByText(/The game is running on LIVE/).waitFor(), '&running=LIVE'],
  ['home-interrupted', 'home', async (p) => p.getByText('The last apply did not finish').waitFor(), '&interrupted=LIVE'],
  ['home-builderr', 'home', async (p) => p.getByText('Could not read the game build').waitFor(), '&builderr=LIVE'],
  ['home-foreign', 'home', async (p) => p.getByText(/changed the text on LIVE/).waitFor(), '&foreign=LIVE'],
  ['home-foreign-remove', 'home', async (p) => {
    await p.getByText(/changed the text on LIVE/).waitFor();
    await p.getByRole('button', { name: 'Remove changes' }).click();
    await p.getByText(/will be removed/).waitFor();
  }, '&foreign=LIVE'],
  ['home-noinstall', 'home', async (p) => p.getByText('No Star Citizen install found').first().waitFor(), '&installs=0'],
  ['home-error', 'home', async (p) => p.getByText('Could not read your game channels').waitFor(), '&fail=overview'],
  ['home-applying', 'home', async (p) => {
    await p.getByRole('heading', { name: 'LIVE' }).waitFor();
    await p.getByRole('button', { name: /^Apply to LIVE/ }).click();
    await p.getByRole('button', { name: /^Stop:/ }).waitFor();
  }, '&delay=3000'],
  ['home-channels5', 'home', async (p) => p.getByRole('combobox', { name: 'Channel' }).waitFor(), '&channels=5'],
  ['home-collapsed-amber-black', 'home', async (p) => p.getByRole('heading', { name: 'LIVE' }).waitFor(), '&accent=amber&palette=black', true],
  ['backups-empty', 'backups', async (p) => p.getByText(/No backups yet/).waitFor(), '&nobackups=LIVE'],
  ['backups-running', 'backups', async (p) => p.getByText(/Game running: close it/).first().waitFor(), '&running=LIVE'],
  ['activity-empty', 'activity', async (p) => p.locator('#page-title').waitFor(), '&log=empty'],
  ['strings-ptu-not-set-up', 'strings', async (p) => {
    await p.getByRole('radio', { name: /PTU/ }).click();
    await p.getByText('PTU is not set up').waitFor();
  }],
  ['enhancements-general', 'enhancements', async (p) => p.getByText('Header line').waitFor()],
  ['enhancements-ships', 'enhancements', async (p) => {
    await p.getByRole('tab', { name: /^Ships/ }).click();
    await p.getByText('Boost speeds').waitFor();
  }],
  ['enhancements-unsaved-leave', 'enhancements', async (p) => {
    await p.getByRole('tab', { name: /^Ships/ }).click();
    await p.getByLabel('Boost speeds').click();
    await p.getByRole('radio', { name: /HOTFIX/ }).click();
    await p.getByRole('heading', { name: 'Save your changes first?' }).waitFor();
  }],
  ['patches-add', 'patches', async (p) => {
    await p.getByRole('button', { name: 'Add patch' }).click();
    await p.locator('#np-title').fill('Sample label fix');
    await p.locator('#np-key').fill('ui_SYN_Label_00007');
    await p.locator('#np-value').fill('Sign 7');
    await p.getByRole('button', { name: 'Add patch' }).last().click();
    await p.getByRole('button', { name: 'Export Sample label fix' }).click();
    await p.getByText('op = "set"').waitFor();
  }],
  ['settings-starorg-hooks', 'settings', async (p) => {
    await p.getByText('Default channel').waitFor();
    await p.locator('#starorg').scrollIntoViewIfNeeded();
    await p.evaluate(() => document.getElementById('starorg').scrollIntoView({ block: 'start' }));
    await p.getByText('Characters to sync').waitFor();
  }, '&starorg=1'],
  // Update check (docs/updates.md; mock-update.ts switches).
  ['home-update-available', 'home', async (p) => p.getByText('StarOrg Strings 0.2.0 is available').waitFor(), '&update=available'],
  ['home-update-notes', 'home', async (p) => p.getByText('core: keep the user.cfg order when re-applying').waitFor(), '&update=available&notes=1'],
  ['settings-updates', 'settings', async (p) => {
    await p.getByText('0.2.0 is available (this is 0.1.0).').waitFor();
    await p.evaluate(toNetwork);
  }, '&update=available'],
  ['settings-updates-store', 'settings', async (p) => {
    await p.getByText('Updates come from the Microsoft Store.').waitFor();
    await p.evaluate(toNetwork);
  }, '&update=store'],
  ['settings-updates-failed', 'settings', async (p) => {
    await p.getByText(/^Can't see releases for someone\/private-fork/).waitFor();
    await p.evaluate(toNetwork);
  }, '&update=failed'],
  ['settings-updates-off', 'settings', async (p) => {
    await p.getByText('Check for updates').waitFor();
    await p.evaluate(toNetwork);
  }],
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
  const sizes = process.env.SIZES ? process.env.SIZES.split(",").map((s) => s.split("x").map(Number)) : [[1280, 800], [1200, 780], [1024, 700]];
  for (const [w, h] of sizes) {
    for (const theme of ['dark', 'light']) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, screen, prep, extra, collapsed] of shots) {
        if (only && !only.test(name)) continue;
        // The group pane exists from 1,000 px, the popover form below.
        if (name.startsWith('strings-groups') && name.endsWith('-narrow') !== w < 1000) continue;
        const page = await ctx.newPage();
        page.setDefaultTimeout(8000);
        await page.addInitScript((c) => {
          localStorage.setItem('starorg:sidebar', JSON.stringify({ w: 208, collapsed: c }));
          // Each shot starts from default Strings group prefs (the context is shared).
          for (const k of Object.keys(localStorage)) if (k.startsWith('starorg.strings.')) localStorage.removeItem(k);
        }, !!collapsed);
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
