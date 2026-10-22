# Installe OpenHeadset Control pour l'utilisateur courant (sans droits
# administrateur), dans %LOCALAPPDATA%\Programs\momentum-ctl-win :
#   - interface WinUI 3 : momentum-winui.exe, le service momentum-daemon.exe
#     (lancé par l'interface) et ctl.exe ; demande le Windows App Runtime 2 ;
#   - prend les exécutables posés à côté de ce script (archive publiée),
#     sinon compile la version release depuis le dépôt (Rust nécessaire) ;
#   - inscrit l'interface au démarrage de Windows (HKCU\...\Run) ;
#   - retire l'ancienne icône Slint (momentum-tray.exe) si elle est installée ;
#   - ajoute « OpenHeadset Control » au menu Démarrer et lance l'interface.
#
# Tout est préparé et vérifié avant d'arrêter quoi que ce soit (F08 de la
# revue) : runtime, fichiers, copie dans un dossier « .nouveau ». La version
# remplacée est gardée (dossier « .ancien ») et remise si le remplacement
# échoue ou si l'interface ne démarre pas.
#
# Désinstallation : .\scripts\installer.ps1 -Desinstaller (journaux et
# réglages restent dans %LOCALAPPDATA%\momentum-ctl-win).
param(
    [switch]$Desinstaller,
    # Installe même si le Windows App Runtime 2 n'est pas détecté.
    [switch]$SansRuntime,
    # Essais : autre dossier, sans démarrage automatique, menu ni lancement.
    [string]$Dossier,
    [switch]$SansIntegration
)

$ErrorActionPreference = 'Stop'
$dest = if ($Dossier) { $Dossier } else { Join-Path $env:LOCALAPPDATA 'Programs\momentum-ctl-win' }
$staging = "$dest.nouveau"
$backup = "$dest.ancien"
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
# Entrée de démarrage (la même que dans l'interface) ; l'ancienne icône
# Slint utilisait « momentum-ctl-win ».
$runWinui = 'OpenHeadset Control'
$runSlint = 'momentum-ctl-win'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenHeadset Control.lnk'
$files = 'momentum-winui.exe', 'momentum-daemon.exe', 'ctl.exe'
$main = 'momentum-winui.exe'

# Processus lancés depuis ce dossier (et l'ancienne icône Slint).
function Stop-Installed {
    Get-Process momentum-tray, momentum-winui, momentum-daemon -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -eq 'momentum-tray' -or ($_.Path -and $_.Path.StartsWith($dest, 'OrdinalIgnoreCase')) } |
        Stop-Process -Force
    Start-Sleep -Milliseconds 500
}

if ($Desinstaller) {
    Stop-Installed
    if (-not $SansIntegration) {
        Remove-ItemProperty -Path $runKey -Name $runSlint -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $runKey -Name $runWinui -ErrorAction SilentlyContinue
        if (Test-Path $shortcut) { Remove-Item -Force $shortcut }
    }
    foreach ($dir in $dest, $staging, $backup) {
        if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    }
    Write-Output "OpenHeadset Control désinstallé (journaux et réglages restent dans $env:LOCALAPPDATA\momentum-ctl-win)."
    return
}

