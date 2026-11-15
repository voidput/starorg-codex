#!/bin/sh
# Install StarOrg Codex on Linux for the current user: the `starorg-codex` CLI (plus a
# `starorg-strings` link, its pre-0.3.0 name, for one release) and, with --app, the AppImage
# with a menu entry. Downloads from GitHub releases with curl,
# checks every file against the release's SHA256SUMS, never uses sudo, writes only below
# $HOME (or the XDG dirs you point it at). See docs/linux.md.
#
#   sh install-linux.sh [--version X.Y.Z] [--app] [--bin-dir DIR] [--dry-run]
#   sh install-linux.sh --uninstall [--app]
#
#   curl -fsSL https://raw.githubusercontent.com/voidput/starorg-codex/main/scripts/install-linux.sh | sh -s -- --app

set -eu

REPO="${STARORG_REPO:-voidput/starorg-codex}"
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CACHE_HOME="${XDG_CACHE_HOME:-$HOME/.cache}"
APP_ID="starorg-codex-app"
# Names before 0.3.0: the CLI keeps a link under its old name for one release; the old menu
# entry and icon are replaced.
LEGACY_CLI="starorg-strings"
LEGACY_APP_ID="starorg-strings-app"
APPIMAGE_NAME="StarOrg-Codex.AppImage"

VERSION=""
WANT_APP=0
UNINSTALL=0
DRY_RUN=0
WORK=""

usage() {
    cat <<EOF
Install StarOrg Codex for the current user (no sudo).

Usage: install-linux.sh [options]

  --version X.Y.Z   Install this release (default: the latest release)
  --app             Also install the desktop app (AppImage, menu entry, icon)
  --bin-dir DIR     Where to put the files (default: \${XDG_BIN_HOME:-\$HOME/.local/bin},
                    now $BIN_DIR)
  --dry-run         Show what would happen; download nothing but the checksum list
  --uninstall       Remove the CLI (and with --app the AppImage, menu entry, icon).
                    Settings, backups and the game are left alone.
  -h, --help        This help

Environment: STARORG_REPO (default voidput/starorg-codex), XDG_BIN_HOME,
XDG_DATA_HOME, XDG_CACHE_HOME.
EOF
}

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# Run a command, or only print it with --dry-run.
run() {
    if [ "$DRY_RUN" = 1 ]; then
        say "  would run: $*"
    else
        "$@"
    fi
}

remove_file() {
    if [ -e "$1" ] || [ -L "$1" ]; then
        if [ "$DRY_RUN" = 1 ]; then say "  would remove $1"; else rm -f "$1" && say "removed $1"; fi
    fi
}

cleanup() {
    if [ -n "$WORK" ] && [ -d "$WORK" ]; then
        rm -rf "$WORK"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

while [ $# -gt 0 ]; do
    case "$1" in
    --version)
        [ $# -ge 2 ] || die "--version needs a value, e.g. --version 0.1.0"
        VERSION="$2"
        shift 2
        ;;
    --version=*)
        VERSION="${1#--version=}"
        shift
        ;;
    --bin-dir)
        [ $# -ge 2 ] || die "--bin-dir needs a directory"
        BIN_DIR="$2"
        shift 2
        ;;
    --bin-dir=*)
        BIN_DIR="${1#--bin-dir=}"
        shift
        ;;
    --app) WANT_APP=1; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) die "unknown option: $1 (see --help)" ;;
    esac
done

