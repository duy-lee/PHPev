; PHPev installer (Inno Setup 6), built by .github/scripts/build-installer.ps1:
;   ISCC.exe /DAppVersion=0.1.2 installer\phpev.iss
;
; The executables go into the program folder; `phpev setup` then copies them
; into the PHPev home (the folder for PHP, MariaDB, databases and settings) of
; the user who ran setup, and puts that on their PATH. Uninstalling runs
; `phpev uninstall --installer` before the program folder is removed.

#ifndef AppVersion
  #error Pass the version: ISCC.exe /DAppVersion=x.y.z
#endif
#ifndef BinDir
  #define BinDir "..\target\release"
#endif
; VERSIONINFO only takes numbers: 1.0.0-rc.1 -> 1.0.0
#define Dash Pos("-", AppVersion)
#if Dash > 0
  #define NumVersion Copy(AppVersion, 1, Dash - 1)
#else
  #define NumVersion AppVersion
#endif

[Setup]
; Also in crates/core/src/installer.rs (APP_ID). Never change it.
AppId={{F5AFBAA9-BD8F-4024-94FF-B613075C9FB6}
AppName=PHPev
AppVersion={#AppVersion}
AppVerName=PHPev {#AppVersion}
AppPublisher=Duy Lee
AppPublisherURL=https://github.com/duy-lee/PHPev
AppSupportURL=https://github.com/duy-lee/PHPev/issues
AppUpdatesURL=https://github.com/duy-lee/PHPev/releases
VersionInfoVersion={#NumVersion}
VersionInfoProductName=PHPev
VersionInfoDescription=PHPev setup
; {autopf} is %LOCALAPPDATA%\Programs for "only for me", Program Files for all users.
DefaultDirName={autopf}\PHPev
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=..\dist
OutputBaseFilename=phpev-{#AppVersion}-setup
SetupIconFile=..\assets\phpev.ico
UninstallDisplayIcon={app}\phpev-daemon.exe
UninstallDisplayName=PHPev
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Start PHPev when I sign in to Windows"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#BinDir}\phpev.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\phpev-daemon.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\phpev-elevate.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BinDir}\phpev-shim.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\PHPev"; Filename: "{app}\phpev-daemon.exe"; Parameters: "--open-dashboard"; Comment: "Open the PHPev dashboard"
Name: "{autodesktop}\PHPev"; Filename: "{app}\phpev-daemon.exe"; Parameters: "--open-dashboard"; Comment: "Open the PHPev dashboard"; Tasks: desktopicon

[Run]
; An upgrade replaces the running service: stop it first (no-op on a new install).
Filename: "{app}\phpev.exe"; Parameters: "stop"; StatusMsg: "Stopping PHPev..."; Flags: runhidden runasoriginaluser
Filename: "{app}\phpev.exe"; Parameters: "setup --home ""{code:HomeDir}"""; StatusMsg: "Setting up PHPev..."; Flags: runhidden runasoriginaluser
Filename: "{app}\phpev.exe"; Parameters: "autostart on"; Flags: runhidden runasoriginaluser; Tasks: autostart
Filename: "{app}\phpev.exe"; Parameters: "autostart off"; Flags: runhidden runasoriginaluser; Tasks: not autostart
Filename: "{app}\phpev-daemon.exe"; Parameters: "--open-dashboard"; Description: "Open PHPev"; Flags: postinstall nowait skipifsilent runasoriginaluser

[Code]
var
  HomePage: TInputDirWizardPage;

{ No spaces or non-ASCII characters: MariaDB and some PHP extensions break on them. }
function IsSafePath(const Path: String): Boolean;
var
  I: Integer;
begin
  Result := Pos(' ', Path) = 0;
  for I := 1 to Length(Path) do
    if Ord(Path[I]) > 127 then
      Result := False;
end;

{ The current PHPev home: PHPEV_HOME, else %USERPROFILE%\.phpev. }
function CurrentHome: String;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'PHPEV_HOME', Result) or (Result = '') then
    Result := ExpandConstant('{%USERPROFILE}\.phpev');
end;

function IsPhpevHome(const Dir: String): Boolean;
begin
  Result := FileExists(AddBackslash(Dir) + 'config.toml');
end;

function IsEmptyDir(const Dir: String): Boolean;
var
  FindRec: TFindRec;
begin
  Result := True;
  if FindFirst(AddBackslash(Dir) + '*', FindRec) then
  try
    repeat
      if (FindRec.Name <> '.') and (FindRec.Name <> '..') then
        Result := False;
    until not Result or not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

function HomeDir(Param: String): String;
begin
  Result := RemoveBackslashUnlessRoot(HomePage.Values[0]);
end;

procedure InitializeWizard;
var
  Default: String;
begin
  HomePage := CreateInputDirPage(wpSelectDir,
    'PHPev data folder',
    'Where should PHPev keep PHP, MariaDB, your databases and settings?',
    'PHPev keeps its data in the following folder, separate from the program files.' + #13#10 +
    'Use a path without spaces or accented characters.' + #13#10#13#10 +
    'To continue, click Next. If you would like to select a different folder, click Browse.',
    True, 'PHPev');
  HomePage.Add('');
  Default := CurrentHome;
  if not IsPhpevHome(Default) and not IsSafePath(Default) then
    Default := 'C:\PHPev';
  HomePage.Values[0] := Default;
end;

{ An existing home keeps its place: moving it would leave the data behind. }
function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = HomePage.ID) and IsPhpevHome(CurrentHome);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Dir, App: String;
begin
  Result := True;
  if CurPageID <> HomePage.ID then
    Exit;
  Dir := HomeDir('');
  App := AddBackslash(WizardDirValue);
  if Length(ExtractFileDrive(Dir)) + 1 >= Length(AddBackslash(Dir)) then begin
    MsgBox('Choose a folder, not a whole drive.', mbError, MB_OK);
    Result := False;
  end else if Pos(Lowercase(App), Lowercase(AddBackslash(Dir))) = 1 then begin
    MsgBox('Choose a folder outside the program folder (' + WizardDirValue + ').', mbError, MB_OK);
    Result := False;
  end else if DirExists(Dir) and not IsPhpevHome(Dir) and not IsEmptyDir(Dir) then begin
    MsgBox('This folder already contains other files. Choose a new or empty folder:' + #13#10 +
      'uninstalling PHPev deletes what PHPev puts in it.', mbError, MB_OK);
    Result := False;
  end else if not IsSafePath(Dir) then
    Result := MsgBox('This path contains spaces or accented characters, which can break MariaDB and some PHP extensions.' + #13#10#13#10 +
      'Use it anyway?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES;
end;

{ Before the program folder goes: undo what `phpev setup` and PHPev did. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Args, Log, Output: String;
  Code: Integer;
  Text: AnsiString;
begin
  if CurUninstallStep <> usUninstall then
    Exit;
  Args := 'uninstall --yes --installer';
  if not UninstallSilent and (MsgBox(
      'Also delete your PHPev databases and settings?' + #13#10#13#10 +
      'Yes: delete everything in ' + CurrentHome + '.' + #13#10 +
      'No: keep the databases (data folder) and config.toml there, so reinstalling PHPev brings them back.',
      mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    Args := Args + ' --purge';
  Log := AddBackslash(GetTempDir) + 'phpev-uninstall.log';
  Exec(ExpandConstant('{cmd}'), '/d /c ""' + ExpandConstant('{app}\phpev.exe') + '" ' + Args + ' > "' + Log + '" 2>&1"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
  if (Code <> 0) and not UninstallSilent then begin
    Output := '';
    if LoadStringFromFile(Log, Text) then
      Output := String(Text);
    MsgBox('PHPev was removed, but a few things need your attention:' + #13#10#13#10 + Output, mbInformation, MB_OK);
  end;
end;
