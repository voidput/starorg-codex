---
title: Windows
tags: [docs]
updated: 2026-11-15
---

# Windows

StarOrg Codex runs natively on Windows 10 (1809 or later) and Windows 11, x64. Two ways to
get it, from the same code:

| | Direct download (GitHub release) | Microsoft Store |
|---|---|---|
| Files | `StarOrg.Codex_<v>_x64-setup.exe` (NSIS, per user), `..._x64_en-US.msi`, CLI `starorg-codex-v<v>-windows-x86_64.zip` | Store listing (the release also carries the `.msix` sent to the Store) |
| Signed | Yes, with Azure Artifact Signing since 0.0.2: the app exe, both installers and the CLI exe ([Code signing](#code-signing)). Prereleases (`X.Y.Z-beta.1` ...) are unsigned | Signed by Microsoft when published |
| Updates | In the app (Settings -> Network -> Check for updates, off by default), signed with the updater key ([updates](updates.md)) | Only through the Store. The app never checks GitHub and never installs an update ("Updates come from the Microsoft Store.") |
| App data | `%APPDATA%\StarOrg Tools\Codex\` (config, data), `%LOCALAPPDATA%\StarOrg Tools\Codex\cache\` (extracted game text; the Developer mode asset cache in `assets\`, 2 GB by default or a folder you choose; MCP datasets in `mcp\`) | The same paths as the app sees them, but Windows redirects them into the package folder (`%LOCALAPPDATA%\Packages\<package>\LocalCache\...`), see [Store notes](#store-notes) |
| CLI | In the zip | Not included |
| Uninstall | Settings -> Apps. **Run Remove in the app first**: the uninstaller option that puts the game back to stock (spec 2) is not built yet | Settings -> Apps. **Run Remove in the app first**: a Store package cannot run code when it is uninstalled |

All uninstall steps and data folders: [uninstall](uninstall.md).

### Upgrading from 0.2 (the rename)

Until 0.3.0 the data folders were `%APPDATA%\StarOrg Tools\Strings\` and
`%LOCALAPPDATA%\StarOrg Tools\Strings\`, the webview storage `%LOCALAPPDATA%\tools.starorg.strings\`,
the app exe `starorg-strings-app.exe` and the CLI `starorg-strings.exe`. The first start of
0.3.0 (app or CLI) renames each folder to its `Codex` / `tools.starorg.codex` name and says so
once in Activity; when a new folder already exists the old one is left alone and named in
Activity. Quit the old app before the first start: a folder that is in use is not moved (the
old one then stays in use and Activity says why). The installer upgrades in place (same
product name and MSI upgrade code, `c6237751-9461-5af9-a0e2-d71aacef213e`, now pinned in
`tauri.conf.json`). The CLI zip also carries `starorg-strings.exe`, a copy under the old name
for one release that prints a deprecation line.

## Requirements

The summary for all platforms is in the [README](../README.md#system-requirements). On Windows:

- **Windows 10 (1809 or later) or Windows 11, x64.**
- **App (installer or Store):** the Microsoft Edge WebView2 runtime. Windows 11 and updated
  Windows 10 already have it; the NSIS installer downloads it if missing. The Store package does
  not install it (see [Store versions](#store-versions)).
- **CLI zip:** the Microsoft Visual C++ Redistributable (`VCRUNTIME140.dll`, x64). Most PCs
  already have it; if `starorg-codex.exe` says `VCRUNTIME140.dll` was not found, install the
  latest supported redistributable from Microsoft. The app does not need it (it carries its own
  copy of the C runtime).
- **Developer mode, 3D model viewer:** WebGL 2 in WebView2, which means a working GPU driver with
  hardware acceleration. Sounds need nothing extra.

## Install (direct download)

1. Download `StarOrg.Codex_<version>_x64-setup.exe` from the release page.
2. Run it. It installs for your user only and never asks for administrator rights.
3. The installers are signed (Azure Artifact Signing). SmartScreen may still show "Windows
   protected your PC" while the certificate builds up reputation, and always for prereleases,
   which are unsigned. Choose **More info -> Run anyway**. Check the file first if you like: `SHA256SUMS` on the release
   lists every file, and each file has a build provenance attestation
   (`gh attestation verify <file> -R voidput/starorg-codex`).
4. With **Smart App Control** on (some fresh Windows 11 installs), unsigned programs cannot run
   at all and there is no "Run anyway". Release builds are signed; use a release, not a
   prerelease. Turning Smart App Control off cannot be undone without resetting Windows, so we do
   not recommend it.
5. Do **not** run StarOrg Codex as administrator. It refuses to change game files when
   elevated (a mistake there could leave files only an administrator can fix).

The portable zip (CLI `starorg-codex.exe`) needs no install; unzip it anywhere. To run it as
`starorg-codex` from any terminal, put it in a folder on your user `PATH`, e.g. in PowerShell:

```powershell
$bin = "$env:LOCALAPPDATA\Programs\starorg-codex"
New-Item -ItemType Directory -Force $bin | Out-Null
Copy-Item .\starorg-codex.exe $bin
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $bin) { [Environment]::SetEnvironmentVariable('Path', "$path;$bin", 'User') }
```

Open a new terminal, then check with `starorg-codex --version` and `starorg-codex doctor`
(installs, paths, other tools' traces; changes nothing). Without this, call it by its full path.

## First run

The setup wizard looks for the game in this order (spec 10.1):

1. The RSI Launcher's logs, `%APPDATA%\rsilauncher\logs\log.log` and `log.old.log`: every
   install and every launch writes the library folder there, on any drive.
2. The usual folders on every fixed or removable drive: `<drive>:\Program Files\Roberts Space
   Industries\StarCitizen` (the launcher default), `<drive>:\Roberts Space
   Industries\StarCitizen`, `<drive>:\Games\Roberts Space Industries\StarCitizen`,
   `<drive>:\StarCitizen` and `<drive>:\<any folder>\StarCitizen`. Network and optical drives
   are skipped (they can hang a scan).
3. Otherwise **Browse...** to the folder that contains `LIVE` (or `PTU`, `HOTFIX`, ...).

The launcher's settings file also names the library, but it is encrypted, so we do not read it.
Paths compare without case, as Windows does. Long paths (over 260 characters) work.

## How apply works on Windows

The same as on Linux (spec 7, [linux](linux.md)), with these Windows parts:

- **Game running:** the game's process image path (`...\LIVE\Bin64\StarCitizen.exe`) is read
  from the OS, so a running LIVE never blocks an apply to PTU. If Windows will not tell us the
  path (a protected process), the game counts as running for every channel: skipping an apply
  is the safe mistake. `RSI Launcher.exe` only gives a warning; an install in progress (read
  from the launcher log) skips the apply.
- **File locks:** a virus scanner or the search indexer often opens a file we just wrote. Renames
  and deletes retry for about five seconds on "being used by another process" before giving up.
  "Access is denied" gets only about half a second of retries, because a read-only folder
  returns it too and the `icacls` hint below should not wait.
- **Permissions:** before writing, a probe file checks that the folder is writable. If not, the
  message gives the `icacls` command for that folder. RSI's installer normally makes the library
  writable for your user.
- **USER.cfg:** a plain file on NTFS (no links). We add or keep one `g_language = english` line
  and nothing else; Remove takes our line out again.

## After playing: blueprints and re-apply

Linux has launch hooks that do this after every session ([linux](linux.md)); Windows has no
equivalent in the RSI Launcher, so you trigger it yourself or let Task Scheduler do it.

**In the app:** quit the game, open **Blueprints** (it reads the new `Received Blueprint` lines
from `Game.log` and `logbackups`), then **Home -> Apply**: a channel with new blueprints shows
**Stale** ("owned blueprints"). After a character reset: Blueprints -> **Fix owned blueprints**
-> **Re-scan game logs**, then apply.

**From a terminal** (the CLI zip, see [Install](#install-direct-download)):

```powershell
starorg-codex hook postexit
```

It reads the new log lines, then re-applies every channel that is stale, and always exits 0 (a
skipped apply is reported, not an error). Step by step instead: `starorg-codex blueprints scan`,
then `starorg-codex apply --all --if-stale`. Only `hook install` / `hook uninstall` (editing the
LUG Helper scripts) are Linux only.

**Automatically with Task Scheduler** (optional). Runs it when you log on and then every hour;
while the game runs it still reads the logs but skips the write, and a later run applies.

1. Task Scheduler -> **Create Task...** (not *Basic Task*, which cannot repeat).
2. **General:** name `StarOrg Codex blueprints`; *Run only when user is logged on*; leave
   **Run with highest privileges** off (apply refuses to run elevated).
3. **Triggers -> New:** *At log on* of your user; tick **Repeat task every 1 hour** for
   **Indefinitely**.
4. **Actions -> New:** *Start a program*; Program: the full path of `starorg-codex.exe` (e.g.
   `%LOCALAPPDATA%\Programs\starorg-codex\starorg-codex.exe`); Arguments: `hook postexit --quiet`.
5. **Settings:** *If the task is already running: Do not start a new instance*.

Or the same from PowerShell (as your normal user, not as administrator):

```powershell
$exe = "$env:LOCALAPPDATA\Programs\starorg-codex\starorg-codex.exe"
$action = New-ScheduledTaskAction -Execute $exe -Argument 'hook postexit --quiet'
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$trigger.Repetition = (New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Hours 1)).Repetition
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 10)
Register-ScheduledTask -TaskName 'StarOrg Codex blueprints' -Action $action -Trigger $trigger -Settings $settings -RunLevel Limited
```

Check it with `Start-ScheduledTask 'StarOrg Codex blueprints'` and the app's **Activity** log;
remove it with `Unregister-ScheduledTask 'StarOrg Codex blueprints' -Confirm:$false`. The task
runs a console window for a moment each hour; that is the CLI doing its run.

The Microsoft Store app has no CLI, and Windows keeps its settings and blueprint data in the
package's private folder ([Store notes](#store-notes)), where the CLI does not look. So the task
pairs with the direct-download app (installer or MSI), which shares its data with the CLI; with
the Store app, use the in-app steps above.

## Updates per flavour

- **Direct download:** off until you turn on Settings -> Network -> Check for updates. Then the
  NSIS and MSI installs update in the app (download, signature check against our key, install,
  restart). The portable CLI is updated by downloading the new zip. Details: [updates](updates.md).
- **Microsoft Store:** Settings shows "Updates come from the Microsoft Store." and nothing else;
  the app makes no update request at all. The Store installs new versions.

## Store notes

- Desktop apps from the Store run with a package identity, and Windows redirects the app's writes
  to `%APPDATA%` and `%LOCALAPPDATA%` into the package's private folder. So the Store version and a
  direct-download copy (or the CLI) keep **separate** settings and backups, and uninstalling the
  Store version deletes its backups. Game files are not redirected: they are written in place.
- Because uninstalling cannot run our code, the game keeps the enhanced `global.ini` after the
  Store app is removed. Use **Remove** (or `starorg-codex remove --all` from the zip) first.
- Writes to the game folder go to the real folder (it is not part of the package), so the
  launcher's "Verify files" or a game patch behave as with the direct download.

## Troubleshooting

| Symptom | What to do |
|---|---|
| "No Star Citizen install found" | Start the RSI Launcher once (it writes the log we read), or Browse to the folder that contains `LIVE` |
| "refusing to run as root or elevated" | Start StarOrg Codex normally, not with "Run as administrator"; if UAC is off on your PC, apply cannot run |
| "cannot write in <folder> ... icacls" | Run the shown `icacls` command in a normal (not admin) terminal for your user, or reinstall the game library under your user folder |
| "the game is running for LIVE" | Quit the game fully (check Task Manager for `StarCitizen.exe`), then apply again |
| "the launcher is installing LIVE" | Wait for the launcher's download or verify to finish |
| Apply fails with "Access is denied" or "being used by another process" after a few seconds | A scanner holds the file. Try again; if it repeats, add the game's `Data\Localization` folder to your antivirus exclusions |
| SmartScreen blocks the installer | More info -> Run anyway. Releases are signed, but SmartScreen can still warn while the certificate is new; prereleases are unsigned |
| The window stays blank | Install or repair the WebView2 runtime from Microsoft, then restart the app |
| In-game text unchanged | The game reads `global.ini` only at start: restart the game. Check that `USER.cfg` in the channel folder has `g_language = english` |
| `starorg-codex` is not recognized | The CLI folder is not on `PATH`: see [Install](#install-direct-download), or call it by full path |
| Developer mode or the MCP server refuses a dataset ("needs about 4.1 GB, more than the ... memory limit allows") | Raise Settings -> Advanced -> Memory limit (default 5 GB or half the RAM). Each worker stops itself at 90% of the limit; Windows has no hard cap |
| Developer mode: the 3D view says "The webview has no WebGL here (...)" | The model viewer needs WebGL 2. Update the GPU driver and check that hardware acceleration is not turned off for the GPU. "Export .glb" works either way; open the file in Blender |
| Developer mode: a sound will not play ("The app's webview could not play…") | WebView2 plays the converted sound itself and needs no extra codecs; repair the WebView2 runtime and restart the app. Exporting the sound and playing it in another player works either way |
| Blueprints shows your character twice, or missions mark nothing `[Owned]` | A character reset gives a new id. Same account and name are merged on their own (the store is backed up first under the channel's `blueprints\backups\`). Otherwise: Blueprints -> Fix owned blueprints -> Re-scan game logs or Re-import from Smart Citizen into your current character, or choose "Any of my characters"; then re-apply. Command line: `starorg-codex blueprints rescan --dry-run`, `blueprints reimport --character <name> --dry-run` |

Diagnostics (Activity -> Diagnostics) exports a report with user names and home paths replaced.
Attach it to a bug report.

## Code signing

Direct downloads are signed with **Azure Artifact Signing** (the service formerly called Trusted
Signing), live since 0.0.2. The release job signs every plain `X.Y.Z` release; prereleases
(`X.Y.Z-beta.1`, `-rc.1`) and a fork without the secrets and variables below build unsigned. `.github/actions/windows-signing` checks the configuration, installs
`artifact-signing-cli` (pinned) and gives Tauri a `bundle.windows.signCommand`; Tauri signs the
app exe, the NSIS installer and uninstaller and the MSI. The CLI exe in the zip is signed too.

Owner setup (once):

1. Azure subscription -> create an **Artifact Signing account** (pick a region; its endpoint is
   e.g. `https://eus.codesigning.azure.net`).
