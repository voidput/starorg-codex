// Keyboard-only pass through the wizard (headless): Tab/Shift+Tab/Enter/Space/arrows only.
const { chromium } = require(process.env.PLAYWRIGHT_CORE || 'playwright-core');
(async () => {
  const b = await chromium.launch({ headless: true });
  const p = await b.newPage({ viewport: { width: 1200, height: 780 } });
  const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  await p.goto('http://127.0.0.1:4317/?delay=0');
  await p.waitForSelector('#wizard-title');
  await p.getByText('found by LUG Helper config').waitFor();
  const pressUntil = async (name) => {
    for (let i = 0; i < 40; i++) {
      await p.keyboard.press('Tab');
      const t = await p.evaluate(() => document.activeElement?.textContent?.trim() ?? '');
      if (t.startsWith(name)) return;
    }
    throw new Error(`could not tab to ${name}`);
  };
  const heading = async () => p.evaluate(() => document.getElementById('wizard-step-title')?.textContent?.trim());
  // Step 1: arrow to the second install and back (native radios), then Next.
  await p.keyboard.press('Tab');
  await pressUntil('Next'); await p.keyboard.press('Enter');
  console.log('after step1:', await heading(), 'focus on', await p.evaluate(() => document.activeElement?.tagName));
  await pressUntil('Next'); await p.keyboard.press('Enter');
  console.log('after step2:', await heading());
  await pressUntil('Next'); await p.keyboard.press('Enter');
  console.log('after step3:', await heading());
  await p.getByText('strings changed').first().waitFor();
  await pressUntil('Set up and apply'); await p.keyboard.press('Enter');
  await p.getByRole('button', { name: 'Open StarOrg Codex' }).waitFor({ timeout: 10000 });
  await pressUntil('Open StarOrg Codex'); await p.keyboard.press('Enter');
  await p.waitForSelector('#wizard-title', { state: 'detached' });
  console.log('wizard closed; page title:', await p.evaluate(() => document.getElementById('page-title')?.textContent));
  // Escape guard: reopen via Ctrl+K palette.
  await p.keyboard.press('Control+k');
  await p.waitForSelector('#palette-list'); await p.waitForTimeout(150);
  await p.keyboard.type('setup');
  await p.keyboard.press('Enter');
  await p.waitForSelector('#wizard-title');
  await pressUntil('Next'); await p.keyboard.press('Enter');
  await p.waitForTimeout(300);
  console.log('before esc: step', await heading(), 'dialogs', await p.evaluate(() => document.querySelectorAll('[data-slot=dialog-content]').length), 'focus', await p.evaluate(() => document.activeElement?.outerHTML.slice(0,60)));
  await p.keyboard.press('Escape');
  await p.waitForTimeout(200);
  console.log('guard shown:', await p.getByRole('heading', { name: 'Leave setup for now?' }).isVisible());
  await p.keyboard.press('Escape');
  await p.keyboard.press('Alt+8');
  console.log('errors:', errs.length ? errs : 'none');
  await b.close();
})().catch((e) => { console.error(e); process.exit(1); });
