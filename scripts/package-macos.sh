#!/usr/bin/env bash
# Build the macOS installer artifacts from dist/mcopy.app.
#
#   dist/mcopy-<version>.pkg  — Installer.app flow; puts mcopy.app in
#                               /Applications and registers the Finder Services
#                               for the installing user.
#   dist/mcopy-<version>.dmg  — drag-to-Applications disk image.
#
# Both are built with tools shipped in macOS (pkgbuild, productbuild, hdiutil),
# so packaging adds no dependencies.
#
# Neither artifact is signed or notarized. Gatekeeper will therefore require a
# right-click > Open on first launch; see the release notes.
#
# Usage: scripts/package-macos.sh
set -euo pipefail

cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
  echo "error: this script must run on macOS" >&2
  exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
[ -n "$VERSION" ] || { echo "error: could not read package.version from Cargo.toml" >&2; exit 1; }

APP="dist/mcopy.app"
IDENTIFIER="com.mcopy.app"

# Build the bundle if it is not already there.
[ -d "$APP" ] || ./scripts/bundle-macos.sh

# Guard the fix for the Dock-visibility bug: an LSUIElement bundle has no Dock
# tile, so the progress window could never be restored.
if /usr/libexec/PlistBuddy -c "Print :LSUIElement" "$APP/Contents/Info.plist" >/dev/null 2>&1; then
  echo "error: Info.plist still sets LSUIElement; the app would have no Dock icon" >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------- pkg --------

# Staged payload root: the pkg installs everything under this into /.
PAYLOAD="$WORK/payload"
mkdir -p "$PAYLOAD/Applications"
cp -R "$APP" "$PAYLOAD/Applications/mcopy.app"

SCRIPTS="$WORK/scripts"
mkdir -p "$SCRIPTS"
cat > "$SCRIPTS/postinstall" <<'POSTINSTALL'
#!/bin/sh
# Register the Finder Services for the user who ran the installer.
#
# The installer runs as root, but the Services live in the *user's*
# ~/Library/Services, so the command has to be dropped back to the console user.
# Failing here must not fail the installation: the user can always register the
# menu from the app itself.
set -eu

console_user="$(/usr/bin/stat -f "%Su" /dev/console 2>/dev/null || echo root)"
if [ "$console_user" = "root" ] || [ -z "$console_user" ]; then
  exit 0
fi

/usr/bin/sudo -u "$console_user" \
  /Applications/mcopy.app/Contents/MacOS/mcopy shell-install || true

exit 0
POSTINSTALL
chmod +x "$SCRIPTS/postinstall"

COMPONENT="$WORK/mcopy-component.pkg"
pkgbuild \
  --root "$PAYLOAD" \
  --scripts "$SCRIPTS" \
  --identifier "$IDENTIFIER" \
  --version "$VERSION" \
  --install-location / \
  "$COMPONENT"

cat > "$WORK/distribution.xml" <<DISTRIBUTION
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>mcopy ${VERSION}</title>
    <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <volume-check>
        <allowed-os-versions><os-version min="10.15"/></allowed-os-versions>
    </volume-check>
    <choices-outline>
        <line choice="default"/>
    </choices-outline>
    <choice id="default" title="mcopy">
        <pkg-ref id="${IDENTIFIER}"/>
    </choice>
    <pkg-ref id="${IDENTIFIER}" version="${VERSION}">mcopy-component.pkg</pkg-ref>
</installer-gui-script>
DISTRIBUTION

productbuild \
  --distribution "$WORK/distribution.xml" \
  --package-path "$WORK" \
  "dist/mcopy-${VERSION}.pkg"

echo "wrote dist/mcopy-${VERSION}.pkg"

# ---------------------------------------------------------------- dmg --------

STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/mcopy.app"
# The Applications symlink is what makes the drag-install gesture work.
ln -s /Applications "$STAGE/Applications"

DMG="dist/mcopy-${VERSION}.dmg"
rm -f "$DMG"
hdiutil create \
  -volname "mcopy ${VERSION}" \
  -srcfolder "$STAGE" \
  -ov \
  -format UDZO \
  "$DMG" >/dev/null

echo "wrote $DMG"
