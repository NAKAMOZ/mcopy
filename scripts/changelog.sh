#!/usr/bin/env bash
# Generate or insert a Keep a Changelog style entry from git commit subjects.
set -euo pipefail

version=""
from_ref=""
write=0

usage() {
  printf '%s\n' \
    "Usage: scripts/changelog.sh [--version X.Y.Z] [--from TAG] [--write]" \
    "" \
    "Generates a CHANGELOG.md entry from commits since the previous tag." \
    "The version defaults to Cargo.toml package.version." \
    "" \
    "Examples:" \
    "  scripts/changelog.sh" \
    "  scripts/changelog.sh --write" \
    "  scripts/changelog.sh --version 0.3.0 --from v0.2.0 --write"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || die "--version requires a value"
      version="${2#v}"
      shift 2
      ;;
    --from)
      [ "$#" -ge 2 ] || die "--from requires a value"
      from_ref="$2"
      shift 2
      ;;
    --write)
      write=1
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
fi

if [[ ! "$version" =~ ^[0-9]+[.][0-9]+[.][0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
  die "version must look like semver, got: $version"
fi

if [ -z "$from_ref" ]; then
  from_ref="$(git describe --tags --abbrev=0 2>/dev/null || true)"
fi

if [ -n "$from_ref" ]; then
  git rev-parse -q --verify "$from_ref" >/dev/null || die "unknown ref: $from_ref"
  range="$from_ref..HEAD"
else
  range="HEAD"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

added="$tmp_dir/added"
changed="$tmp_dir/changed"
fixed="$tmp_dir/fixed"
performance="$tmp_dir/performance"
release="$tmp_dir/release"
touch "$added" "$changed" "$fixed" "$performance" "$release"
commit_regex='^([a-z]+)(\([^)]+\))?(!)?:[[:space:]]*(.*)$'

append_item() {
  local file="$1"
  local item="$2"

  item="${item#.}"
  item="${item#"${item%%[![:space:]]*}"}"
  item="${item%"${item##*[![:space:]]}"}"
  [ -n "$item" ] || return 0

  printf -- '- %s\n' "$item" >> "$file"
}

while IFS= read -r subject; do
  [ -n "$subject" ] || continue

  section="$changed"
  item="$subject"

  if [[ "$subject" =~ $commit_regex ]]; then
    kind="${BASH_REMATCH[1]}"
    item="${BASH_REMATCH[4]}"

    case "$kind" in
      feat)
        section="$added"
        ;;
      fix)
        section="$fixed"
        ;;
      perf)
        section="$performance"
        ;;
      build|ci)
        section="$release"
        ;;
      chore|docs|refactor|style|test)
        section="$changed"
        ;;
    esac
  fi

  append_item "$section" "$item"
done < <(git log --reverse --format='%s' "$range")

entry="$tmp_dir/entry.md"
{
  printf '## [%s] - %s\n\n' "$version" "$(date +%F)"

  wrote=0
  for section_file in "$added" "$changed" "$fixed" "$performance" "$release"; do
    [ -s "$section_file" ] || continue

    case "$section_file" in
      "$added") heading="Added" ;;
      "$changed") heading="Changed" ;;
      "$fixed") heading="Fixed" ;;
      "$performance") heading="Performance" ;;
      "$release") heading="Release" ;;
    esac

    printf '### %s\n\n' "$heading"
    cat "$section_file"
    printf '\n'
    wrote=1
  done

  if [ "$wrote" -eq 0 ]; then
    printf '### Changed\n\n'
    printf -- '- No user-facing changes recorded.\n\n'
  fi
} > "$entry"

if [ "$write" -eq 0 ]; then
  cat "$entry"
  exit 0
fi

if [ ! -f CHANGELOG.md ]; then
  {
    printf '# Changelog\n\n'
    printf 'All notable user-facing changes to this project are documented in this file.\n\n'
    printf '## [Unreleased]\n\n'
    cat "$entry"
  } > CHANGELOG.md
  printf 'wrote CHANGELOG.md\n'
  exit 0
fi

if awk -v version="$version" '
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
  die "CHANGELOG.md already has an entry for $version"
fi

tmp_changelog="$tmp_dir/CHANGELOG.md"
awk -v entry="$entry" '
  BEGIN { inserted = 0 }

  /^##[[:space:]]+\[?Unreleased\]?/ {
    print
    print ""
    while ((getline line < entry) > 0) {
      print line
    }
    inserted = 1
    next
  }

  { print }

  END {
    if (!inserted) {
      exit 2
    }
  }
' CHANGELOG.md > "$tmp_changelog" || die "CHANGELOG.md must contain an Unreleased section"

mv "$tmp_changelog" CHANGELOG.md
printf 'updated CHANGELOG.md with %s\n' "$version"
