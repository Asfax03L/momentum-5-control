; Installateur Windows d'OpenHeadset Control (Inno Setup 6), par
; utilisateur, sans droits administrateur. Même dossier, même raccourci et
; même entrée de démarrage que scripts/installer.ps1 : une installation par
; le script est mise à jour par cet installateur, et inversement.
;
; Construction (release.yml) :
;   iscc /DVersion=0.14.0 /DBin=<dossier des .exe> /DRacine=<dépôt> /O<sortie> packaging\windows\openheadset-control.iss
;
; Installation silencieuse (winget, essais) : /VERYSILENT /SUPPRESSMSGBOXES
; /NORESTART. /SANSRUNTIME installe même sans Windows App Runtime 2
; (essais en CI seulement : l'interface ne démarrerait pas).
; Désinstallation : Paramètres > Applications, ou unins000.exe /VERYSILENT.
; Réglages, scènes et journaux restent dans %LOCALAPPDATA%\momentum-ctl-win.

#ifndef Version
  #error Passer /DVersion=X.Y.Z
#endif
#ifndef Bin
  #define Bin "..\..\target\release"
#endif
#ifndef Racine
  #define Racine "..\.."
#endif

[Setup]
AppId={{67997F7F-2010-488F-9966-CE13D6A42173}
AppName=OpenHeadset Control
AppVersion={#Version}
AppVerName=OpenHeadset Control {#Version}
AppPublisher=Asfax03L
AppPublisherURL=https://github.com/Asfax03L/openheadset-control
AppSupportURL=https://github.com/Asfax03L/openheadset-control/issues
AppUpdatesURL=https://github.com/Asfax03L/openheadset-control/releases
AppComments=Réglages du casque Sennheiser MOMENTUM 5 (non officiel, sans lien avec Sennheiser ni Sonova)
VersionInfoVersion={#Version}
DefaultDirName={localappdata}\Programs\momentum-ctl-win
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputBaseFilename=openheadset-control-v{#Version}-windows-x86_64-installateur
UninstallDisplayName=OpenHeadset Control
UninstallDisplayIcon={app}\momentum-winui.exe
LicenseFile={#Racine}\LICENSE-MIT
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Les processus sont arrêtés par [Code] (même règle que installer.ps1).
CloseApplications=no
RestartApplications=no

[Languages]
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "demarrage"; Description: "Lancer OpenHeadset Control au démarrage de Windows"

[Files]
Source: "{#Bin}\momentum-winui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\momentum-daemon.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\ctl.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Racine}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Racine}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; Même nom que installer.ps1 : relancer l'exécutable montre la fenêtre de
; l'instance en cours.
Name: "{userprograms}\OpenHeadset Control"; Filename: "{app}\momentum-winui.exe"; WorkingDir: "{app}"; Comment: "OpenHeadset Control : réglages du MOMENTUM 5 (non officiel)"

[Registry]
; Même entrée que l'interface et installer.ps1 (réglable dans l'interface).
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "OpenHeadset Control"; ValueData: """{app}\momentum-winui.exe"" --cache"; Tasks: demarrage; Flags: uninsdeletevalue
; L'entrée reste à la désinstallation si l'utilisateur l'a créée depuis
; l'interface sans la tâche : elle est retirée ici dans tous les cas.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "OpenHeadset Control"; Flags: uninsdeletevalue dontcreatekey

[UninstallDelete]
; Dossiers de travail d'installer.ps1 (version précédente gardée).
Type: filesandordirs; Name: "{localappdata}\Programs\momentum-ctl-win.ancien"
Type: filesandordirs; Name: "{localappdata}\Programs\momentum-ctl-win.nouveau"

[Run]
Filename: "{app}\momentum-winui.exe"; Description: "Lancer OpenHeadset Control"; Flags: nowait postinstall skipifsilent

[Code]
const
  RuntimeAbsent = 'Windows App Runtime 2 introuvable : l''interface WinUI ne démarrerait pas, rien n''a été installé. Installer le runtime depuis https://aka.ms/windowsappsdk/2.0/latest/windowsappruntimeinstall-x64.exe (ou winget install Microsoft.WindowsAppRuntime.2), puis relancer.';

function PowerShell(Script: String): Integer;
var
  Code: Integer;
begin
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "' + Script + '"',
    '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Code := -1;
  Result := Code;
end;

function RuntimePresent(): Boolean;
begin
  Result := PowerShell('if (Get-AppxPackage -Name ''Microsoft.WindowsAppRuntime.2*'') { exit 0 } else { exit 1 }') = 0;
end;

{ Processus lancés depuis le dossier d'installation (et l'ancienne icône
  Slint), comme Stop-Installed d'installer.ps1. }
procedure StopInstalled();
var
  Dossier: String;
begin
  Dossier := ExpandConstant('{localappdata}\Programs\momentum-ctl-win');
  PowerShell('Get-Process momentum-tray, momentum-winui, momentum-daemon -ErrorAction SilentlyContinue | ' +
    'Where-Object { $_.Name -eq ''momentum-tray'' -or ($_.Path -and $_.Path.StartsWith(''' + Dossier + ''', ''OrdinalIgnoreCase'')) } | ' +
    'Stop-Process -Force; Start-Sleep -Milliseconds 500');
end;

function InitializeSetup(): Boolean;
begin
  Result := True;
  if (Pos('/SANSRUNTIME', UpperCase(GetCmdTail)) = 0) and not RuntimePresent() then
  begin
    SuppressibleMsgBox(RuntimeAbsent, mbCriticalError, MB_OK, IDOK);
    Result := False;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopInstalled();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopInstalled();
  Result := True;
end;
