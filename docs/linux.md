---
title: Linux
tags: [docs]
updated: 2026-11-15
---

# Linux

StarOrg Codex runs natively on Linux (x86_64). It does not need Wine: it reads and writes the
game's files inside your Wine prefix directly. Builds come from Ubuntu 22.04, so any distro
with glibc 2.35 or newer works (the CLI is a static musl binary and runs anywhere).

The fastest way to get the CLI on your `PATH` (and, with `--app`, the AppImage with a menu
entry), for your user only, without sudo:

```sh
curl -fsSL https://raw.githubusercontent.com/voidput/starorg-strings/main/scripts/install-linux.sh | sh -s -- --app
```

Read the script first if you like ([scripts/install-linux.sh](../scripts/install-linux.sh));
it is described under [Installer script](#installer-script). Everything else on this page is
the manual way.

## Flavours

All files are on the [release page](https://github.com/voidput/starorg-strings/releases).
`<v>` is the version without the `v`, e.g. `0.1.0`.

| | AppImage | .deb | .rpm | CLI tarball | Build from source |
|---|---|---|---|---|---|
| File | `StarOrg.Codex_<v>_amd64.AppImage` | `StarOrg.Codex_<v>_amd64.deb` | `StarOrg.Codex-<v>-1.x86_64.rpm` | `starorg-strings-v<v>-linux-x86_64.tar.gz` | this repo |
| For | Arch, CachyOS, Steam Deck, atomic distros, anything else | Debian, Ubuntu, Mint, Pop!_OS | Fedora, openSUSE, Nobara | Scripts, launch hooks, servers; every flavour's command line | Other CPUs, development |
| Contains | The app (`starorg-strings-app`) with WebKitGTK bundled | `/usr/bin/starorg-strings-app`, menu entry, icons; uses the system `libwebkit2gtk-4.1-0` | Same as the .deb | `starorg-strings` (static) and `LICENSE` | What you build |
| CLI included | No: add the CLI tarball | No: add the CLI tarball | No: add the CLI tarball | Yes | `cargo build -p starorg-cli` |
| Updates | In the app (Settings -> Network -> Check for updates, off by default): download, signature check, replace, restart ([updates](updates.md)) | The app tells you; install the new .deb with apt | The app tells you; install the new .rpm with dnf/zypper | Download the new tarball (or rerun the installer script) | `git pull` and rebuild |
| Uninstall | Delete the file (and the menu entry, if you made one) | `sudo apt remove star-org-codex` | `sudo dnf remove star-org-codex` | Delete `~/.local/bin/starorg-strings` | Delete the build |

**Run Remove first.** Uninstalling any flavour leaves the game as it is. To put the game back
to stock, use **Remove** in the app or `starorg-strings remove --all` *before* you uninstall.

### App data (all flavours, shared by the app and the CLI)

| What | Where |
|---|---|
| Config (profiles, settings) | `${XDG_CONFIG_HOME:-~/.config}/starorg-strings/` |
| Data (backups of game files, blueprints, update state `app-update.json`) | `${XDG_DATA_HOME:-~/.local/share}/starorg-strings/` |
| Cache (extracted game text, GameData) | `${XDG_CACHE_HOME:-~/.cache}/starorg-strings/` |
| Webview storage (WebKitGTK, made by Tauri) | `~/.local/share/tools.starorg.strings/`, `~/.cache/tools.starorg.strings/` |

Nothing of ours is stored in the game folder, so the RSI Launcher's "Verify files" cannot
remove it. Delete these folders after uninstalling if you want everything gone; keep the data
folder if you may still need a backup.

## Install the app

### Debian, Ubuntu, Mint, Pop!_OS

```sh
sudo apt install ./StarOrg.Codex_<v>_amd64.deb
```

The `./` matters: without it apt looks for a package of that name online. apt pulls in
`libwebkit2gtk-4.1-0` and `libgtk-3-0`. Ubuntu 22.04 / Debian 12 or newer. Start it from the
menu (**StarOrg Codex**) or run `starorg-strings-app`. Updating is the same command with the
newer file.

### Fedora, openSUSE, Nobara

```sh
sudo dnf install ./StarOrg.Codex-<v>-1.x86_64.rpm      # Fedora, Nobara
sudo zypper install ./StarOrg.Codex-<v>-1.x86_64.rpm   # openSUSE
```

The package needs WebKitGTK 4.1 (`webkit2gtk4.1` on Fedora, `libwebkit2gtk-4_1-0` on
openSUSE); install it by hand if the package manager does not.

### Arch, CachyOS, Manjaro, EndeavourOS (AppImage)

There is no AUR package yet. Use the AppImage:

```sh
sudo pacman -S --needed fuse2          # AppImages need libfuse.so.2
mkdir -p ~/.local/bin
mv ~/Downloads/StarOrg.Codex_<v>_amd64.AppImage ~/.local/bin/StarOrg-Codex.AppImage
chmod +x ~/.local/bin/StarOrg-Codex.AppImage
~/.local/bin/StarOrg-Codex.AppImage
```

Without FUSE 2 the AppImage can still run: `StarOrg-Codex.AppImage --appimage-extract-and-run`
(slower to start; it unpacks to a temp folder each time). The installer script with `--app`
does the steps above and adds a menu entry and icon. Keep the AppImage in a folder you own
(as here): the in-app update replaces the file in place.

On other distros the FUSE 2 package is `libfuse2` (Debian/Ubuntu before 24.04),
`libfuse2t64` (Ubuntu 24.04+) or `fuse-libs` (Fedora).

### Steam Deck (SteamOS) and atomic distros (Bazzite, Silverblue)

Use the AppImage, in **Desktop Mode** on the Deck. The system folders are read-only, so do not
use pacman, apt or dnf there; everything goes into your home folder:

1. Open Konsole and run the installer script line from the top of this page (or download the
   AppImage in the browser, then right-click -> Properties -> Permissions -> **Is executable**).
2. Start **StarOrg Codex** from the application menu (category Utilities), or from Konsole:
   `~/.local/bin/StarOrg-Codex.AppImage`.
3. If it does not start, run it from Konsole with `--appimage-extract-and-run` and see
   [Troubleshooting](#troubleshooting).

## Verify the download (optional)

Each release has `SHA256SUMS` over every file and a build provenance attestation per file.
In the folder with your downloads:

```sh
sha256sum -c --ignore-missing SHA256SUMS
gh attestation verify StarOrg.Codex_<v>_amd64.AppImage -R voidput/starorg-strings
```

`sha256sum` must print `OK` for each file you have. `gh attestation verify` (GitHub CLI) proves
the file was built by this repo's release workflow. The installer script always checks
`SHA256SUMS` and refuses a file that does not match.

## Install the command line tool

None of the app packages include `starorg-strings`; it is the separate CLI tarball. You need it
for the launch hooks and for scripts. The usual mistake is to extract it in Downloads and then
get `starorg-strings: command not found`: the shell only finds programs in the folders listed in
`PATH`.

```sh
tar -xzf starorg-strings-v<v>-linux-x86_64.tar.gz
mkdir -p ~/.local/bin
install -m 755 starorg-strings-v<v>-linux-x86_64/starorg-strings ~/.local/bin/
```

Check that the shell finds it:

```sh
command -v starorg-strings     # should print /home/<you>/.local/bin/starorg-strings
starorg-strings --version
starorg-strings doctor         # installs, paths, casing, other tools' traces; changes nothing
```

If `command -v` prints nothing, `~/.local/bin` is not on your `PATH`. Many distros add it only
at login and only if the folder existed then, so logging out and back in may be enough.
Otherwise add it for your shell (check with `echo $SHELL`):

| Shell | Run once |
|---|---|
| bash | `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc` |
| zsh | `echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc` |
| fish | `fish_add_path ~/.local/bin` |

Then open a new terminal. Programs started from a launcher (Lutris, the LUG hooks) may not
read your shell's startup files, so give the hooks the full path, see
[Launch hooks](#launch-hooks).

## First run: find the game in the Wine prefix

Star Citizen on Linux lives inside a Wine prefix (LUG Helper, Lutris or your own). The app and
the CLI look for it in this order and list every install they find:

1. `--game-dir <folder>` (CLI only; skips the search).
2. `--prefix <prefix>` (CLI only), else the `WINEPREFIX` environment variable.
3. The LUG Helper config: `~/.config/starcitizen-lug/winedir.conf` (the prefix) and
   `gamedir.conf` (the game folder).
4. The default prefixes `~/Games/star-citizen` and `~/games/star-citizen` (LUG Helper and the
   Lutris installer use the first).

For each prefix (a folder with `drive_c/` and `system.reg`) it reads the RSI Launcher's logs
(`drive_c/users/<you>/AppData/Roaming/rsilauncher/logs/log.log` and `log.old.log`), which name
the library folder on any drive, then tries the default
`drive_c/Program Files/Roberts Space Industries/StarCitizen`. Windows paths from the log are
mapped `C:` -> `<prefix>/drive_c`, `Z:` -> `/`, other letters through `<prefix>/dosdevices/`.
Symlinked paths (for example `~/games/star-citizen` pointing to `/mnt/games/star-citizen`) are
resolved and listed once.

If your prefix is somewhere else, point it at the game:

- **App:** in the setup wizard choose **Browse...** and pick the folder that contains `LIVE`
  (or `PTU`, `HOTFIX`, ...), normally
  `<prefix>/drive_c/Program Files/Roberts Space Industries/StarCitizen`. A Lutris prefix is
  shown in Lutris under the game's Configure -> Game options -> Wine prefix. Hidden folders:
  press Ctrl+H in the file picker.
- **App, alternative:** start it with the prefix set, e.g.
  `WINEPREFIX=/path/to/prefix starorg-strings-app` (or the AppImage path).
- **CLI:** `starorg-strings --prefix /path/to/prefix channels`, or
  `starorg-strings --game-dir "/path/to/prefix/drive_c/Program Files/Roberts Space Industries/StarCitizen" channels`.

`starorg-strings channels` lists installs and channels with branch and game version;
`starorg-strings doctor` also says how each install was found.

A few rules the code enforces on Linux:

- **Not as root.** Apply refuses to run as root (`refusing to run as root or elevated`), so
  never use `sudo starorg-strings`.
- **Casing.** The prefix is case-sensitive but Wine is not. We write into the folders as they
  exist (`Data/`, `USER.cfg`) and stop with an error if two spellings exist side by side.
- **Game running.** A running `StarCitizen.exe` in the same prefix and channel blocks an apply
  to that channel (found through `/proc`); quit the game first. Other channels are not blocked.

## Launch hooks

With LUG Helper's launch scripts, `starorg-strings hook install` adds a small block to
`sc-prelaunch.sh` and `sc-postexit.sh` in the prefix (backed up first) so every set-up channel
is re-applied before the game starts and blueprints are read after it exits. Because the
launcher may not have your shell's `PATH`, install the block with the absolute path:

```sh
starorg-strings hook install --bin ~/.local/bin/starorg-strings
starorg-strings hook status
```

`hook uninstall` removes the block again. Use `--prefix` (or `--prelaunch-script` /
`--postexit-script`) when the prefix is not found automatically.

## Build from source

Needs the Rust toolchain from `rust-toolchain.toml` (rustup installs it) and, for the app,
Node 24 and the WebKitGTK development packages (Debian/Ubuntu: `libwebkit2gtk-4.1-dev
libsoup-3.0-dev libgtk-3-dev librsvg2-dev`).

```sh
git clone https://github.com/voidput/starorg-strings && cd starorg-strings
cargo build --release --locked -p starorg-cli        # CLI: target/release/starorg-strings
install -m 755 target/release/starorg-strings ~/.local/bin/
cd app && npm ci && npx tauri build --bundles deb    # app: target/release/bundle/
```

A self-built app shows new versions but never installs them (download only).

## Installer script

`scripts/install-linux.sh` (POSIX `sh`, needs `curl`, `sha256sum`, `tar`):

| Option | Does |
|---|---|
| (none) | Latest release: download the CLI tarball and `SHA256SUMS`, check the checksum, install `starorg-strings` to `${XDG_BIN_HOME:-~/.local/bin}` |
| `--app` | Also the AppImage as `~/.local/bin/StarOrg-Codex.AppImage`, a menu entry `~/.local/share/applications/starorg-strings-app.desktop` and its icon; warns if `libfuse.so.2` is missing |
| `--version X.Y.Z` | That release instead of the latest |
| `--bin-dir DIR` | Install somewhere else |
| `--dry-run` | Resolve the release, check the files exist, print what would happen; writes nothing |
| `--uninstall` | Remove the CLI (with `--app` also the AppImage, menu entry and icon). Settings, backups and the game are kept |

It never uses sudo, refuses to run as root, writes only below your home (temp files go to
`~/.cache` and are removed) and stops if a checksum does not match or the release has no
`SHA256SUMS` yet (a release is complete a few minutes after it appears). When the install folder
is not on your `PATH` it prints the exact line to add for your shell (bash, zsh, fish or
`~/.profile`). `STARORG_REPO=owner/name` installs from a fork.

## Troubleshooting

| Symptom | What to do |
|---|---|
| `starorg-strings: command not found` | The CLI is not in a `PATH` folder. Install it to `~/.local/bin` and add that folder to `PATH` ([above](#install-the-command-line-tool)); or call it by full path |
| Hooks do nothing although the CLI works in a terminal | The launcher has a different `PATH`: `starorg-strings hook install --bin ~/.local/bin/starorg-strings` |
| AppImage: nothing happens, or `dlopen(): error loading libfuse.so.2` | Install FUSE 2 (`fuse2`, `libfuse2`, `libfuse2t64` or `fuse-libs`), or run it with `--appimage-extract-and-run` |
| AppImage: "Permission denied" | `chmod +x StarOrg-Codex.AppImage`; also check the folder is not on a `noexec` mount |
| AppImage crashes at start on a rolling distro (Arch, CachyOS), Wayland errors in the terminal | A library bundled in the AppImage clashes with newer system ones. Try `GDK_BACKEND=x11 ./StarOrg-Codex.AppImage`, or extract it (`--appimage-extract`), delete `squashfs-root/usr/lib/libwayland-*` and run `squashfs-root/AppRun`. Building from source uses only system libraries |
| Window blank, white or black, or flickers (often NVIDIA) | The app already sets WebKitGTK workarounds on NVIDIA (`__NV_DISABLE_EXPLICIT_SYNC=1` on KDE Wayland, `WEBKIT_DISABLE_DMABUF_RENDERER=1` on X11 and Hyprland). If it still fails, start it with `--safe-gpu` (adds `WEBKIT_DISABLE_COMPOSITING_MODE=1` too), or set `WEBKIT_DISABLE_DMABUF_RENDERER=1` yourself. If you set any of these variables yourself, the app leaves all of them alone |
| No icon in the taskbar on Wayland | The window's app id is `starorg-strings-app`; the packages and the installer script ship a `starorg-strings-app.desktop` for it. For a hand-placed AppImage, run the installer with `--app` or create that file |
| .deb: "unable to locate package" | Use `./` before the file name: `sudo apt install ./StarOrg.Codex_<v>_amd64.deb` |
| "No Star Citizen install found" | Start the RSI Launcher in the prefix once (it writes the log we read), or Browse / `--prefix` / `--game-dir` ([first run](#first-run-find-the-game-in-the-wine-prefix)) |
| "refusing to run as root or elevated" | Run it as your normal user, without sudo |
| "the game is running for LIVE" | Quit the game fully (also check for a leftover `StarCitizen.exe` with `pgrep -af StarCitizen`), then apply again |
| A duplicate-case error (`data` and `Data`) | Two folders differ only in case inside the game folder; Wine may use either. Merge them by hand, then apply again. `starorg-strings doctor` reports casing problems |
| In-game text unchanged | The game reads `global.ini` only at start: restart the game. Check that `USER.cfg` in the channel folder has `g_language = english` |
| Blueprints shows your character twice, or missions mark nothing `[Owned]` | A character reset gives a new id. Same account and name are merged on their own (the store is backed up first under the channel's `blueprints/backups/`). Otherwise: Blueprints -> Fix owned blueprints -> Re-scan game logs or Re-import from Smart Citizen into your current character, or choose "Any of my characters"; then re-apply. Command line: `starorg-strings blueprints rescan --dry-run`, `blueprints reimport --character <name> --dry-run` |

Diagnostics (Activity -> Diagnostics) exports a report with user names and home paths replaced.
Attach it to a bug report, together with the output of `starorg-strings doctor`.
