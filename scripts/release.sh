#!/usr/bin/env bash
# Create the version tag that triggers the GitHub Actions release workflow.
set -euo pipefail

remote="origin"
version=""
skip_checks=0
dry_run=0

usage() {
  printf '%s\n' \
    "Usage: scripts/release.sh [--version X.Y.Z] [--remote origin] [--skip-checks] [--dry-run]" \
    "" \
    "Creates and pushes v<X.Y.Z>. The version defaults to Cargo.toml package.version." \
    "CHANGELOG.md must contain a section for the release version." \
    "Commit all release changes before running this script." \
    "" \
    "Examples:" \
    "  scripts/release.sh" \
    "  scripts/release.sh --version 0.2.0" \
    "  scripts/release.sh --dry-run"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

run() {
  if [ "$dry_run" -eq 1 ]; then
    printf 'dry-run:'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die "--version requires a value"
      version="${2#v}"
      shift 2
      ;;
    --remote)
      [ "$#" -ge 2 ] || die "--remote requires a value"
      remote="$2"
      shift 2
      ;;
    --skip-checks)
      skip_checks=1
      shift
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

[ -f Cargo.toml ] || die "Cargo.toml not found"

cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
[ -n "$cargo_version" ] || die "could not read package.version from Cargo.toml"

if [ -z "$version" ]; then
  version="$cargo_version"
elif [ "$version" != "$cargo_version" ]; then
  die "--version $version does not match Cargo.toml version $cargo_version"
fi

if [[ ! "$version" =~ ^[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  die "version must look like semver, got: $version"
fi

tag="v$version"
branch="$(git branch --show-current)"
[ -n "$branch" ] || die "run from a branch, not a detached HEAD"

git remote get-url "$remote" >/dev/null || die "remote not found: $remote"

if [ ! -f CHANGELOG.md ]; then
  die "CHANGELOG.md not found; run scripts/changelog.sh --write first"
fi

if ! awk -v version="$version" '
  /^##[[:space:]]+/ {
    heading = $0
    sub(/^##[[:space:]]+/, "", heading)
    sub(/[[:space:]].*$/, "", heading)
    gsub(/^\[/, "", heading)
    gsub(/\]$/, "", heading)
    if (heading == version || heading == "v" version) {
      found = 1
    }
  }
  END { exit found ? 0 : 1 }
' CHANGELOG.md; then
  die "CHANGELOG.md does not contain a section for $version; run scripts/changelog.sh --write first"
fi

status_output="$(git status --porcelain)"
if [ -n "$status_output" ]; then
  git status --short >&2
  if [ "$dry_run" -eq 0 ]; then
    die "working tree is not clean; commit or stash changes first"
  fi
  printf 'dry-run: working tree is not clean; real release would stop here.\n' >&2
fi

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  die "local tag already exists: $tag"
fi

if [ "$dry_run" -eq 0 ]; then
  set +e
  remote_tag_output="$(git ls-remote --exit-code --tags "$remote" "refs/tags/$tag" 2>&1)"
  remote_tag_status=$?
  set -e

  case "$remote_tag_status" in
    0)
      die "remote tag already exists: $tag"
      ;;
    2)
      ;;
    *)
      printf '%s\n' "$remote_tag_output" >&2
      die "could not check remote tag on $remote"
      ;;
  esac
fi

if [ "$skip_checks" -eq 0 ]; then
  run cargo test --locked
fi

run git tag -a "$tag" -m "Release $tag"
run git push "$remote" "HEAD:$branch"
run git push "$remote" "refs/tags/$tag"

if [ "$dry_run" -eq 1 ]; then
  printf 'Dry run complete. Pushing %s would trigger GitHub Actions to build assets and publish the release.\n' "$tag"
else
  printf 'Release tag %s pushed. GitHub Actions will build assets and publish the release.\n' "$tag"
fi
