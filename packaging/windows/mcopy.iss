; Inno Setup script for the mcopy per-user installer.
;
; Build:  iscc /DAppVersion=0.3.0 packaging\windows\mcopy.iss
; Output: dist\mcopy-setup-<version>-x86_64.exe
;
; PrivilegesRequired=lowest is deliberate and load-bearing. mcopy 0.3 registers
; its shell integration under HKEY_CURRENT_USER, so neither installing nor
; using it needs administrator rights. Asking for elevation here would train
; users to expect a privilege the application never actually holds at runtime,
; which is exactly the mismatch that made 0.2 fail after a "successful" install.

#ifndef AppVersion
  #error AppVersion must be supplied, e.g. iscc /DAppVersion=0.3.0
#endif

#define AppName      "mcopy"
#define AppPublisher "NAKAMOZ"
; NOTE: this file must stay UTF-8 *with BOM*. Inno Setup 6 falls back to the
; system ANSI codepage for a BOM-less script, which mangles the non-ASCII
; letters below. scripts/package-windows.ps1 refuses to build without the BOM.
#define AppCopyright "Copyright (c) 2026 Nevzat ÇELİKKANAT"
#define AppExeName   "mcopy.exe"
#define AppUrl       "https://github.com/NAKAMOZ/mcopy"
#define AppSupportUrl "https://github.com/NAKAMOZ/mcopy/issues"

[Setup]
AppId={{6D1F2C4A-9E35-4B7D-9A2F-0C7E5B1D8A44}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppSupportUrl}
AppUpdatesURL={#AppUrl}/releases
AppContact={#AppSupportUrl}
AppCopyright={#AppCopyright}

; Populates the installer executable's own File Properties dialog, so the
; download is attributable before anyone runs it. The installed mcopy.exe gets
; the same identity from the version resource generated in build.rs.
VersionInfoVersion={#AppVersion}
VersionInfoProductVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}
VersionInfoCopyright={#AppCopyright}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup

; Per-user install: no UAC prompt, no admin requirement.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto

; Registers the Add/Remove Programs entry under HKCU so the app can be removed
; through Settings > Apps like any other installed application.
UninstallDisplayName={#AppName} {#AppVersion}
UninstallDisplayIcon={app}\{#AppExeName}

OutputDir=..\..\dist
OutputBaseFilename=mcopy-setup-{#AppVersion}-x86_64
SetupIconFile=..\..\logo.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; The installer is not code-signed; see the release notes for the SmartScreen
; warning users will see.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md";                    DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE";                      DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\CHANGELOG.md";                 DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}";               Filename: "{app}\{#AppExeName}"
Name: "{group}\Uninstall {#AppName}";     Filename: "{uninstallexe}"
Name: "{userdesktop}\{#AppName}";         Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "shellmenu";   Description: "Add ""Copy with mcopy"" and ""Paste with mcopy"" to the right-click menu"; GroupDescription: "Integration:"

[Run]
; Register the context menu against the *installed* path. This is what makes the
; integration survive deleting the downloaded installer.
Filename: "{app}\{#AppExeName}"; Parameters: "shell-install"; Tasks: shellmenu; Flags: runhidden waituntilterminated; StatusMsg: "Registering the right-click menu..."
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Remove the menu entries before the executable disappears, otherwise the
; registry would keep verbs pointing at a deleted file.
Filename: "{app}\{#AppExeName}"; Parameters: "shell-uninstall"; RunOnceId: "ShellUninstall"; Flags: runhidden waituntilterminated

[UninstallDelete]
; Logs and the transient clipboard payload are mcopy's own runtime state, not
; user documents, so removing them leaves the system clean.
Type: filesandordirs; Name: "{localappdata}\mcopy\logs"
Type: dirifempty;     Name: "{localappdata}\mcopy"
