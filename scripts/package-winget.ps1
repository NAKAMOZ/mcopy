<#
.SYNOPSIS
    Generate the winget-pkgs manifest for a built mcopy installer.

.DESCRIPTION
    Emits the three YAML files winget requires, under
    dist\winget\manifests\n\NAKAMOZ\mcopy\<version>\.

    The manifests are generated rather than committed because two of their
    fields cannot be known ahead of time: the SHA256 of the installer, and the
    release URL it will be downloaded from. A committed manifest with a stale
    hash is worse than none — winget rejects the package at install time, after
    the user has already tried.

    Publisher, copyright and version all come from scripts\Identity.ps1, so this
    manifest cannot disagree with the installer it describes.

.PARAMETER InstallerPath
    The built installer. Defaults to the one package-windows.ps1 produces.

.PARAMETER ReleaseUrlBase
    Where the installer will be downloaded from. Defaults to the GitHub release
    asset URL for the current version.

.EXAMPLE
    .\scripts\package-winget.ps1
#>
[CmdletBinding()]
param(
    [string]$InstallerPath,
    [string]$ReleaseUrlBase
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. "$PSScriptRoot\Identity.ps1"

$repoRoot = Split-Path -Parent $PSScriptRoot
$id = Get-McopyIdentity -RepoRoot $repoRoot

if (-not $InstallerPath) {
    $InstallerPath = Join-Path $repoRoot "dist\mcopy-setup-$($id.Version)-x86_64.exe"
}
if (-not (Test-Path $InstallerPath)) {
    throw "installer not found at $InstallerPath — run scripts\package-windows.ps1 first"
}

if (-not $ReleaseUrlBase) {
    $ReleaseUrlBase = "$($id.Homepage)/releases/download/v$($id.Version)"
}

$installerName = Split-Path -Leaf $InstallerPath
$installerUrl = "$ReleaseUrlBase/$installerName"
$sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $InstallerPath).Hash

# winget derives the package identifier from the publisher and package name.
$packageIdentifier = "$($id.Publisher).mcopy"

# Inno Setup writes its uninstall entry as {AppId}_is1. Declaring it lets winget
# recognise an existing install and handle upgrades rather than reinstalling.
$productCode = '{6D1F2C4A-9E35-4B7D-9A2F-0C7E5B1D8A44}_is1'

$manifestVersion = '1.6.0'
$outDir = Join-Path $repoRoot "dist\winget\manifests\n\$($id.Publisher)\mcopy\$($id.Version)"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

# winget requires UTF-8; a BOM is accepted and keeps the non-ASCII copyright
# readable in editors that guess the encoding.
$utf8 = New-Object System.Text.UTF8Encoding($true)
function Write-Manifest([string]$Name, [string]$Content) {
    $path = Join-Path $outDir $Name
    [System.IO.File]::WriteAllText($path, $Content, $utf8)
    Write-Host "wrote $path"
}

Write-Manifest "$packageIdentifier.yaml" @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.$manifestVersion.schema.json
PackageIdentifier: $packageIdentifier
PackageVersion: $($id.Version)
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $manifestVersion
"@

Write-Manifest "$packageIdentifier.installer.yaml" @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.$manifestVersion.schema.json
PackageIdentifier: $packageIdentifier
PackageVersion: $($id.Version)
MinimumOSVersion: 10.0.17763.0
InstallerType: inno
# The installer is per-user (Inno PrivilegesRequired=lowest), so it never
# prompts for elevation and winget must not assume a machine-wide scope.
Scope: user
InstallModes:
  - interactive
  - silent
  - silentWithProgress
UpgradeBehavior: install
ReleaseDate: $(Get-Date -Format 'yyyy-MM-dd')
ProductCode: '$productCode'
FileExtensions: []
Installers:
  - Architecture: x64
    InstallerUrl: $installerUrl
    InstallerSha256: $sha256
    InstallerLocale: en-US
    ExpectedReturnCodes:
      - InstallerReturnCode: 1223
        ReturnResponse: cancelledByUser
ManifestType: installer
ManifestVersion: $manifestVersion
"@

Write-Manifest "$packageIdentifier.locale.en-US.yaml" @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.$manifestVersion.schema.json
PackageIdentifier: $packageIdentifier
PackageVersion: $($id.Version)
PackageLocale: en-US
Publisher: $($id.Publisher)
PublisherUrl: https://github.com/$($id.Publisher)
PublisherSupportUrl: $($id.SupportUrl)
Author: $($id.Author)
PackageName: mcopy
PackageUrl: $($id.Homepage)
License: $($id.License)
LicenseUrl: $($id.Homepage)/blob/main/LICENSE
Copyright: $($id.Copyright)
CopyrightUrl: $($id.Homepage)/blob/main/LICENSE
ShortDescription: $($id.Description)
Description: |-
  mcopy turns the file manager right-click gesture into an asynchronous copy
  pipeline with a live progress window and cooperative pause, resume and cancel
  controls.

  Instead of a progress dialog you cannot influence, a copy started from
  Explorer opens a window you can minimise, come back to, pause mid-run, or
  cancel - and which tells you exactly why an item was skipped rather than
  reporting an anonymous failure count.

  The Paste entry only appears once something has been copied, and disappears
  again as soon as the paste succeeds. No administrator rights are required at
  any point.
Moniker: mcopy
Tags:
  - copy
  - file-manager
  - files
  - explorer
  - context-menu
  - utility
ReleaseNotesUrl: $($id.Homepage)/releases/tag/v$($id.Version)
Documentations:
  - DocumentLabel: README
    DocumentUrl: $($id.Homepage)/blob/main/README.md
ManifestType: defaultLocale
ManifestVersion: $manifestVersion
"@

Write-Host ''
Write-Host "Manifests for $packageIdentifier $($id.Version) written to:"
Write-Host "  $outDir"
Write-Host ''
Write-Host 'Validate them with:'
Write-Host "  winget validate --manifest `"$outDir`""
Write-Host ''
Write-Host 'Then submit by copying the directory into a fork of'
Write-Host '  https://github.com/microsoft/winget-pkgs'
Write-Host 'under manifests/n/... and opening a pull request.'
Write-Host ''
Write-Host 'NOTE: InstallerUrl must resolve before the PR is merged, so publish'
Write-Host '      the GitHub release first.'
