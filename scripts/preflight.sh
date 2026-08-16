#!/usr/bin/env bash
# Run what CI runs, before pushing.
#
#   scripts/preflight.sh          fmt, clippy, tests, CLI smoke test  (seconds)
#   scripts/preflight.sh --full   the above plus a release build and packaging
#
# This mirrors the `check` job in .github/workflows/ci.yml, and with --full the
# `packaging` job too. Running it first turns a ten-minute round trip through
# GitHub into a local one.
#
# What it does NOT cover, so nobody mistakes a green run for a green CI:
#
#   - Windows and macOS. CI builds on all three, and this repository has real
#     `#[cfg(target_os)]` code behind those, so a compile error there is
#     invisible here. Cross-checking is not an option either: `ring` (pulled in
#     by gpui's HTTP client) builds C for the target, which needs an MSVC or
#     osxcross toolchain rather than a rustup target.
#   - The workflow files themselves. Use `act` for those; see the README.
#
# In practice fmt, clippy and test failures are what actually break CI, and all
# three are caught here.
set -euo pipefail

cd "$(dirname "$0")/.."

full=0
case "${1:-}" in
    --full) full=1 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    "") ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
esac

step() { printf '\n\033[1m▸ %s\033[0m\n' "$1"; }
failed=""

# Keep going after a failure so one run reports every problem, rather than
# making the user fix, re-run, and discover the next one.
run() {
    local label="$1"; shift
    step "$label"
    if "$@"; then
        return 0
    fi
    failed="${failed}${failed:+, }${label}"
    return 0
}

# --quiet throughout: a passing run should be a few lines, not the name of
# every test. Failures still print in full, which is the only time the detail
# is wanted.
run "Formatting"  cargo fmt --all --check
run "Lint"        cargo clippy --all-targets --locked --quiet -- -D warnings
run "Tests"       cargo test --locked --quiet

# The GUI cannot be exercised without a desktop session, but the CLI paths can:
# this catches a binary that fails to start at all, exactly as CI does.
run "CLI smoke test" bash -c '
    set -e
    cargo run --locked --release --quiet -- --version
    cargo run --locked --release --quiet -- status
'

if [ "$full" -eq 1 ]; then
    run "Release build" cargo build --release --locked
    run "Packaging"     ./scripts/package-linux.sh

    # Same assertion as the CI step: an AppImage that does not run is not a
    # working artifact. --appimage-extract-and-run avoids needing FUSE.
    run "AppImage runs" bash -c '
        set -e
        appimage="$(ls -t dist/mcopy-*-x86_64.AppImage | head -1)"
        chmod +x "$appimage"
        "$appimage" --appimage-extract-and-run --version
    '
fi

if [ -n "$failed" ]; then
    printf '\n\033[31m✗ failed: %s\033[0m\n' "$failed" >&2
    exit 1
fi

printf '\n\033[32m✓ preflight passed\033[0m — Windows and macOS are still only covered by CI.\n'
