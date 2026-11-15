/**
 * Legal texts shown in Footer -> About (AboutDialog.svelte). The same wording is in the root
 * README.md and docs/store-listing.md; keep them in step.
 */
export const REPO_URL = 'https://github.com/voidput/starorg-codex';

/** Fan content disclaimer (RSI/CIG fan content policy). */
export const FAN_DISCLAIMER =
  'StarOrg Codex is an unofficial Star Citizen fan project, not affiliated with the Cloud Imperium group of companies. ' +
  'All content in this app not authored by its host or users are property of their respective owners. ' +
  'Star Citizen®, Roberts Space Industries® and Cloud Imperium® are registered trademarks of Cloud Imperium Rights LLC.';

/** What the app touches: only the user's own install, and it ships no game assets. */
export const LOCAL_FILES_NOTE =
  'StarOrg Codex ships no game assets. It reads the game files of your own installation and writes only your own local files ' +
  '(the game text file, one line in USER.cfg, and its own settings, backups and cache).';
