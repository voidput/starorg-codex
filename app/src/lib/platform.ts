/**
 * Which OS the window runs on, for wording only (paths and places we searched). Every decision
 * that matters (discovery, guards, paths) is made in Rust. WebView2 reports "Windows" in its
 * user agent; WebKitGTK reports Linux.
 */
export function isWindowsAgent(ua: string): boolean {
  return /\bWindows\b|\bWin64\b|\bWin32\b/.test(ua);
}

export const onWindows: boolean = typeof navigator !== 'undefined' && isWindowsAgent(navigator.userAgent ?? '');

/** Setup wizard, "Find your game": where discovery looked. */
export function searchedWhere(windows: boolean): string {
  return windows
    ? 'StarOrg Codex looked in the RSI Launcher logs and the usual install folders on every drive (Program Files, Roberts Space Industries, Games).'
    : 'StarOrg Codex looked in the LUG Helper config, your Wine prefixes, Lutris and the RSI Launcher logs.';
}

/** Setup wizard, nothing found. */
export function notFoundHint(windows: boolean): string {
  return windows
    ? 'Browse to the StarCitizen folder, the one that contains LIVE (by default C:\\Program Files\\Roberts Space Industries\\StarCitizen).'
    : 'Browse to the StarCitizen folder, or run a deeper scan of /mnt, removable media and your home folder.';
}
