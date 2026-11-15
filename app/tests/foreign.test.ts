// Another tool's global.ini (Smart Citizen, ...): the warning before writing, the acknowledgement
// that gates "Set up and apply", and the wording of the Apply result. Run: npm test.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ACK_LABEL, ackKey, foreignAfter, foreignBefore, foreignBeforeApply, foreignOf, isForeignWarning, toolFromWarning } from '../src/lib/foreign.ts';
import { blocker, emptyDraft, foreignPicked } from '../src/components/wizard/draft.svelte.ts';

const samples = JSON.parse(readFileSync(new URL('../src/lib/contract/samples.gen.json', import.meta.url), 'utf8'));
const install = structuredClone(samples.overview_first_run.installs[0]);
const live = install.channels.find((c: { channel: string }) => c.channel === 'LIVE');
const ptu = install.channels.find((c: { channel: string }) => c.channel === 'PTU');
live.status = { state: 'changed-by-another-tool', tool: 'Smart Citizen' };
live.foreign_file = true;
live.foreign_tool = 'Smart Citizen';
ptu.status = { state: 'changed-by-another-tool', tool: null };
ptu.foreign_file = true;
ptu.foreign_tool = null;

test('the callout names the tool, the channel, the backup and how to stop the fight', () => {
  const t = foreignBefore(foreignOf(live)!);
  assert.equal(t.title, "Smart Citizen has changed LIVE's game text");
  assert.match(t.body, /back up that file and replace it with StarOrg Codex's text/);
  assert.match(t.body, /restore it any time from Backups/);
  assert.match(t.stop, /turn off Smart Citizen's auto-apply or patching for LIVE \(or uninstall it\) first/);
  assert.equal(foreignBefore(foreignOf(ptu)!).title, "Another tool has changed PTU's game text");
  assert.match(foreignBeforeApply(foreignOf(live)!).body, /^Applying will/);
});

test('a channel with our own or no file is not foreign', () => {
  const c = { ...live, status: { state: 'applied' }, foreign_file: false, foreign_tool: null };
  assert.equal(foreignOf(c), null);
});

test('Review blocks "Set up and apply" until "Back up and replace" is ticked', () => {
  const d = emptyDraft();
  for (const c of install.channels) d.channels[c.channel] = { consent: false, switch_language: false };
  d.channels.LIVE!.consent = true;
  assert.equal(foreignPicked(d, install).length, 1);
  assert.match(blocker('review', d, install)!, new RegExp(ACK_LABEL));
  d.replace_ack = ackKey(foreignPicked(d, install));
  assert.equal(blocker('review', d, install), null);
  // A newly ticked channel with another tool's file needs a new tick.
  d.channels.PTU!.consent = true;
  assert.notEqual(blocker('review', d, install), null);
  // Not ticked = not touched = nothing to acknowledge.
  d.channels.LIVE!.consent = false;
  d.channels.PTU!.consent = false;
  assert.equal(blocker('review', d, install), null);
});

test('the Apply result rewords the core warning and keeps the tool name', () => {
  const w = { kind: 'foreign-file', detail: 'replacing a global.ini from Smart Citizen (kept as a backup); do not run two tools on one channel' };
  assert.ok(isForeignWarning(w));
  assert.equal(toolFromWarning(w), 'Smart Citizen');
  assert.equal(toolFromWarning({ kind: 'foreign-file', detail: 'replacing a global.ini from another tool (kept as a backup); ...' }), null);
  const a = foreignAfter({ channel: 'LIVE', tool: 'Smart Citizen' });
  assert.equal(a.title, "Replaced Smart Citizen's file on LIVE");
  assert.match(a.body, /backed up \(kept for good\)/);
  assert.match(foreignAfter({ channel: 'PTU', tool: null }).body, /^The other tool's global.ini/);
});
