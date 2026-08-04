#!/usr/bin/env bash
# Assemble dist/mcopy.app from a release binary.
#
# A bare Mach-O binary on macOS is treated as a background process, so the GUI
# windows open unfocused / behind other apps and cx.activate() can't bring them
# forward. Wrapping the binary in a proper .app bundle (with an Info.plist and a
# bundle identifier) fixes activation and Dock/⌘-Tab behavior. The CLI
# subcommands still work when invoking the inner binary directly.
#
# NOTE: this bundle deliberately does NOT set LSUIElement. Version 0.2 did, which
# made mcopy an accessory (agent) application with no Dock tile at all — so the
# copy progress window could never be reached once it lost focus, no matter what
# window kind it used. AppKit is only initialised on the code paths that open a
# window, so the short-lived `mcopy copy` invocation still shows nothing.
#
# Usage: scripts/bundle-macos.sh [path/to/mcopy-binary]
# Signing/notarization is intentionally out of scope (TODO for signed releases).
set -euo pipefail

cd "$(dirname "$0")/.."
. "$(dirname "$0")/identity.sh"

BIN="${1:-target/release/mcopy}"
if [ ! -f "$BIN" ]; then
  echo "binary not found at $BIN — building release…" >&2
  cargo build --release
  BIN="target/release/mcopy"
fi

[ -f logo.icns ] || { echo "error: logo.icns missing — run scripts/make-icns.sh" >&2; exit 1; }

VERSION="$APP_VERSION"
APP="dist/mcopy.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/mcopy"
chmod +x "$APP/Contents/MacOS/mcopy"
cp logo.icns "$APP/Contents/Resources/logo.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>mcopy</string>
    <key>CFBundleDisplayName</key>
    <string>mcopy</string>
    <key>CFBundleExecutable</key>
    <string>mcopy</string>
    <key>CFBundleIdentifier</key>
    <string>${APP_ID}</string>
    <key>CFBundleGetInfoString</key>
    <string>${VERSION}, ${APP_COPYRIGHT}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>logo</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>NSHumanReadableCopyright</key>
    <string>${APP_COPYRIGHT}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "wrote $APP (version ${VERSION})"
