/**
 * Microsoft Store package (docs/windows.md, "Microsoft Store"): writes AppxManifest.xml and
 * stages the package folder for `makeappx pack`. release.yml runs this on windows-latest after
 * `tauri build --features store --no-bundle`; the .msix is unsigned on purpose (the Store
 * signs what it publishes).
 *
 *   node scripts/msix.ts stage --exe <starorg-codex-app.exe> --version <x.y.z> --out <dir>
 *
 * Identity comes from the repo variables MSIX_IDENTITY_NAME, MSIX_PUBLISHER (CN=...) and
 * MSIX_PUBLISHER_DISPLAY_NAME (Partner Center -> the app -> Product identity). Pure functions
 * below are covered by tests/msix.test.ts (golden manifest: tests/fixtures/AppxManifest.golden.xml).
 * Erasable TypeScript only, so plain `node` runs it (type stripping).
 */
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface MsixIdentity {
  /** Package/Identity/Name, e.g. `12345VoidPut.StarOrgCodex`. */
  name: string;
  /** Package/Identity/Publisher, e.g. `CN=1A2B3C4D-...`. */
  publisher: string;
  /** Shown in the Store and in Settings -> Apps. */
  publisherDisplayName: string;
}

export interface ManifestInput extends MsixIdentity {
  /** Four-part package version, see {@link msixVersion}. */
  version: string;
  /** File name of the app exe inside the package root. */
  executable: string;
}

/** The logos the manifest names, each copied from app/src-tauri/icons at exactly this size
 *  (`tauri icon` makes them; tests/msix.test.ts checks the sizes). */
export const LOGOS: ReadonlyArray<{ file: string; width: number; height: number }> = [
  { file: 'StoreLogo.png', width: 50, height: 50 },
  { file: 'Square44x44Logo.png', width: 44, height: 44 },
  { file: 'Square71x71Logo.png', width: 71, height: 71 },
  { file: 'Square150x150Logo.png', width: 150, height: 150 },
];

export const DISPLAY_NAME = 'StarOrg Codex';
export const DESCRIPTION = 'Star Citizen text enhancements: component tags, ship and weapon stats, mission info.';
/** Windows 10 1809: the oldest build the Tauri runtime and MSIX desktop apps support well. */
export const MIN_VERSION = '10.0.17763.0';
export const MAX_VERSION_TESTED = '10.0.26100.0';

/** First app version the Store accepts. Partner Center rejects a package whose first version
 *  part is 0 ("App package requirements for MSIX app": each part 0-65535 except the first,
 *  which cannot be 0), so 0.x releases get no Store package (release.yml's store job skips them
 *  with a notice) and the first Store release is app version 1.0.0 (docs/windows.md). The
 *  package version is the app version, no offset, so Settings -> Apps and the About box agree. */
export const FIRST_STORE_VERSION = '1.0.0';

/** Why `semver` cannot become a Store package version, or null. */
export function storeVersionProblem(semver: string): string | null {
  const m = /^v?(\d+)\.(\d+)\.(\d+)(?:[-+].*)?$/.exec(semver.trim());
  if (!m) return `not a semver version: ${semver}`;
  const parts = m.slice(1, 4).map(Number);
  if (parts.some((n) => n > 65535)) return `version parts must be at most 65535: ${semver}`;
  if (parts[0] === 0) {
    return `version ${semver}: the Microsoft Store rejects package versions starting with 0; Store packages start at ${FIRST_STORE_VERSION}`;
  }
  return null;
}

/** `1.3.1` -> `1.3.1.0`. Pre-release and build parts are dropped (the Store has none), and the
 *  fourth part is always 0: the Store reserves it. Throws for a 0.x version
 *  ({@link storeVersionProblem}). */
export function msixVersion(semver: string): string {
  const problem = storeVersionProblem(semver);
  if (problem) throw new Error(problem);
  const m = /^v?(\d+)\.(\d+)\.(\d+)/.exec(semver.trim())!;
  return `${m.slice(1, 4).map(Number).join('.')}.0`;
}

/** Why an identity is unusable, or null. Mirrors the AppxManifest schema rules. */
export function identityProblem(id: Partial<MsixIdentity>): string | null {
  if (!id.name || !id.publisher || !id.publisherDisplayName) {
    return 'MSIX_IDENTITY_NAME, MSIX_PUBLISHER and MSIX_PUBLISHER_DISPLAY_NAME must all be set';
  }
  if (!/^[A-Za-z0-9.-]{3,50}$/.test(id.name)) return `MSIX_IDENTITY_NAME "${id.name}": 3-50 letters, digits, dots or dashes`;
  if (!/^CN=[^,=]+(,\s*[A-Z]+=[^,=]+)*$/.test(id.publisher)) return `MSIX_PUBLISHER "${id.publisher}" must be a distinguished name starting with CN=`;
  if (id.publisher.length > 8192) return 'MSIX_PUBLISHER is too long';
  if (id.publisherDisplayName.length > 256) return 'MSIX_PUBLISHER_DISPLAY_NAME is too long';
  return null;
}

