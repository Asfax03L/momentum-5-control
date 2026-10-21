# Installe OpenHeadset Control pour l'utilisateur courant (sans droits
# administrateur), dans %LOCALAPPDATA%\Programs\momentum-ctl-win :
#   - interface WinUI 3 : momentum-winui.exe, le service momentum-daemon.exe
#     (lancé par l'interface) et ctl.exe ; demande le Windows App Runtime 2
#     (vérifié ci-dessous) ;
#   - prend les exécutables posés à côté de ce script (archive publiée),
#     sinon compile la version release depuis le dépôt (Rust nécessaire) ;
#   - inscrit l'interface au démarrage de Windows (HKCU\...\Run) ;
#   - retire l'ancienne icône Slint (momentum-tray.exe) si elle est installée ;
#   - ajoute « OpenHeadset Control » au menu Démarrer (logo de l'exécutable) ;
#   - la lance.
#
# Désinstallation : .\scripts\installer.ps1 -Desinstaller
param(
    [switch]$Desinstaller
)

$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\momentum-ctl-win'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
# Entrée de démarrage (la même que dans l'interface) ; l'ancienne icône
# Slint utilisait « momentum-ctl-win ».
$runWinui = 'OpenHeadset Control'
$runSlint = 'momentum-ctl-win'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenHeadset Control.lnk'

# Arrête ce qui tourne (les fichiers seraient verrouillés sinon).
Get-Process momentum-tray, momentum-winui, momentum-daemon -ErrorAction SilentlyContinue |
    Stop-Process -Force
Start-Sleep -Milliseconds 500

if ($Desinstaller) {
    Remove-ItemProperty -Path $runKey -Name $runSlint -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $runKey -Name $runWinui -ErrorAction SilentlyContinue
    if (Test-Path $shortcut) { Remove-Item -Force $shortcut }
    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    Write-Output "OpenHeadset Control désinstallé (journaux et réglages restent dans $env:LOCALAPPDATA\momentum-ctl-win)."
    return
}

$files = 'momentum-winui.exe', 'momentum-daemon.exe', 'ctl.exe'
$main = 'momentum-winui.exe'
# WinUI 3 demande le Windows App Runtime 2 (paquet du système).
if (-not (Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2*' -ErrorAction SilentlyContinue)) {
    Write-Warning ('Windows App Runtime 2 introuvable : WinUI ne démarrera pas sans lui. ' +
        'Téléchargement : https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads')
}

$prebuilt = @($files | ForEach-Object { Join-Path $PSScriptRoot $_ })
if (@($prebuilt | Where-Object { -not (Test-Path $_) }).Count -eq 0) {
    # Archive publiée : exécutables déjà compilés, pas besoin de Rust.
    $sources = $prebuilt
} else {
    $repo = Split-Path -Parent $PSScriptRoot
    Push-Location $repo
    try {
        cargo build --release -p momentum-winui -p momentum-daemon -p momentum-cli
        if ($LASTEXITCODE -ne 0) { throw 'la compilation a échoué' }
    } finally {
        Pop-Location
    }
    $sources = @($files | ForEach-Object { Join-Path $repo "target\release\$_" })
}

New-Item -ItemType Directory -Force $dest | Out-Null
# L'ancienne icône Slint ne doit plus démarrer ni rester sur le disque.
Remove-Item -Force (Join-Path $dest 'momentum-tray.exe') -ErrorAction SilentlyContinue
Remove-ItemProperty -Path $runKey -Name $runSlint -ErrorAction SilentlyContinue
Copy-Item $sources $dest -Force
$exe = Join-Path $dest $main

Set-ItemProperty -Path $runKey -Name $runWinui -Value "`"$exe`" --cache"

# Menu Démarrer : relancer l'exécutable montre la fenêtre de l'instance en cours.
$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$link.TargetPath = $exe
$link.WorkingDirectory = $dest
$link.Description = 'OpenHeadset Control : réglages du MOMENTUM 5 (non officiel)'
$link.Save()
Start-Process -FilePath $exe

Write-Output "Installé dans $dest"
Write-Output 'Lancement au démarrage de Windows : activé (modifiable dans les réglages de l''interface).'
