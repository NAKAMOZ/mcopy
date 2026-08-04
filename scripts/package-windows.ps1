<#
.SYNOPSIS
    Build the mcopy Windows installer.

.DESCRIPTION
    Compiles a release binary if one is not present, then runs Inno Setup to
    produce dist\mcopy-setup-<version>-x86_64.exe.

    The version is read from Cargo.toml so there is exactly one place to bump it.

.PARAMETER SkipBuild
    Use the existing target\release\mcopy.exe instead of rebuilding.
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $cargoToml = Join-Path $repoRoot 'Cargo.toml'
    if (-not (Test-Path $cargoToml)) {
        throw "Cargo.toml not found at $cargoToml"
    }

    # Read package.version, not any dependency's version: stop at the first
    # match, which is inside [package].
    $version = (Select-String -Path $cargoToml -Pattern '^version = "(.*)"' |
        Select-Object -First 1).Matches[0].Groups[1].Value
    if ([string]::IsNullOrWhiteSpace($version)) {
        throw 'Could not read package.version from Cargo.toml'
    }
    Write-Host "Packaging mcopy $version"

    $exePath = Join-Path $repoRoot 'target\release\mcopy.exe'
    if (-not $SkipBuild) {
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
    }
    if (-not (Test-Path $exePath)) {
        throw "Release binary not found at $exePath"
    }

    # Locate the Inno Setup compiler. It is not on PATH by default even after
    # `choco install innosetup`, so check the standard install locations too.
    $iscc = (Get-Command 'iscc.exe' -ErrorAction SilentlyContinue)?.Source
    if (-not $iscc) {
        $candidates = @(
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
        )
        $iscc = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    }
    if (-not $iscc) {
        throw 'Inno Setup (ISCC.exe) not found. Install it with: choco install innosetup'
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $repoRoot 'dist') | Out-Null

    $script = Join-Path $repoRoot 'packaging\windows\mcopy.iss'

    # The script carries a non-ASCII copyright holder. Inno Setup 6 decodes a
    # BOM-less file in the system ANSI codepage, which would silently ship a
    # mangled name, so fail loudly if an editor has stripped the BOM.
    $prefix = [System.IO.File]::ReadAllBytes($script)[0..2]
    if (($prefix -join ',') -ne '239,187,191') {
        throw "packaging\windows\mcopy.iss lost its UTF-8 BOM; re-save it as UTF-8 with BOM"
    }
    & $iscc "/DAppVersion=$version" $script
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed ($LASTEXITCODE)" }

    $installer = Join-Path $repoRoot "dist\mcopy-setup-$version-x86_64.exe"
    if (-not (Test-Path $installer)) {
        throw "Expected installer not produced at $installer"
    }

    Write-Host "wrote $installer"
}
finally {
    Pop-Location
}
