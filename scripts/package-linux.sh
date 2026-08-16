#!/usr/bin/env bash
# Build the Linux distribution artifacts.
#
#   dist/mcopy-<version>-x86_64.AppImage  — portable, runs on any distro
#   dist/mcopy-<version>-x86_64.tar.gz    — tarball + install.sh (~/.local)
#
# AppImage is used instead of a .deb so the same artifact runs on Debian,
# Fedora, Arch and everything else without a package manager involved. The
# AppImage is built with appimagetool, downloaded on demand if it is not
# already on PATH — it needs no other build dependencies.
#
# Usage: scripts/package-linux.sh [path/to/mcopy-binary]
set -euo pipefail

cd "$(dirname "$0")/.."
. "$(dirname "$0")/identity.sh"

VERSION="$APP_VERSION"

BIN="${1:-target/release/mcopy}"
if [ ! -f "$BIN" ]; then
  echo "binary not found at $BIN — building release…" >&2
  cargo build --release --locked
  BIN="target/release/mcopy"
fi

# The desktop entry, icon and AppStream component are all named after the
# application id. That is not cosmetic: the icon lookup, the AppStream/desktop
# pairing and the Wayland window-to-launcher match all key off the same string.
DESKTOP="packaging/linux/${APP_ID}.desktop"
METAINFO="packaging/linux/${APP_ID}.metainfo.xml"
ICON="logo.svg"
for required in "$DESKTOP" "$METAINFO" "$ICON" LICENSE README.md; do
  [ -f "$required" ] || { echo "error: $required is missing" >&2; exit 1; }
done

mkdir -p dist
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Shared file layout used by both the AppImage and the tarball. Paths are
# relative to an install prefix so the tarball can target ~/.local instead of
# /usr, and the AppDir can target /usr under its own root.
stage_tree() {
  local root="$1" prefix="$2"
  install -Dm755 "$BIN"      "$root$prefix/bin/mcopy"
  install -Dm644 "$DESKTOP"  "$root$prefix/share/applications/${APP_ID}.desktop"
  install -Dm644 "$METAINFO" "$root$prefix/share/metainfo/${APP_ID}.metainfo.xml"
  install -Dm644 "$ICON"     "$root$prefix/share/icons/hicolor/scalable/apps/${APP_ID}.svg"
  install -Dm644 LICENSE     "$root$prefix/share/doc/mcopy/copyright"
  install -Dm644 README.md   "$root$prefix/share/doc/mcopy/README.md"
}

# Validate the AppStream component when the tooling is available. A malformed
# component is silently ignored by software centres, which would put us back to
# an application nobody can attribute.
if command -v appstreamcli >/dev/null 2>&1; then
  appstreamcli validate --no-net "$METAINFO" \
    || { echo "error: AppStream metadata failed validation" >&2; exit 1; }
else
  echo "note: appstreamcli not installed; skipping metainfo validation" >&2
fi

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$DESKTOP" \
    || { echo "error: desktop entry failed validation" >&2; exit 1; }
fi

# ----------------------------------------------------------- AppImage --------

APPDIR="$WORK/AppDir"
stage_tree "$APPDIR" "/usr"

# appimagetool requires the desktop entry and an icon at the AppDir root (not
# just under usr/share), named so the Icon= key in the desktop file resolves.
install -Dm644 "$DESKTOP" "$APPDIR/${APP_ID}.desktop"
install -Dm644 "$ICON"    "$APPDIR/${APP_ID}.svg"

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/mcopy" "$@"
APPRUN
chmod 755 "$APPDIR/AppRun"

APPIMAGETOOL="appimagetool"
if ! command -v appimagetool >/dev/null 2>&1; then
  APPIMAGETOOL="$WORK/appimagetool-x86_64.AppImage"
  echo "appimagetool not found on PATH; downloading…" >&2
  curl -fsSL -o "$APPIMAGETOOL" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$APPIMAGETOOL"
fi

APPIMAGE="dist/mcopy-${VERSION}-x86_64.AppImage"
rm -f "$APPIMAGE"
# --appimage-extract-and-run avoids requiring FUSE, which most CI runners and
# some desktops don't have set up.
ARCH=x86_64 "$APPIMAGETOOL" --appimage-extract-and-run "$APPDIR" "$APPIMAGE"
echo "wrote $APPIMAGE"

# ------------------------------------------------------------- tarball -------

TAR_ROOT="$WORK/tar/mcopy-${VERSION}"
stage_tree "$TAR_ROOT" ""

cat > "$TAR_ROOT/install.sh" <<'INSTALL'
#!/bin/sh
# Install mcopy for the current user (no root required).
#
# Usage: ./install.sh [--prefix DIR]     (default: ~/.local)
set -eu

prefix="${HOME}/.local"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix) prefix="$2"; shift 2 ;;
        -h|--help) sed -n '2,5p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 1 ;;
    esac
done

here="$(cd "$(dirname "$0")" && pwd)"

# Mirror the staged layout rather than naming each file. The desktop entry,
# icon and AppStream component are all named after the application id, and
# copying the tree keeps this script from having to repeat that id.
find "$here/bin" "$here/share" -type f 2>/dev/null | while IFS= read -r file; do
    relative="${file#"$here"/}"
    case "$relative" in
        bin/*) mode=755 ;;
        *)     mode=644 ;;
    esac
    install -D -m "$mode" "$file" "$prefix/$relative"
done

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$prefix/share/applications" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -qtf "$prefix/share/icons/hicolor" 2>/dev/null || true
fi

echo "Installed mcopy to $prefix/bin/mcopy"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Note: $prefix/bin is not on your PATH." ;;
esac

# Registering against the installed path is the whole point: the menu entries
# must not reference the extracted tarball, which the user will delete.
"$prefix/bin/mcopy" shell-install
INSTALL
chmod 755 "$TAR_ROOT/install.sh"

cat > "$TAR_ROOT/uninstall.sh" <<'UNINSTALL'
#!/bin/sh
# Remove a user-level mcopy install.
#
# Run from the extracted tarball, so the file list matches what was installed.
#
# Usage: ./uninstall.sh [prefix]        (default: ~/.local)
set -eu

prefix="${1:-${HOME}/.local}"
here="$(cd "$(dirname "$0")" && pwd)"

if [ -x "$prefix/bin/mcopy" ]; then
    "$prefix/bin/mcopy" shell-uninstall || true
fi

find "$here/bin" "$here/share" -type f 2>/dev/null | while IFS= read -r file; do
    rm -f "$prefix/${file#"$here"/}"
done

rmdir "$prefix/share/doc/mcopy" 2>/dev/null || true

echo "Removed mcopy from $prefix"
UNINSTALL
chmod 755 "$TAR_ROOT/uninstall.sh"

TARBALL="dist/mcopy-${VERSION}-x86_64.tar.gz"
tar -czf "$TARBALL" -C "$WORK/tar" "mcopy-${VERSION}"
echo "wrote $TARBALL"