# 1. Préparation : rien n'est arrêté ni modifié avant la fin de cette étape.
if (-not $SansRuntime -and -not (Get-AppxPackage -Name 'Microsoft.WindowsAppRuntime.2*' -ErrorAction SilentlyContinue)) {
    throw ('Windows App Runtime 2 introuvable : WinUI ne démarrerait pas, rien n''a été changé. ' +
        'Téléchargement : https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads ' +
        '(ou relancer avec -SansRuntime pour installer quand même).')
}
$prebuilt = @($files | ForEach-Object { Join-Path $PSScriptRoot $_ })
$absent = @($prebuilt | Where-Object { -not (Test-Path $_) })
if ($absent.Count -gt 0 -and $absent.Count -lt $prebuilt.Count) {
    throw "Archive incomplète, rien n'a été changé : manque $(($absent | ForEach-Object { Split-Path -Leaf $_ }) -join ', ')."
}
if ($absent.Count -eq 0) {
    # Archive publiée : exécutables déjà compilés, pas besoin de Rust.
    $sources = $prebuilt
} else {
    $repo = Split-Path -Parent $PSScriptRoot
    Push-Location $repo
    try {
        cargo build --release --locked -p momentum-winui -p momentum-daemon -p momentum-cli
        if ($LASTEXITCODE -ne 0) { throw 'la compilation a échoué, rien n''a été changé' }
    } finally {
        Pop-Location
    }
    $sources = @($files | ForEach-Object { Join-Path $repo "target\release\$_" })
}
$missing = @($sources | Where-Object { -not (Test-Path $_) -or (Get-Item $_).Length -eq 0 })
if ($missing.Count -gt 0) {
    throw "Fichiers manquants, rien n'a été changé : $($missing -join ', ')"
}
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force $staging | Out-Null
Copy-Item $sources $staging -Force
foreach ($f in $files) {
    $copy = Join-Path $staging $f
    $source = $sources | Where-Object { (Split-Path -Leaf $_) -eq $f }
    if (-not (Test-Path $copy) -or (Get-Item $copy).Length -ne (Get-Item $source).Length) {
        Remove-Item -Recurse -Force $staging
        throw "Copie incomplète de $f, rien n'a été changé."
    }
}

# 2. Remplacement : l'ancienne version est gardée et remise en cas d'échec.
Stop-Installed
if (Test-Path $backup) { Remove-Item -Recurse -Force $backup }
$hadPrevious = Test-Path $dest
try {
    if ($hadPrevious) { Rename-Item $dest (Split-Path -Leaf $backup) }
    Rename-Item $staging (Split-Path -Leaf $dest)
} catch {
    if ($hadPrevious -and -not (Test-Path $dest) -and (Test-Path $backup)) {
        Rename-Item $backup (Split-Path -Leaf $dest)
    }
    throw "Remplacement impossible ($($_.Exception.Message)) : ancienne version remise."
}
$exe = Join-Path $dest $main

if ($SansIntegration) {
    Write-Output "Installé dans $dest (essai : sans démarrage, menu ni lancement)."
    return
}

# 3. Intégration et lancement ; sans démarrage de l'interface, retour à
# l'ancienne version.
Remove-ItemProperty -Path $runKey -Name $runSlint -ErrorAction SilentlyContinue
Set-ItemProperty -Path $runKey -Name $runWinui -Value "`"$exe`" --cache"
# Menu Démarrer : relancer l'exécutable montre la fenêtre de l'instance en cours.
$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$link.TargetPath = $exe
$link.WorkingDirectory = $dest
$link.Description = 'OpenHeadset Control : réglages du MOMENTUM 5 (non officiel)'
$link.Save()
Start-Process -FilePath $exe
$started = $false
foreach ($i in 1..20) {
    Start-Sleep -Milliseconds 500
    if (Get-Process momentum-winui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }) {
        $started = $true
        break
    }
}
if (-not $started) {
    if ($hadPrevious -and (Test-Path $backup)) {
        Stop-Installed
        if (Test-Path "$dest.echec") { Remove-Item -Recurse -Force "$dest.echec" }
        Rename-Item $dest "$(Split-Path -Leaf $dest).echec"
        Rename-Item $backup (Split-Path -Leaf $dest)
        Start-Process -FilePath $exe
        throw "L'interface installée n'a pas démarré : ancienne version remise (nouvelle gardée dans $dest.echec)."
    }
    throw "L'interface installée n'a pas démarré (Windows App Runtime 2 présent ?)."
}

Write-Output "Installé dans $dest (version précédente gardée dans $backup)."
Write-Output 'Lancement au démarrage de Windows : activé (modifiable dans les réglages de l''interface).'