[ -n "${HOME:-}" ] || die "HOME is not set"
case "$BIN_DIR" in
/*) ;;
*) die "--bin-dir must be an absolute path: $BIN_DIR" ;;
esac
if [ "$(id -u)" = 0 ]; then
    die "do not run this as root: it installs for your user only, and StarOrg Codex refuses to change game files as root"
fi

CLI_DEST="$BIN_DIR/starorg-codex"
LEGACY_CLI_DEST="$BIN_DIR/$LEGACY_CLI"
APP_DEST="$BIN_DIR/$APPIMAGE_NAME"
DESKTOP_DEST="$DATA_HOME/applications/$APP_ID.desktop"
ICON_DEST="$DATA_HOME/icons/hicolor/128x128/apps/$APP_ID.png"
LEGACY_DESKTOP="$DATA_HOME/applications/$LEGACY_APP_ID.desktop"
LEGACY_ICON="$DATA_HOME/icons/hicolor/128x128/apps/$LEGACY_APP_ID.png"

# The exact line that puts BIN_DIR on PATH for the user's login shell.
path_hint() {
    case "$BIN_DIR" in
    "$HOME"/*) shown="\$HOME${BIN_DIR#"$HOME"}" ;;
    *) shown="$BIN_DIR" ;;
    esac
    shell_name="$(basename "${SHELL:-sh}")"
    case "$shell_name" in
    fish)
        say "  fish_add_path $shown"
        say "(run it once in fish; it is remembered)"
        ;;
    zsh)
        say "  echo 'export PATH=\"$shown:\$PATH\"' >> ~/.zshrc"
        say "then open a new terminal (or run: exec zsh)"
        ;;
    bash)
        say "  echo 'export PATH=\"$shown:\$PATH\"' >> ~/.bashrc"
        say "then open a new terminal (or run: exec bash)"
        ;;
    *)
        say "  echo 'export PATH=\"$shown:\$PATH\"' >> ~/.profile"
        say "then log out and back in"
        ;;
    esac
}

check_path() {
    case ":${PATH:-}:" in
    *":$BIN_DIR:"* | *":$BIN_DIR/:"*) return 0 ;;
    esac
    say ""
    say "*** $BIN_DIR is not on your PATH, so 'starorg-codex' will say \"command not found\"."
    say "Add it (detected shell: $(basename "${SHELL:-sh}")):"
    say ""
    path_hint
    say ""
    say "Or call it by its full path: $CLI_DEST"
}

if [ "$UNINSTALL" = 1 ]; then
    say "Before uninstalling, put the game back to stock if you applied anything:"
    say "  starorg-codex remove --all      (or Remove in the app)"
    say ""
    remove_file "$CLI_DEST"
    remove_file "$LEGACY_CLI_DEST"
    if [ "$WANT_APP" = 1 ]; then
        for f in "$APP_DEST" "$DESKTOP_DEST" "$ICON_DEST" "$LEGACY_DESKTOP" "$LEGACY_ICON"; do
            remove_file "$f"
        done
        if command -v update-desktop-database >/dev/null 2>&1; then
            run update-desktop-database "$DATA_HOME/applications" || true
        fi
    fi
    say ""
    say "Kept (delete by hand if you want them gone; backups of game files live here):"
    say "  ${XDG_CONFIG_HOME:-$HOME/.config}/starorg-codex"
    say "  $DATA_HOME/starorg-codex"
    say "  $CACHE_HOME/starorg-codex"
    exit 0
fi

command -v curl >/dev/null 2>&1 || die "curl is required (install it with your package manager)"
command -v sha256sum >/dev/null 2>&1 || die "sha256sum is required (coreutils)"
command -v tar >/dev/null 2>&1 || die "tar is required"

arch="$(uname -m)"
[ "$arch" = x86_64 ] || die "only x86_64 builds are published (this machine is $arch); build from source instead (docs/linux.md)"

# HTTPS only, fail on HTTP errors, follow redirects (release assets live on a CDN).
dl() { curl -fsSL --proto '=https' --tlsv1.2 --retry 3 "$@"; }

# Resolve the tag: the latest release via github.com's redirect (no API rate limit).
if [ -n "$VERSION" ]; then
    TAG="v${VERSION#v}"
else
    latest="$(dl -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")" ||
        die "cannot reach github.com/$REPO/releases (offline, or no release yet?)"
    TAG="${latest##*/}"
    case "$TAG" in
    v[0-9]*) ;;
    *) die "could not find the latest release of $REPO (got $latest)" ;;
    esac
fi
BASE="https://github.com/$REPO/releases/download/$TAG"
say "StarOrg Codex $TAG from $REPO"

sums="$(dl "$BASE/SHA256SUMS")" ||
    die "release $TAG has no SHA256SUMS (wrong version, or the release is still being built: try again in a few minutes)"

in_sums() { printf '%s\n' "$sums" | awk -v n="$1" '$2 == n || $2 == "*" n { f = 1 } END { exit !f }'; }

# Releases before 0.3.0 (--version 0.2.0) ship starorg-strings-<tag>-...: installed under
# that name, without the link, as they were.
CLI_NAME="starorg-codex"
CLI_ASSET="$CLI_NAME-$TAG-linux-x86_64.tar.gz"
if ! in_sums "$CLI_ASSET" && in_sums "$LEGACY_CLI-$TAG-linux-x86_64.tar.gz"; then
    CLI_NAME="$LEGACY_CLI"
    CLI_ASSET="$CLI_NAME-$TAG-linux-x86_64.tar.gz"
    CLI_DEST="$LEGACY_CLI_DEST"
    # Its AppImage still has the old window id and icon name.
    APP_ID="$LEGACY_APP_ID"
    DESKTOP_DEST="$LEGACY_DESKTOP"
    ICON_DEST="$LEGACY_ICON"
    warn "$TAG is from before the rename to starorg-codex: installing it as $LEGACY_CLI"
fi
in_sums "$CLI_ASSET" || die "$CLI_ASSET is not in SHA256SUMS of $TAG"

APP_ASSET=""
if [ "$WANT_APP" = 1 ]; then
    APP_ASSET="$(printf '%s\n' "$sums" | awk '$2 ~ /_amd64\.AppImage$/ { sub(/^\*/, "", $2); print $2; exit }')"
    [ -n "$APP_ASSET" ] || die "release $TAG has no AppImage (see the release page; the .deb/.rpm are installed with your package manager)"
fi

