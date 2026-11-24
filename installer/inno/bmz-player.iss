#ifndef AppName
#define AppName "BMZ Player"
#endif
#define AppExeName "bmz-player.exe"
#define AppPublisher "hyrorre"
#ifndef AppId
#define AppId "D519C28F-4D6B-4E80-B38A-83DFBD0E7C15"
#endif

#ifndef AppVersion
#define AppVersion "0.5.4"
#endif

#ifndef AppArch
#define AppArch "x64"
#endif

#ifndef SourceDir
#define SourceDir "..\..\dist\windows\BMZ Player"
#endif

#ifndef OutputDir
#define OutputDir "..\..\dist\windows\installer"
#endif

#ifndef IconFile
#define IconFile "..\..\assets\app-icon\bmz-player.ico"
#endif

[Setup]
MinVersion=6.3
AppId={#AppId}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename=bmz-player-{#AppVersion}-windows-{#AppArch}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\resources\bmz-player.ico
SetupIconFile={#IconFile}
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
#ifdef SxsDllLayout
Source: "{#SourceDir}\updater\bmz-updater.exe"; Flags: dontcopy
#endif
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

; Merge packaged files into an existing installation. Never recursively delete
; resources: it may also contain user skins, fonts and other unknown files.

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\resources\bmz-player.ico"
Name: "{userdesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\resources\bmz-player.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

#ifdef SxsDllLayout
[Code]
procedure RunDllCleanup(const Arguments: String);
var
  ExitCode: Integer;
begin
  try
    if not ExecAndLogOutput(ExpandConstant('{tmp}\bmz-updater.exe'), Arguments,
      ExpandConstant('{tmp}'), SW_HIDE, ewWaitUntilTerminated, ExitCode, nil) or (ExitCode <> 0) then
      Log('Root DLL cleanup helper did not complete: ' + IntToStr(ExitCode));
  except
    Log('Root DLL cleanup helper could not run: ' + GetExceptionMessage);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  InstallDir: String;
begin
  InstallDir := AddQuotes(ExpandConstant('{app}'));
  if CurStep = ssInstall then
  begin
    try
      ExtractTemporaryFile('bmz-updater.exe');
      RunDllCleanup('--prepare-installer-dll-cleanup ' + InstallDir);
    except
      Log('Could not prepare root DLL cleanup: ' + GetExceptionMessage);
    end;
  end
  else if CurStep = ssPostInstall then
    RunDllCleanup('--retire-installer-dlls ' + InstallDir + ' {#AppVersion}');
end;
#endif
