#!/usr/bin/env bash
# Canonical project identity, read straight out of the source tree.
#
# Every packaging artifact needs the same publisher, copyright, app id and
# version. Re-typing them per script is how an AppImage ends up crediting
# someone different from the .app, so this file derives them from the two
# places that already own them — `src/lib.rs` and `Cargo.toml` — and nothing
# else declares them independently.
#
# Usage:  . "$(dirname "$0")/identity.sh"
#
# Exports: APP_ID, APP_PUBLISHER, APP_COPYRIGHT, APP_VERSION, APP_DESCRIPTION,
#          APP_HOMEPAGE, APP_SUPPORT_URL, APP_LICENSE, APP_MAINTAINER

set -euo pipefail

_identity_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Pull `pub const NAME: &str = "value";` out of src/lib.rs. The constants are
# short enough to stay on one line under the project's 80-column rustfmt
# setting, so a line-oriented match is sound.
_read_rust_const() {
    local name="$1" value
    value="$(sed -n \
        "s/^pub const ${name}: &str = \"\\(.*\\)\";\$/\\1/p" \
        "${_identity_root}/src/lib.rs" | head -1)"

    if [ -z "$value" ]; then
        echo "identity.sh: could not read ${name} from src/lib.rs" >&2
        echo "  (was the constant renamed, or wrapped across lines?)" >&2
        exit 1
    fi
    printf '%s' "$value"
}

# Read a key from the [package] table, stopping at the next table so a
# dependency's `version` can never be mistaken for the package's.
_read_cargo_field() {
    local key="$1" value
    value="$(awk -v key="$key" '
        /^\[/ { in_package = ($0 == "[package]"); next }
        in_package && $0 ~ "^" key " *= *\"" {
            sub("^" key " *= *\"", "")
            sub("\".*$", "")
            print
            exit
        }
    ' "${_identity_root}/Cargo.toml")"

    if [ -z "$value" ]; then
        echo "identity.sh: could not read ${key} from Cargo.toml [package]" >&2
        exit 1
    fi
    printf '%s' "$value"
}

APP_ID="$(_read_rust_const APP_ID)"
APP_PUBLISHER="$(_read_rust_const APP_PUBLISHER)"
APP_COPYRIGHT="$(_read_rust_const APP_COPYRIGHT)"

APP_VERSION="$(_read_cargo_field version)"
APP_DESCRIPTION="$(_read_cargo_field description)"
APP_HOMEPAGE="$(_read_cargo_field homepage)"
APP_LICENSE="$(_read_cargo_field license)"

APP_SUPPORT_URL="${APP_HOMEPAGE}/issues"

# Debian requires an RFC822 address in the Maintainer field. The GitHub noreply
# form keeps a personal inbox out of every installed package while still routing
# to a real destination.
APP_MAINTAINER="${APP_PUBLISHER} <nakamoz@users.noreply.github.com>"

export APP_ID APP_PUBLISHER APP_COPYRIGHT APP_VERSION APP_DESCRIPTION
export APP_HOMEPAGE APP_SUPPORT_URL APP_LICENSE APP_MAINTAINER