2. Complete **identity validation** (Individual or Organization; individuals need a government
   ID). Check Microsoft's current eligibility rules for your country first: they have changed
   over time and decide whether this route works at all (spec Q3).
3. Create a **certificate profile** (Public Trust).
4. Create an **App registration** (Entra ID) with a client secret, and give it the role
   *Artifact Signing Certificate Profile Signer* on the account.
5. Add to the repo (Settings -> Secrets and variables -> Actions):
   - secrets `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID`;
   - variables `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE`.
6. Next release: the app job log says nothing about "not code-signed". The jobs check the
   signatures themselves (`Get-AuthenticodeSignature` must say `Valid` for the CLI exe, the app
   exe, the NSIS installer and the MSI, or the job fails), and the signing step finds the
   newest `signtool.exe` of the runner's Windows SDK and the Azure CLI itself, so a runner image
   update does not break it. A spot check on a downloaded file is still worth it once:
   `Get-AuthenticodeSignature '.\StarOrg.Codex_<v>_x64-setup.exe'` shows `Valid`.

SmartScreen reputation still builds up per certificate over the first downloads; a signed
build may show the prompt for a while.

## Microsoft Store

The Store build is the Cargo feature `store` of the app (`app/src-tauri/Cargo.toml`): no update
check, no updater plugin, Settings says "Updates come from the Microsoft Store."
release.yml's `store` job builds it with `tauri build --no-bundle --features store`, writes
`AppxManifest.xml` (Windows.Desktop, full trust, logos from `app/src-tauri/icons`) with
`app/scripts/msix.ts`, packs it with the Windows SDK's `makeappx` and uploads
`starorg-codex_<v>_x64_store-submission-unsigned.msix` to the release. It is **unsigned on
purpose**: it is for Store submission only (Partner Center signs what it publishes) and will not
install by double-click. Without the three `MSIX_*` variables the job skips with a notice.

