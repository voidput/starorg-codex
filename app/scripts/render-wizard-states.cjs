// Headless renders of the wizard's slow, failing and other-tool states at the Windows tester's
// window (1544x1366) and a small laptop (1280x800), light and dark (mock backend, no window).
// Run against `vite preview`: BASE_URL=http://127.0.0.1:<port>/ node scripts/render-wizard-states.cjs
// Fails (exit 1) when a panel is blank: no preview, no loading state, no error, no message.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
const fs = require('fs');
const OUT = (process.env.RENDER_DIR || require('path').resolve(__dirname, '../../.tmp/renders')) + '/wizard-states';
const BASE = process.env.BASE_URL || 'http://127.0.0.1:4317/';
fs.mkdirSync(OUT, { recursive: true });

const sizes = [
  [1544, 1366],
  [1280, 800],
];
/** name, query, check(page) that throws when the state is not shown. */
const cases = [
  ['4-enhancements', '?wizard=enhancements', (p) => p.getByText('Ship description').first().waitFor()],
  ['4-enhancements-nodata', '?wizard=enhancements&preview=nodata', (p) => p.getByTestId('preview-nodata').waitFor()],
  [
    '4-enhancements-reading',
    '?wizard=enhancements&preview=nodata&slow=plan_setup:60000',
    async (p) => {
      await p.getByRole('button', { name: 'Read game data now' }).click();
      await p.getByTestId('working').waitFor();
    },
  ],
  [
    '4-enhancements-read-done',
    '?wizard=enhancements&preview=nodata&slow=plan_setup:300',
    async (p) => {
      await p.getByRole('button', { name: 'Read game data now' }).click();
      await p.getByText('Ship description').first().waitFor({ timeout: 5000 });
    },
  ],
  ['4-enhancements-failed', '?wizard=enhancements&fail=build_preview', (p) => p.getByText('Preview failed').waitFor()],
  ['6-review-slow', '?wizard=review&slow=plan_setup:60000', (p) => p.getByTestId('working').waitFor()],
  [
    '6-review-slow-hint',
    '?wizard=review&slow=plan_setup:60000',
    async (p) => {
      await p.getByTestId('working').waitFor();
      await p.getByText('can take a few minutes').waitFor({ timeout: 20000 });
    },
  ],
  ['6-review-failed', '?wizard=review&fail=plan_setup', (p) => p.getByText('Could not prepare the review').waitFor()],
  [
    '6-review-foreign',
    '?wizard=review&foreign=LIVE',
    async (p) => {
      await p.getByTestId('foreign-review').waitFor();
      await p.getByText('strings changed').first().waitFor();
      if (await p.getByRole('button', { name: /Set up and apply/ }).isEnabled()) throw new Error('Set up and apply enabled before the acknowledgement');
    },
  ],
  [
    '6-review-foreign-acked',
    '?wizard=review&foreign=LIVE',
    async (p) => {
      await p.locator('#replace-ack').click();
      await p.getByText('strings changed').first().waitFor();
      if (!(await p.getByRole('button', { name: /Set up and apply/ }).isEnabled())) throw new Error('Set up and apply still disabled after the acknowledgement');
    },
  ],
  [
    '6-review-readfail',
    '?wizard=review&failonce=plan_setup',
    async (p) => {
      // The real shape: Ok with a game-data warning and zero counts. Error per channel, no apply.
      await p.getByText("Could not read LIVE's game data").waitFor();
      if (await p.getByRole('button', { name: /Set up and apply/ }).isEnabled()) throw new Error('apply enabled with unread game data');
    },
  ],
  [
    '6-review-readfail-retried',
    '?wizard=review&failonce=plan_setup',
    async (p) => {
      await p.getByRole('button', { name: 'Try again' }).first().click();
      await p.getByText('strings changed').first().waitFor();
      if (await p.getByText('Game data not read').count()) throw new Error('error left after retry');
    },
  ],
  [
    '6-review-busy',
    '?wizard=review&busy=1',
    async (p) => {
      await p.getByText('Waiting for another game-data read to finish').waitFor();
      await p.getByText('strings changed').first().waitFor({ timeout: 8000 });
    },
  ],
  [
    '6-review-joins-running-read',
    '?wizard=enhancements&preview=nodata&slow=plan_setup:2500',
    async (p) => {
      // Read from Enhancements, then go on to Review while it runs: Review waits and uses it.
      await p.getByRole('button', { name: 'Read game data now' }).click();
      await p.getByTestId('working').waitFor();
      await p.getByRole('button', { name: /^Next/ }).click();
      await p.getByRole('button', { name: /^Next/ }).click();
      await p.getByRole('heading', { name: 'Review' }).waitFor();
      await p.getByText('strings changed').first().waitFor({ timeout: 10000 });
      if (await p.getByText('Game data not read').count()) throw new Error('review shows unread data');
    },
  ],
  [
    '6-review-two-foreign',
    '?wizard=review&foreign=LIVE,HOTFIX',
    async (p) => {
      await p.getByText('strings changed').first().waitFor();
      const box = await p.locator('#replace-ack').boundingBox();
      const vp = p.viewportSize();
      if (!box || box.y < 0 || box.y + box.height > vp.height) throw new Error('the tick is off screen');
    },
  ],
  [
    '7-restore-original',
    '?wizard=apply&foreign=LIVE',
    async (p) => {
      await p.getByRole('button', { name: 'Open StarOrg Codex' }).waitFor({ timeout: 15000 });
      await p.getByRole('button', { name: /Restore the original/ }).first().click();
      await p.getByRole('heading', { name: 'Restore the file from before StarOrg Codex?' }).waitFor();
    },
  ],
  ['2-channels-foreign', '?wizard=channels&foreign=LIVE', (p) => p.getByText("Smart Citizen has changed LIVE's game text").first().waitFor()],
  [
    '7-apply-done-foreign',
    '?wizard=apply&foreign=LIVE',
    async (p) => {
      await p.getByRole('button', { name: 'Open StarOrg Codex' }).waitFor({ timeout: 15000 });
      await p.getByTestId('foreign-after').first().waitFor();
    },
  ],
  [
    '7-apply-done-windows',
    '?wizard=apply&hooks=windows',
    async (p) => {
      await p.getByRole('button', { name: 'Open StarOrg Codex' }).waitFor({ timeout: 15000 });
      await p.getByTestId('automate-app').waitFor();
      if (await p.getByText('LUG Helper').count()) throw new Error('LUG Helper advice shown without the LUG Helper');
    },
  ],
  [
    'home-foreign-confirm',
    '?state=ready&foreign=LIVE',
    async (p) => {
      await p.getByTestId('foreign-home').waitFor();
      await p.getByRole('button', { name: /Replace Smart Citizen's file with ours/ }).click();
      await p.getByRole('dialog').getByText('Back up and replace').waitFor();
    },
  ],
];

(async () => {
  const browser = await chromium.launch({ headless: true });
  const problems = [];
  for (const theme of ['light', 'dark']) {
    for (const [w, h] of sizes) {
      const ctx = await browser.newContext({ viewport: { width: w, height: h }, colorScheme: theme, reducedMotion: 'reduce' });
      for (const [name, q, check] of cases) {
        const page = await ctx.newPage();
        page.on('pageerror', (e) => problems.push(`${name}/${theme}/${w}: ${e.message}`));
        await page.goto(`${BASE}${q}&theme=${theme}&delay=0`);
        await page.waitForSelector('#wizard-title, #page-title');
        try {
          await check(page);
        } catch (e) {
          problems.push(`${name}/${theme}/${w}x${h}: ${e.message.split('\n')[0]}`);
        }
        // The rail must not truncate step names when the window is wide enough.
        if (q.includes('wizard=') && w >= 1280) {
          const cut = await page.$$eval('nav[aria-label="Setup steps"] li span.whitespace-nowrap', (els) =>
            els.filter((e) => getComputedStyle(e).display !== 'none' && e.scrollWidth > e.clientWidth + 1).map((e) => e.textContent),
          );
          if (cut.length) problems.push(`${name}/${theme}/${w}: truncated rail labels ${cut.join(', ')}`);
        }
        await page.waitForTimeout(250);
        await page.screenshot({ path: `${OUT}/${name}-${w}x${h}-${theme}.png` });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(problems.length ? problems.join('\n') : `ok: ${cases.length * sizes.length * 2} renders in ${OUT}`);
  if (problems.length) process.exit(1);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
