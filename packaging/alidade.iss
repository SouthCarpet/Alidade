; Alidade Windows installer (Inno Setup 6).
; Build with packaging\build-installer.ps1, or manually:
;   iscc /DAppVersion=0.1.0 packaging\alidade.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{1D8A1348-B80B-4CC7-BEF4-B7DB2E97B70F}
AppName=Alidade
AppVersion={#AppVersion}
AppPublisher=SouthCarpet
AppPublisherURL=https://github.com/SouthCarpet/Alidade
AppSupportURL=https://github.com/SouthCarpet/Alidade
AppUpdatesURL=https://github.com/SouthCarpet/Alidade/releases
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\Alidade
DefaultGroupName=Alidade
OutputBaseFilename=alidade-setup-{#AppVersion}
OutputDir=..\target\installer
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\alidade-app.exe
CloseApplications=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: checkedonce

[Files]
Source: "..\target\release\alidade-app.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\target\release\alidade.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Alidade"; Filename: "{app}\alidade-app.exe"
Name: "{group}\Uninstall Alidade"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Alidade"; Filename: "{app}\alidade-app.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\alidade-app.exe"; Description: "Launch Alidade now"; Flags: postinstall nowait skipifsilent

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DeleteData: Integer;
begin
  if CurUninstallStep = usUninstall then
  begin
    if not UninstallSilent then
    begin
      DeleteData := MsgBox('Odstrániť aj namerané dáta (databáza a nastavenia)?' + #13#10 +
        'Delete measured data too (database and settings)?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2);
      if DeleteData = IDYES then
      begin
        DelTree(ExpandConstant('{localappdata}\Alidade'), True, True, True);
        DelTree(ExpandConstant('{userappdata}\Alidade'), True, True, True);
      end;
    end;
  end;
end;
