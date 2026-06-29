#!/usr/bin/env bash
# Generate logo.icns from logo.svg for the macOS .app bundle.
#
# macOS-only (uses qlmanage + sips + iconutil). The result is committed so
# non-mac CI/devs don't need this toolchain; re-run only when logo.svg changes.
set -euo pipefail

cd "$(dirname "$0")/.."

SVG="logo.svg"
OUT="logo.icns"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Render the SVG to a high-res master PNG via Quick Look.
qlmanage -t -s 1024 -o "$WORK" "$SVG" >/dev/null 2>&1
MASTER="$WORK/$(basename "$SVG").png"
[ -f "$MASTER" ] || { echo "error: failed to render $SVG" >&2; exit 1; }

# Build the .iconset with every size macOS expects.
ICONSET="$WORK/logo.iconset"
mkdir -p "$ICONSET"
gen() { sips -z "$1" "$1" "$MASTER" --out "$ICONSET/$2" >/dev/null; }
gen 16   icon_16x16.png
gen 32   icon_16x16@2x.png
gen 32   icon_32x32.png
gen 64   icon_32x32@2x.png
gen 128  icon_128x128.png
gen 256  icon_128x128@2x.png
gen 256  icon_256x256.png
gen 512  icon_256x256@2x.png
gen 512  icon_512x512.png
gen 1024 icon_512x512@2x.png

iconutil -c icns "$ICONSET" -o "$OUT"
echo "wrote $OUT"