if [ "$DRY_RUN" = 1 ]; then
    say "Dry run: nothing is downloaded or written."
    for a in "$CLI_ASSET" "$APP_ASSET"; do
        [ -n "$a" ] || continue
        if dl -o /dev/null -I "$BASE/$a"; then
            say "  found $BASE/$a"
        else
            die "listed in SHA256SUMS but not downloadable: $BASE/$a"
        fi
    done
    say "  would install the CLI to $CLI_DEST"
    [ "$CLI_NAME" = "$LEGACY_CLI" ] || say "  would link $LEGACY_CLI_DEST -> starorg-codex (the old name, until the next release)"
    if [ "$WANT_APP" = 1 ]; then
        say "  would install the app to $APP_DEST"
        say "  would write $DESKTOP_DEST and $ICON_DEST"
    fi
    check_path
    exit 0
fi

mkdir -p "$CACHE_HOME"
WORK="$(mktemp -d "$CACHE_HOME/starorg-install.XXXXXX")"
printf '%s\n' "$sums" >"$WORK/SHA256SUMS"

# Download one asset into $WORK and check it against SHA256SUMS.
fetch() {
    say "downloading $1"
    dl -o "$WORK/$1" "$BASE/$1" || die "download failed: $BASE/$1"
    (cd "$WORK" && awk -v n="$1" '$2 == n || $2 == "*" n' SHA256SUMS | sha256sum -c --status -) ||
        die "checksum mismatch for $1: not installed"
    say "  checksum ok"
}

fetch "$CLI_ASSET"
mkdir -p "$WORK/cli"
tar -xzf "$WORK/$CLI_ASSET" -C "$WORK/cli"
cli_bin="$WORK/cli/$CLI_NAME-$TAG-linux-x86_64/$CLI_NAME"
[ -f "$cli_bin" ] && [ ! -L "$cli_bin" ] || die "the archive does not contain $CLI_NAME-$TAG-linux-x86_64/$CLI_NAME"

mkdir -p "$BIN_DIR"
# Copy next to the target, then rename: a running starorg-codex is never half-written.
cp "$cli_bin" "$CLI_DEST.new"
chmod 0755 "$CLI_DEST.new"
mv -f "$CLI_DEST.new" "$CLI_DEST"
say "installed $CLI_DEST ($("$CLI_DEST" --version 2>/dev/null || echo "version unknown"))"
if [ "$CLI_NAME" != "$LEGACY_CLI" ]; then
    # The old name for scripts and LUG Helper hooks that still call it (it prints a
    # deprecation line). Replaces a 0.2 binary of that name in one rename.
    ln -sf starorg-codex "$LEGACY_CLI_DEST.new"
    mv -f "$LEGACY_CLI_DEST.new" "$LEGACY_CLI_DEST"
    say "linked $LEGACY_CLI_DEST -> starorg-codex (the old name works until the next release)"
fi

if [ "$WANT_APP" = 1 ]; then
    fetch "$APP_ASSET"
    chmod 0755 "$WORK/$APP_ASSET"
    # The icon comes out of the verified AppImage (extracting needs no FUSE).
    icon_rel="usr/share/icons/hicolor/128x128/apps/$APP_ID.png"
    (cd "$WORK" && "./$APP_ASSET" --appimage-extract "$icon_rel" >/dev/null 2>&1) || true
    cp "$WORK/$APP_ASSET" "$APP_DEST.new"
    mv -f "$APP_DEST.new" "$APP_DEST"
    say "installed $APP_DEST"

    icon_src="$WORK/squashfs-root/$icon_rel"
    if [ -f "$icon_src" ] && [ ! -L "$icon_src" ]; then
        mkdir -p "$(dirname "$ICON_DEST")"
        cp "$icon_src" "$ICON_DEST"
        say "installed $ICON_DEST"
    else
        warn "no icon found in the AppImage; the menu entry uses a generic icon"
    fi

    mkdir -p "$(dirname "$DESKTOP_DEST")"
    # Named after the window's app_id so Wayland compositors match window and icon.
    cat >"$DESKTOP_DEST" <<EOF
[Desktop Entry]
Type=Application
Name=StarOrg Codex
Comment=Star Citizen text enhancements
Exec="$APP_DEST"
Icon=$APP_ID
StartupWMClass=$APP_ID
Categories=Utility;Game;
Terminal=false
EOF
    say "installed $DESKTOP_DEST"
    # The 0.2 menu entry and icon (named after the old window id) would show the app twice.
    if [ "$APP_ID" != "$LEGACY_APP_ID" ]; then
        for f in "$LEGACY_DESKTOP" "$LEGACY_ICON"; do
            remove_file "$f"
        done
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$DATA_HOME/applications" >/dev/null 2>&1 || true
    fi

    # Type 2 AppImages need libfuse.so.2 to mount themselves.
    if command -v ldconfig >/dev/null 2>&1 && ! ldconfig -p 2>/dev/null | grep -q 'libfuse\.so\.2'; then
        warn "libfuse.so.2 not found: the AppImage will not start until you install it"
        warn "  Debian/Ubuntu: sudo apt install libfuse2   (24.04+: libfuse2t64)"
        warn "  Fedora: sudo dnf install fuse-libs   Arch/CachyOS: sudo pacman -S fuse2"
        warn "  or start it with: $APP_DEST --appimage-extract-and-run"
    fi
fi

say ""
say "Done. Check it with: starorg-codex --version && starorg-codex doctor"
check_path