### Store versions

Partner Center rejects a package whose version starts with 0 (Microsoft Learn, "App package
requirements for MSIX app": each of the four parts is 0-65535, "except for the first section,
which cannot be 0"). Decision (2026-11-15): the package version is the app version plus `.0`,
with no offset, so the Store, Settings -> Apps and the About box show the same number; therefore
**0.x releases get no Store package** and the first Store release is app version **1.0.0**.
For a 0.x release the store job ends with the notice "No Store package for 0.x.y" and the rest
of the release is unaffected; `app/scripts/msix.ts` refuses a 0.x version too
(`FIRST_STORE_VERSION`, tested in `app/tests/msix.test.ts`).

To start Store releases, cut 1.0.0 with release-please: an empty commit on `main` with the body
`Release-As: 1.0.0` (`git commit --allow-empty -m "chore: release 1.0.0" -m "Release-As: 1.0.0"`),
then merge the release PR. Later versions only ever go up, which the Store also requires.

The package declares no WebView2 dependency (the Store has no package for it). Windows 11 and
current Windows 10 have the WebView2 runtime built in; on an old Windows 10 without it the
Store app opens a blank window (see Troubleshooting). Mention this under the listing's system
requirements.

Owner setup:

1. Create a Partner Center developer account (individual or company) at
   <https://partner.microsoft.com/dashboard>.
2. Apps and games -> New product -> MSIX or PWA app -> **reserve the name** "StarOrg Codex".
3. Product management -> **Product identity**: copy *Package/Identity/Name*,
   *Package/Identity/Publisher* (`CN=...`) and *Package/Properties/PublisherDisplayName* into the
   repo **variables** `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, `MSIX_PUBLISHER_DISPLAY_NAME`.
   (Done 2026-11-14 for "StarOrg Codex", Store ID in `MS_STORE_ID`, package family name in
   `MS_STORE_PFN`; steps 1-3 are only needed again for a new account.)
4. First release with version **1.0.0** or later (see [Store versions](#store-versions)):
   download the `.msix` from the release, start a submission, upload it under Packages.
5. Fill in the listing from [store-listing](store-listing.md), the privacy policy URL
   <https://github.com/voidput/starorg-codex/blob/main/docs/privacy.md> ([privacy](privacy.md)),
   the age rating questionnaire (below), pricing (free) and availability.
6. The `runFullTrust` capability needs a short justification in the submission: "Desktop app
   (Tauri/WebView2) that edits the localization file of a game the user has installed, in a
   folder the user chooses."
7. Certification notes for the tester: "Needs a Star Citizen install to apply. Without one, the
   setup wizard shows 'No Star Citizen install found'; the app makes no network request unless
   a switch under Settings -> Network is turned on."

Age rating (IARC questionnaire): a utility, not a game; no violence, sexual content, gambling or
drugs shown by the app itself; no user-to-user communication, no sharing of location or personal
information, no purchases. That normally gives the lowest rating (e.g. PEGI 3 / ESRB Everyone).
The game's own text (weapon names and so on) is shown only from the user's install.

Policy points to keep in mind for the listing: say plainly that the app changes files of a
third-party game and is not affiliated with Cloud Imperium Games or Roberts Space Industries; do
not use the Star Citizen logo or game art in the screenshots or logos.

### Automatic submission (on since 1.1.0)

With the repo variable `MS_STORE_SUBMIT=true` (set), the store job also publishes the package
(`msstore publish`): a new submission with this package goes to certification and is published
when it passes, usually within a few hours to 3 days. Listing text, screenshots and age rating
carry over from the last submission; change them in Partner Center when needed. The API may only
change submissions it created: while a submission started by hand is in progress, the step fails
("delete the current in-progress submission"); finish or delete it, then re-run the failed job
(`gh run rerun <run id> --failed`). Needs
an Entra app with Partner Center API access (Partner Center -> Account settings -> User
management -> Microsoft Entra applications) and:

- secrets `MS_STORE_TENANT_ID`, `MS_STORE_SELLER_ID`, `MS_STORE_CLIENT_ID`, `MS_STORE_CLIENT_SECRET`;
- variable `MS_STORE_ID` (the Store ID, e.g. `9N...`).

A missing one fails that step with a clear error; the `.msix` is already on the release by then.
Check the first run against Microsoft's current `msstore` CLI docs, since its flags change.

## Testing on Windows

CI runs `cargo test --workspace --locked` on `windows-latest`, including the native apply flow
(`crates/starorg-cli/tests/native_flow.rs`), the real-backend tests and the `cfg(windows)`
discovery tests. GitHub's runners are administrators, so the tests switch on a test-only override
(`STARORG_TEST_ALLOW_ELEVATED`, compiled in only with the `test-hooks` feature that only
dev-dependencies enable; CI checks no release dependency tree has it).

Before a release is called Windows-ready, a person runs [windows-test-checklist](windows-test-checklist.md)
on a real install.

Locally on Linux, the Windows unit and CLI tests can be tried under Wine (not a substitute for
CI or a real machine): `rustup target add x86_64-pc-windows-gnu`, then
`cargo test -p starorg-core -p starorg-cli --target x86_64-pc-windows-gnu --no-run` and run the
test `.exe` files with `wine` in a **throwaway** `WINEPREFIX` (never the game's prefix).
