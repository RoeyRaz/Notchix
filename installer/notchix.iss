; Notchix per-user installer. Build after cargo build --release.
#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

[Setup]
AppId={{A3F5D0C2-6B1E-4F7A-9C2D-5E8B7A1D3F40}
AppName=Notchix
AppVersion={#MyAppVersion}
AppVerName=Notchix {#MyAppVersion}
AppPublisher=Notchix contributors (based on Nock by Akhilesh Varute)
AppPublisherURL=https://github.com/RoeyRaz/Notchix
AppSupportURL=https://github.com/RoeyRaz/Notchix/issues
DefaultDirName={localappdata}\Programs\Notchix
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=Output
OutputBaseFilename=Notchix-Setup-{#MyAppVersion}
SetupIconFile=..\assets\notchix.ico
UninstallDisplayIcon={app}\nock.exe
UninstallDisplayName=Notchix
LicenseFile=..\LICENSE
InfoBeforeFile=..\docs\SETUP_GUIDE.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern dynamic
WizardSmallImageFile=..\assets\notchix_icon.png
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
VersionInfoProductName=Notchix

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startup"; Description: "Start Notchix when &Windows starts"; GroupDescription: "Options:"
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "..\target\release\nock.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\SETUP_GUIDE.txt"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\SETUP_GUIDE.html"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\assets\notchix_icon256.png"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\SPOTIFY_QUEUE_SETUP.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\QBIT_SETUP.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\SETUP-NOTION.ps1"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Notchix"; Filename: "{app}\nock.exe"
Name: "{autoprograms}\Notchix Setup Guide"; Filename: "{app}\docs\SETUP_GUIDE.html"
Name: "{autodesktop}\Notchix"; Filename: "{app}\nock.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Nock"; ValueData: """{app}\nock.exe"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\docs\SETUP_GUIDE.html"; Description: "Read the Notchix setup guide"; Flags: postinstall shellexec skipifsilent
Filename: "{app}\nock.exe"; Description: "Start Notchix now"; Flags: nowait postinstall skipifsilent

[Code]
procedure StopNotchix();
var
  rc: Integer;
begin
  Exec('taskkill.exe', '/F /IM nock.exe', '', SW_HIDE, ewWaitUntilTerminated, rc);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopNotchix();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    StopNotchix();
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Nock');
  end;
end;
