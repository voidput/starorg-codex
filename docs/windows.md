---
title: Windows
tags: [docs]
updated: 2026-11-14
---

# Windows

StarOrg Strings runs natively on Windows 10 (1809 or later) and Windows 11, x64. Two ways to
get it, from the same code:

| | Direct download (GitHub release) | Microsoft Store |
|---|---|---|
| Files | `StarOrg Strings_<v>_x64-setup.exe` (NSIS, per user), `..._x64_en-US.msi`, CLI `starorg-strings-v<v>-windows-x86_64.zip` | Store listing (the release also carries the `.msix` sent to the Store) |
| Signed | Yes once Azure Artifact Signing is set up ([Code signing](#code-signing)); until then unsigned | Signed by Microsoft when published |
| Updates | In the app (Settings -> Network -> Check for updates, off by default), signed with the updater key ([updates](updates.md)) | Only through the Store. The app never checks GitHub and never installs an update ("Updates come from the Microsoft Store.") |
| App data | `%APPDATA%\StarOrg Tools\Strings\` (config, data), `%LOCALAPPDATA%\StarOrg Tools\Strings\cache\` | The same paths as the app sees them, but Windows redirects them into the package folder (`%LOCALAPPDATA%\Packages\<package>\LocalCache\...`), see [Store notes](#store-notes) |
| CLI | In the zip | Not included |
| Uninstall | Settings -> Apps. **Run Remove in the app first**: the uninstaller option that puts the game back to stock (spec 2) is not built yet | Settings -> Apps. **Run Remove in the app first**: a Store package cannot run code when it is uninstalled |

Both need the Microsoft Edge WebView2 runtime, which Windows 10 (updated) and 11 already have.
The NSIS installer downloads it if missing.

## Install (direct download)

1. Download `StarOrg Strings_<version>_x64-setup.exe` from the release page.
2. Run it. It installs for your user only and never asks for administrator rights.
3. **While the builds are unsigned**, SmartScreen shows "Windows protected your PC". Choose
   **More info -> Run anyway**. Check the file first if you like: `SHA256SUMS` on the release
   lists every file, and each file has a build provenance attestation
   (`gh attestation verify <file> -R voidput/starorg-strings`).
4. With **Smart App Control** on (some fresh Windows 11 installs), unsigned programs cannot run
   at all and there is no "Run anyway". Use the Store version, or wait for signed builds. Turning
   Smart App Control off cannot be undone without resetting Windows, so we do not recommend it.
5. Do **not** run StarOrg Strings as administrator. It refuses to change game files when
   elevated (a mistake there could leave files only an administrator can fix).

The portable zip (CLI `starorg-strings.exe`) needs no install; unzip it anywhere.

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

The same as on Linux (spec 7), with these Windows parts:

- **Game running:** the game's process image path (`...\LIVE\Bin64\StarCitizen.exe`) is read
  from the OS, so a running LIVE never blocks an apply to PTU. If Windows will not tell us the
  path (a protected process), the game counts as running for every channel: skipping an apply
  is the safe mistake. `RSI Launcher.exe` only gives a warning; an install in progress (read
  from the launcher log) skips the apply.
- **File locks:** a virus scanner or the search indexer often opens a file we just wrote. Renames
  and deletes retry for about five seconds before giving up.
- **Permissions:** before writing, a probe file checks that the folder is writable. If not, the
  message gives the `icacls` command for that folder. RSI's installer normally makes the library
  writable for your user.
- **USER.cfg:** a plain file on NTFS (no links). We add or keep one `g_language = english` line
  and nothing else; Remove takes our line out again.

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
  Store app is removed. Use **Remove** (or `starorg-strings remove --all` from the zip) first.
- Writes to the game folder go to the real folder (it is not part of the package), so the
  launcher's "Verify files" or a game patch behave as with the direct download.

## Troubleshooting

| Symptom | What to do |
|---|---|
| "No Star Citizen install found" | Start the RSI Launcher once (it writes the log we read), or Browse to the folder that contains `LIVE` |
| "refusing to run as root or elevated" | Start StarOrg Strings normally, not with "Run as administrator"; if UAC is off on your PC, apply cannot run |
| "cannot write in <folder> ... icacls" | Run the shown `icacls` command in a normal (not admin) terminal for your user, or reinstall the game library under your user folder |
| "the game is running for LIVE" | Quit the game fully (check Task Manager for `StarCitizen.exe`), then apply again |
| "the launcher is installing LIVE" | Wait for the launcher's download or verify to finish |
| Apply fails with "Access is denied" or "being used by another process" after a few seconds | A scanner holds the file. Try again; if it repeats, add the game's `Data\Localization` folder to your antivirus exclusions |
| SmartScreen blocks the installer | More info -> Run anyway (unsigned builds), or use the Store version |
| The window stays blank | Install or repair the WebView2 runtime from Microsoft, then restart the app |
| In-game text unchanged | The game reads `global.ini` only at start: restart the game. Check that `USER.cfg` in the channel folder has `g_language = english` |

Diagnostics (Activity -> Diagnostics) exports a report with user names and home paths replaced.
Attach it to a bug report.

## Code signing

Direct downloads are signed with **Azure Artifact Signing** (the service formerly called Trusted
Signing) when the repo has its secrets and variables; without them, releases build unsigned
exactly as before. `.github/actions/windows-signing` checks the configuration, installs
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
6. Next release: the app job log says nothing about "not code-signed", and
   `Get-AuthenticodeSignature '.\StarOrg Strings_<v>_x64-setup.exe'` shows `Valid`.

SmartScreen reputation still builds up per certificate over the first downloads; a signed
build may show the prompt for a while.

## Microsoft Store

The Store build is the Cargo feature `store` of the app (`app/src-tauri/Cargo.toml`): no update
check, no updater plugin, Settings says "Updates come from the Microsoft Store."
release.yml's `store` job builds it with `tauri build --no-bundle --features store`, writes
`AppxManifest.xml` (Windows.Desktop, full trust, logos from `app/src-tauri/icons`) with
`app/scripts/msix.ts`, packs it with the Windows SDK's `makeappx` and uploads
`starorg-strings_<v>_x64_store-submission-unsigned.msix` to the release. It is **unsigned on
purpose**: it is for Store submission only (Partner Center signs what it publishes) and will not
install by double-click. Without the three `MSIX_*` variables the job skips with a notice.

Owner setup:

1. Create a Partner Center developer account (individual or company) at
   <https://partner.microsoft.com/dashboard>.
2. Apps and games -> New product -> MSIX or PWA app -> **reserve the name** "StarOrg Strings".
3. Product management -> **Product identity**: copy *Package/Identity/Name*,
   *Package/Identity/Publisher* (`CN=...`) and *Package/Properties/PublisherDisplayName* into the
   repo **variables** `MSIX_IDENTITY_NAME`, `MSIX_PUBLISHER`, `MSIX_PUBLISHER_DISPLAY_NAME`.
   (Done 2026-11-14 for "StarOrg Strings", Store ID in `MS_STORE_ID`, package family name in
   `MS_STORE_PFN`; steps 1-3 are only needed again for a new account.)
4. Next release: download the `.msix` from the release, start a submission, upload it under
   Packages.
5. Fill in the listing from [store-listing](store-listing.md), the privacy policy URL
   <https://github.com/voidput/starorg-strings/blob/main/docs/privacy.md> ([privacy](privacy.md)),
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

### Automatic submission (optional, off by default)

With the repo variable `MS_STORE_SUBMIT=true`, the store job also sends the package to Partner
Center as a **draft** (`msstore publish --noCommit`); you still review and submit by hand. Needs
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
