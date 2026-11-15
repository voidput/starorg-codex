---
title: Uninstall
tags: [docs]
updated: 2026-11-15
---

# Uninstall

How to take StarOrg Codex off a Windows or Linux computer and leave the game as it was. Three
steps, in this order:

1. [Put the game back to stock](#1-put-the-game-back-to-stock) (and, on Linux, remove the launch hooks).
2. [Remove the program](#2-remove-the-program) for the way you installed it.
3. [Optional: delete your data](#3-optional-delete-your-data) (settings, backups, caches).

**Do step 1 first.** No uninstaller touches the game. If you skip it, the game keeps our
`global.ini` and the `g_language = english` line in `USER.cfg`, and on Linux the LUG Helper
scripts keep our block, which re-applies the text at every launch while `starorg-codex` is
still installed.

## 1. Put the game back to stock

Close the game first: an apply or remove is refused while `StarCitizen.exe` runs on that
channel.

### In the app

For **each channel** you set up (LIVE, PTU, ...): pick it, then **Home -> Remove changes**
(or **Backups -> Remove StarOrg changes -> Remove changes**) and confirm.

### With the command line

```sh
starorg-codex remove --all
starorg-codex status
```

`remove --all` covers every channel of the install it finds. Have more than one install (another
drive, another Wine prefix)? Run it once per install with `--game-dir <folder that contains LIVE>`
(or, on Linux, `--prefix <Wine prefix>`). `status` should then say *Not applied* for every channel.

### What Remove does

- **`global.ini`** (`Data/Localization/english/global.ini` in the channel): deleted when it is
  the file we wrote, so the game shows its stock text again.
- **A `global.ini` from another tool** (Smart Citizen, for example): left alone. The app asks
  before removing it; on the command line add `--yes` (`starorg-codex remove --all --yes`). It
  is backed up first.
- **`USER.cfg`**: only the `g_language = english` line we added is taken out. A
  `g_language = english` line that was there before us stays. If you had another language and
  agreed to switch it, that old line comes back. Every other line stays. If you changed our line since, it is left alone (Remove warns).
  If another channel still uses the same `USER.cfg` (a shared link), the line stays until that
  channel is removed too.
- **Backups** are kept (in the data folder, step 3). Remove never deletes them.
- **Your settings and edits** are kept, in case you apply again.

### Want another tool's file back?

If Smart Citizen or another tool's `global.ini` was in place before you first applied, it was
backed up. To get that file back instead of stock text, restore its backup:

- App: **Backups**, pick the channel, find the backup marked as from another tool, **Restore**.
- CLI:

  ```sh
  starorg-codex backups --channel LIVE
  starorg-codex restore --channel LIVE --backup <id>
  ```

  Add `--allow-build-mismatch` if the backup was made for another game build.

### Linux: remove the launch hooks

Only if you installed the LUG Helper launch hooks. Do this **while the app or CLI is still
installed**: you need it to remove the block. Remove keeps the channel set up, so as long as the
hooks and `starorg-codex` are both present, the pre-launch script re-applies the text at the
next game start. With the CLI gone the block does nothing, but it stays in the scripts.

- App: **Settings -> Launch hooks -> Remove hooks** (click again to confirm).
- CLI:

  ```sh
  starorg-codex hook uninstall
  starorg-codex hook status
  ```

  Add `--prefix <Wine prefix>` (or `--prelaunch-script <file>` / `--postexit-script <file>`) if
  the prefix is not found automatically.

This takes our block (between `# >>> starorg-codex hook >>>` and `# <<< starorg-codex hook <<<`)
out of `sc-prelaunch.sh` and `sc-postexit.sh` and keeps the rest of each script as it was. Old
0.2 blocks (`# >>> starorg-strings hook >>>`) are removed too. A copy of each script is saved
first in the data folder under `hooks/backups/`. Windows has no launch hooks.

### Signed in to StarOrg Tools?

Only if you used **Settings -> StarOrg Tools**. Press **Sign out** there before you uninstall: it
deletes the sign-in token from the system keyring (Windows Credential Manager, or the Secret
Service on Linux) or from its file. Uninstalling does not remove keyring entries; if you
skipped this, delete the entries for `starorg-codex` (account `starorg-tools-<env>`, one per server you signed
in to) in Credential Manager (Windows Credentials)
or with your keyring app (for example Seahorse) on Linux.

### Used the local MCP server?

The app never edits your AI client's settings; you added the entry yourself
([mcp](mcp.md)). Take it out again:

- **Claude Code:** `claude mcp remove starorg-codex` (add `-s user` or `-s project` if you added it
  with that scope).
- **Claude Desktop:** delete the `"starorg-codex"` entry under `"mcpServers"` in
  `claude_desktop_config.json` (on Windows `%APPDATA%\Claude\claude_desktop_config.json`), then
  restart Claude Desktop.

## 2. Remove the program

### Windows

| Installed with | Remove it |
|---|---|
| `StarOrg.Codex_<v>_x64-setup.exe` (NSIS) | **Settings -> Apps -> Installed apps -> StarOrg Codex -> Uninstall** (Windows 10: *Apps & features*). The uninstaller's *Delete the application data* box removes only the webview storage (`%APPDATA%\tools.starorg.codex`, `%LOCALAPPDATA%\tools.starorg.codex`), not the folders in step 3 |
| `StarOrg.Codex_<v>_x64_en-US.msi` | **Settings -> Apps -> Installed apps -> StarOrg Codex -> Uninstall** |
| Microsoft Store | **Settings -> Apps -> Installed apps -> StarOrg Codex -> Uninstall** (or right-click it in Start -> Uninstall). This **also deletes its settings and backups**, see below |
| CLI zip `starorg-codex-v<v>-windows-x86_64.zip` | Nothing is installed: delete the folder you unpacked (`starorg-codex.exe`, `starorg-strings.exe`, `LICENSE`, `THIRD_PARTY_NOTICES.md`). If you added that folder to your `PATH`, take it out again (Start -> "Edit environment variables for your account" -> Path) |

**Microsoft Store:** the Store copy keeps its settings and backups inside its package folder
(`%LOCALAPPDATA%\Packages\<package>\LocalCache\`), and Windows deletes that folder when you
uninstall. Run **Remove** in the Store app first (step 1): the CLI zip or a direct-download copy
has its own, separate data and does not know what the Store copy applied. Want a backup kept?
Copy it out of that folder before you uninstall.

### Linux

| Installed with | Remove it |
|---|---|
| `.deb` (Debian, Ubuntu, Mint, Pop!_OS) | `sudo apt remove star-org-codex`. Removes `/usr/bin/starorg-codex-app`, the menu entries and icons |
| `.rpm` (Fedora, Nobara) | `sudo dnf remove star-org-codex` |
| `.rpm` (openSUSE) | `sudo zypper remove star-org-codex` |
| Installer script (`scripts/install-linux.sh`) | `sh install-linux.sh --uninstall --app` (see below) |
| AppImage you downloaded yourself | Delete the `.AppImage` file, and any menu entry you made for it |
| CLI tarball `starorg-codex-v<v>-linux-x86_64.tar.gz` | Delete `starorg-codex` and the `starorg-strings` link from where you put them (for example `rm ~/.local/bin/starorg-codex ~/.local/bin/starorg-strings`) |
| Built from source | Delete the binaries you copied (for example `~/.local/bin/starorg-codex`) and the clone |

**Installer script.** The same script that installed it removes it, for your user, without sudo:

```sh
curl -fsSL https://raw.githubusercontent.com/voidput/starorg-codex/main/scripts/install-linux.sh | sh -s -- --uninstall --app
```

Add `--dry-run` to only see what it would remove, and `--bin-dir DIR` if you installed with
`--bin-dir`. Without `--app` it removes only the CLI. It removes:

- `~/.local/bin/starorg-codex` and the `starorg-strings` link (the 0.2 name)
- with `--app`: `~/.local/bin/StarOrg-Codex.AppImage`, the menu entry
  `~/.local/share/applications/starorg-codex-app.desktop`, its icon
  `~/.local/share/icons/hicolor/128x128/apps/starorg-codex-app.png`, and the 0.2 menu entry and
  icon (`starorg-strings-app.desktop`, `starorg-strings-app.png`) if they are still there

(`~/.local/bin` is `$XDG_BIN_HOME` and `~/.local/share` is `$XDG_DATA_HOME` when you set them.)
It keeps your settings, backups and the game, and prints the data folders so you can delete
them in step 3.

## 3. Optional: delete your data

The app and the CLI share these folders. Nothing of ours is in the game folder. Delete what you
no longer want; **keep the data folder if you may still need a backup** of a game file.

### Linux

| Folder | Holds |
|---|---|
| `~/.config/starorg-codex/` | Settings and profiles (`config.toml`), the StarOrg Tools token file if there was no keyring (`starorg-tokens/`) |
| `~/.local/share/starorg-codex/` | **Backups** of game files (per install and channel), the `USER.cfg` records Remove uses, your patches, blueprints, fleet and ship names, launch hook script backups (`hooks/backups/`), update state |
| `~/.cache/starorg-codex/` | Extracted game text and game data, the Developer mode asset cache (`assets/`), MCP datasets (`mcp/`). Safe to delete, rebuilt when needed |
| `~/.local/share/tools.starorg.codex/`, `~/.cache/tools.starorg.codex/` | The app window's storage (remembered layout and filters) |

`~/.config`, `~/.local/share` and `~/.cache` are `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME` and
`$XDG_CACHE_HOME` when you set them. Also look for `~/.local/share/.starorg-codex-migration.lock`
(an empty lock file from the 0.3 rename).

### Windows

| Folder | Holds |
|---|---|
| `%APPDATA%\StarOrg Tools\Codex\config\` | Settings and profiles (`config.toml`), the StarOrg Tools token file if there was no keyring (`starorg-tokens\`) |
| `%APPDATA%\StarOrg Tools\Codex\data\` | **Backups** of game files (per install and channel), the `USER.cfg` records Remove uses, your patches, blueprints, fleet and ship names, update state |
| `%LOCALAPPDATA%\StarOrg Tools\Codex\cache\` | Extracted game text and game data, the Developer mode asset cache (`assets\`), MCP datasets (`mcp\`). Safe to delete, rebuilt when needed |
| `%LOCALAPPDATA%\tools.starorg.codex\` | The app window's storage (WebView2) |

When the `StarOrg Tools` folders are empty afterwards, delete them too (`%APPDATA%\StarOrg Tools\`
may also hold `.codex-migration.lock`, an empty lock file from the 0.3 rename). Paste a path such
as `%APPDATA%\StarOrg Tools` into the File Explorer address bar to open it. Store copy: see
[step 2](#windows).

### Asset cache in a folder you chose

If you set **Settings -> Advanced -> Asset cache** to your own folder, the cache is in a
`StarOrg Codex asset cache` subfolder there. **Clear** in that setting empties it (the row is
shown only with Developer mode on); or delete that subfolder by hand. If you changed the folder before, earlier folders keep their
`StarOrg Codex asset cache` subfolder too. Do this before deleting the config folder, which holds the setting.

### Old StarOrg Strings folders (before 0.3.0)

The first start of 0.3 renames these to the folders above. They stay behind if a new folder
already existed, if the move was a copy to another drive (then the old one is kept), or if the
move failed. Delete
them once you have what you need:

- Linux: `~/.config/starorg-strings/`, `~/.local/share/starorg-strings/`,
  `~/.cache/starorg-strings/`, `~/.local/share/tools.starorg.strings/`,
  `~/.cache/tools.starorg.strings/`
- Windows: `%APPDATA%\StarOrg Tools\Strings\`, `%LOCALAPPDATA%\StarOrg Tools\Strings\`,
  `%LOCALAPPDATA%\tools.starorg.strings\`

## Check it is gone

- `starorg-codex` (and `starorg-strings`) is no longer found in a new terminal.
- In each channel folder, `Data/Localization/english/global.ini` is gone (or is the other tool's
  file you restored), and `USER.cfg` no longer has a `g_language` line from us.
- Linux: `sc-prelaunch.sh` and `sc-postexit.sh` in the Wine prefix have no `starorg-codex` (or
  `starorg-strings`) block.
- The RSI Launcher's **Verify files** does not need to run: we never change the game's own files.
