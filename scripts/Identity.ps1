<#
.SYNOPSIS
    Canonical project identity for the Windows packaging scripts.

.DESCRIPTION
    The PowerShell counterpart of scripts/identity.sh. Both read the same two
    files — src/lib.rs and Cargo.toml — so no packaging artifact declares the
    publisher, copyright or app id independently and they cannot drift apart.

    Dot-source it:  . "$PSScriptRoot\Identity.ps1"; $id = Get-McopyIdentity
#>

function Get-McopyIdentity {
    [CmdletBinding()]
    param(
        [string]$RepoRoot = (Split-Path -Parent $PSScriptRoot)
    )

    # Scoped to this function on purpose. Dot-sourcing is how callers get at
    # Get-McopyIdentity, and setting strict mode at file scope would silently
    # change the rules for the caller's whole session.
    Set-StrictMode -Version Latest

    $libRs = Join-Path $RepoRoot 'src\lib.rs'
    $cargo = Join-Path $RepoRoot 'Cargo.toml'
    foreach ($required in @($libRs, $cargo)) {
        if (-not (Test-Path $required)) { throw "not found: $required" }
    }

    # `pub const NAME: &str = "value";` — one line each under the project's
    # 80-column rustfmt setting, so a line match is sound.
    function Read-RustConst([string]$Name) {
        $match = Select-String -Path $libRs `
            -Pattern "^pub const $Name`: &str = ""(.*)"";$" |
            Select-Object -First 1
        if (-not $match) {
            throw "could not read $Name from src/lib.rs (renamed, or wrapped across lines?)"
        }
        $match.Matches[0].Groups[1].Value
    }

    # Read from the [package] table only, so a dependency's `version` can never
    # be picked up instead.
    function Read-CargoField([string]$Key) {
        $inPackage = $false
        foreach ($line in Get-Content -LiteralPath $cargo) {
            if ($line -match '^\[') { $inPackage = ($line.Trim() -eq '[package]'); continue }
            if ($inPackage -and $line -match "^$Key\s*=\s*""(.*)""") {
                return $Matches[1]
            }
        }
        throw "could not read $Key from Cargo.toml [package]"
    }

    # `authors` is a TOML array, so it needs its own reader rather than the
    # quoted-scalar match above.
    function Read-CargoAuthor {
        $inPackage = $false
        foreach ($line in Get-Content -LiteralPath $cargo) {
            if ($line -match '^\[') { $inPackage = ($line.Trim() -eq '[package]'); continue }
            if ($inPackage -and $line -match '^authors\s*=\s*\[\s*"([^"]*)"') {
                return $Matches[1]
            }
        }
        throw 'could not read authors from Cargo.toml [package]'
    }

    $homepage = Read-CargoField 'homepage'

    [pscustomobject]@{
        AppId       = Read-RustConst 'APP_ID'
        Publisher   = Read-RustConst 'APP_PUBLISHER'
        Copyright   = Read-RustConst 'APP_COPYRIGHT'
        Version     = Read-CargoField 'version'
        Description = Read-CargoField 'description'
        Homepage    = $homepage
        License     = Read-CargoField 'license'
        Author      = Read-CargoAuthor
        SupportUrl  = "$homepage/issues"
    }
}