const xml = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&apos;');

/** AppxManifest.xml for a full-trust desktop app (Windows.Desktop, runFullTrust). CRLF line
 *  ends, like the files Visual Studio writes; the golden test compares bytes. */
export function renderManifest(m: ManifestInput): string {
  const problem = identityProblem(m);
  if (problem) throw new Error(problem);
  if (!/^[1-9]\d*\.\d+\.\d+\.0$/.test(m.version)) throw new Error(`package version must be x.y.z.0 with x >= 1: ${m.version}`);
  if (!/^[^\\/:*?"<>|]+\.exe$/i.test(m.executable)) throw new Error(`not an exe file name: ${m.executable}`);
  const lines = [
    '<?xml version="1.0" encoding="utf-8"?>',
    '<!-- Generated by app/scripts/msix.ts for the Microsoft Store build. Do not edit by hand. -->',
    '<Package',
    '  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"',
    '  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"',
    '  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"',
    '  IgnorableNamespaces="uap rescap">',
    `  <Identity Name="${xml(m.name)}" Publisher="${xml(m.publisher)}" Version="${m.version}" ProcessorArchitecture="x64" />`,
    '  <Properties>',
    `    <DisplayName>${xml(DISPLAY_NAME)}</DisplayName>`,
    `    <PublisherDisplayName>${xml(m.publisherDisplayName)}</PublisherDisplayName>`,
    '    <Logo>Assets\\StoreLogo.png</Logo>',
    `    <Description>${xml(DESCRIPTION)}</Description>`,
    '  </Properties>',
    '  <Dependencies>',
    `    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="${MIN_VERSION}" MaxVersionTested="${MAX_VERSION_TESTED}" />`,
    '  </Dependencies>',
    '  <Resources>',
    '    <Resource Language="en-us" />',
    '  </Resources>',
    '  <Applications>',
    `    <Application Id="StarOrgCodex" Executable="${xml(m.executable)}" EntryPoint="Windows.FullTrustApplication">`,
    '      <uap:VisualElements',
    `        DisplayName="${xml(DISPLAY_NAME)}"`,
    `        Description="${xml(DESCRIPTION)}"`,
    '        BackgroundColor="transparent"',
    '        Square150x150Logo="Assets\\Square150x150Logo.png"',
    '        Square44x44Logo="Assets\\Square44x44Logo.png">',
    '        <uap:DefaultTile Square71x71Logo="Assets\\Square71x71Logo.png" />',
    '      </uap:VisualElements>',
    '    </Application>',
    '  </Applications>',
    '  <Capabilities>',
    '    <rescap:Capability Name="runFullTrust" />',
    '  </Capabilities>',
    '</Package>',
    '',
  ];
  return lines.join('\r\n');
}

/** Width and height from a PNG's IHDR chunk. */
export function pngSize(bytes: Uint8Array): { width: number; height: number } {
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (bytes.length < 24 || sig.some((b, i) => bytes[i] !== b)) throw new Error('not a PNG file');
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: v.getUint32(16), height: v.getUint32(20) };
}

/** Package root: the exe, Assets/ with the logos, AppxManifest.xml. */
export function stage(opts: ManifestInput & { exePath: string; iconsDir: string; out: string }): string {
  mkdirSync(join(opts.out, 'Assets'), { recursive: true });
  copyFileSync(opts.exePath, join(opts.out, opts.executable));
  for (const l of LOGOS) {
    const src = join(opts.iconsDir, l.file);
    const size = pngSize(readFileSync(src));
    if (size.width !== l.width || size.height !== l.height) {
      throw new Error(`${src} is ${size.width}x${size.height}, the manifest needs ${l.width}x${l.height}; regenerate with \`npx tauri icon\``);
    }
    copyFileSync(src, join(opts.out, 'Assets', l.file));
  }
  const manifest = join(opts.out, 'AppxManifest.xml');
  writeFileSync(manifest, renderManifest(opts));
  return manifest;
}

function arg(argv: string[], name: string): string | undefined {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 ? argv[i + 1] : undefined;
}

function main(argv: string[]): void {
  if (argv[0] !== 'stage') {
    console.error('usage: node scripts/msix.ts stage --exe <path> --version <x.y.z> --out <dir> [--icons <dir>]');
    process.exit(2);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const exePath = arg(argv, 'exe');
  const version = arg(argv, 'version');
  const out = arg(argv, 'out');
  if (!exePath || !version || !out) return main([]);
  const manifest = stage({
    name: process.env.MSIX_IDENTITY_NAME ?? '',
    publisher: process.env.MSIX_PUBLISHER ?? '',
    publisherDisplayName: process.env.MSIX_PUBLISHER_DISPLAY_NAME ?? '',
    version: msixVersion(version),
    executable: basename(exePath),
    exePath,
    iconsDir: arg(argv, 'icons') ?? resolve(here, '../src-tauri/icons'),
    out,
  });
  console.log(`staged ${out} (${manifest})`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(`msix: ${(e as Error).message}`);
    process.exit(1);
  }
}
