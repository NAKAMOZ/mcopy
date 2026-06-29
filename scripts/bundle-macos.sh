#!/usr/bin/env bash
# Assemble dist/mcopy.app from a release binary.
#
# A bare Mach-O binary on macOS is treated as a background process, so the GUI
# windows open unfocused / behind other apps and cx.activate() can't bring them
# forward. Wrapping the binary in a proper .app bundle (with an Info.plist and a
# bundle identifier) fixes activation and Dock/⌘-Tab behavior. The CLI
# subcommands still work when invoking the inner binary directly.
#
# Usage: scripts/bundle-macos.sh [path/to/mcopy-binary]
# Signing/notarization is intentionally out of scope (TODO for signed releases).
set -euo pipefail

cd "$(dirname "$0")/.."

BIN="${1:-target/release/mcopy}"
if [ ! -f "$BIN" ]; then
  echo "binary not found at $BIN — building release…" >&2
  cargo build --release
  BIN="target/release/mcopy"
fi

[ -f logo.icns ] || { echo "error: logo.icns missing — run scripts/make-icns.sh" >&2; exit 1; }

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
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
    <string>com.mcopy.app</string>
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
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "wrote $APP (version ${VERSION})"
