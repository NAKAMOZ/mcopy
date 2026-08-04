#!/usr/bin/env bash
# Build the Linux distribution artifacts.
#
#   dist/mcopy_<version>_amd64.deb        — Debian/Ubuntu package
#   dist/mcopy-<version>-x86_64.tar.gz    — portable tarball + install.sh
#
# The .deb is assembled with dpkg-deb, which ships with Debian and Ubuntu (and
# is preinstalled on GitHub's ubuntu runners), so packaging adds no build
# dependencies. Runtime dependencies are derived from the linked binary rather
# than hand-maintained, so they cannot drift from what gpui actually needs.
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

# Shared file layout used by both the .deb and the tarball. Paths are relative
# to an install prefix so the tarball can target ~/.local instead of /usr.
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

# ---------------------------------------------------------------- deb --------

DEB_ROOT="$WORK/deb"
stage_tree "$DEB_ROOT" "/usr"
mkdir -p "$DEB_ROOT/DEBIAN"

# Resolve runtime dependencies from the binary's own NEEDED entries. dpkg-shlibdeps
# is the correct tool but needs a full debian/ tree, so map the shared objects to
# packages directly with dpkg -S.
resolve_dependencies() {
  local libs packages=()
  libs="$(ldd "$BIN" 2>/dev/null | awk '/=> \//{print $3}' | sort -u)" || return 0

  while IFS= read -r lib; do
    [ -n "$lib" ] || continue
    local pkg
    pkg="$(dpkg -S "$(readlink -f "$lib")" 2>/dev/null | cut -d: -f1 | head -1)" || continue
    [ -n "$pkg" ] && packages+=("$pkg")
  done <<< "$libs"

  printf '%s\n' "${packages[@]}" | sort -u | paste -sd', ' -
}

DEPENDS="$(resolve_dependencies || true)"
# gpui renders through Vulkan and needs a loader at runtime; it is dlopen'd, so
# it never shows up in ldd output and has to be named explicitly.
if [ -n "$DEPENDS" ]; then
  DEPENDS="$DEPENDS, libvulkan1"
else
  echo "warning: could not resolve library dependencies; falling back to a known-good list" >&2
  DEPENDS="libc6, libgcc-s1, libx11-6, libxcb1, libxkbcommon0, libwayland-client0, libfontconfig1, libvulkan1"
fi

INSTALLED_SIZE="$(du -ks "$DEB_ROOT/usr" | cut -f1)"

cat > "$DEB_ROOT/DEBIAN/control" <<CONTROL
Package: mcopy
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: ${APP_MAINTAINER}
Installed-Size: ${INSTALLED_SIZE}
Depends: ${DEPENDS}
Homepage: ${APP_HOMEPAGE}
Description: Fast and reliable file copy utility
 mcopy turns the file manager right-click gesture into an asynchronous copy
 pipeline with a live progress window and pause, resume and cancel controls.
 .
 File manager integration is per-user and is registered from the application
 itself, so installing this package never modifies another user's home
 directory.
CONTROL

# The file-manager integration is per-user, so the package cannot register it
# for everyone at install time. Tell the user how, once, rather than silently
# doing nothing.
cat > "$DEB_ROOT/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e

if [ "$1" = "configure" ]; then
    # Refresh the desktop and icon caches so the launcher entry appears without
    # a re-login. Both are optional on minimal systems.
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -qtf /usr/share/icons/hicolor || true
    fi

    echo "mcopy installed. Run 'mcopy shell-install' (or launch mcopy and press"
    echo "Install) to add the file manager entries for your user account."
fi

exit 0
POSTINST
chmod 755 "$DEB_ROOT/DEBIAN/postinst"

cat > "$DEB_ROOT/DEBIAN/prerm" <<'PRERM'
#!/bin/sh
set -e

# Remove the invoking user's menu entries before the binary goes away, so no
# entry is left pointing at a deleted executable. Other users' entries are
# theirs to remove; `mcopy shell-uninstall` does it per account.
if [ "$1" = "remove" ] && [ -x /usr/bin/mcopy ]; then
    /usr/bin/mcopy shell-uninstall >/dev/null 2>&1 || true
fi

exit 0
PRERM
chmod 755 "$DEB_ROOT/DEBIAN/prerm"

DEB="dist/mcopy_${VERSION}_amd64.deb"
dpkg-deb --build --root-owner-group "$DEB_ROOT" "$DEB" >/dev/null
echo "wrote $DEB"

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
