# Essai de l'installateur Windows (Inno Setup), en silencieux :
#   1. (si -Precedente) version précédente installée par installer.ps1 ;
#   2. installation par l'installateur (mise à jour par-dessus) ;
#   3. réinstallation par-dessus elle-même ;
#   4. désinstallation : programmes, raccourci et démarrage retirés,
#      réglages gardés.
# Arrêt à la première anomalie. Pour la CI et pour un essai sur un PC.
param(
    [Parameter(Mandatory)][string]$Installateur,
    [Parameter(Mandatory)][string]$Version,
    # Dossier d'une archive Windows précédente (avec installer.ps1).
    [string]$Precedente,
    # Sans Windows App Runtime 2 (CI sans runtime seulement).
    [switch]$SansRuntime
)
$ErrorActionPreference = 'Stop'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\momentum-ctl-win'
$ctl = Join-Path $dest 'ctl.exe'
$raccourci = Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenHeadset Control.lnk'
$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$desinstallation = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{67997F7F-2010-488F-9966-CE13D6A42173}_is1'
$reglages = Join-Path $env:LOCALAPPDATA 'momentum-ctl-win'

function Verifier($condition, $message) {
    if (-not $condition) { throw "Échec : $message" }
    "ok : $message"
}

function Installer {
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/TASKS=demarrage')
    if ($SansRuntime) { $arguments += '/SANSRUNTIME' }
    $p = Start-Process $Installateur -ArgumentList $arguments -Wait -PassThru
    Verifier ($p.ExitCode -eq 0) "installateur terminé (code $($p.ExitCode))"
    $v = & $ctl --version
    Verifier ($v -eq "ctl $Version") "version installée : $v"
    Verifier (Test-Path $raccourci) 'raccourci du menu Démarrer'
    Verifier ((Get-ItemProperty $run -ErrorAction SilentlyContinue).'OpenHeadset Control' -like '*momentum-winui.exe" --cache') 'démarrage avec Windows'
    Verifier ((Get-ItemProperty $desinstallation).DisplayVersion -eq $Version) 'inscrit dans Applications installées'
}

if ($Precedente) {
    '== version précédente (installer.ps1)'
    $script = Join-Path $Precedente 'installer.ps1'
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script)
    if ($SansRuntime) { $arguments += '-SansRuntime' }
    & powershell @arguments
    Get-Process momentum-winui, momentum-daemon -ErrorAction SilentlyContinue | Stop-Process -Force
    "précédente : $(& $ctl --version)"
}
New-Item -ItemType Directory -Force $reglages | Out-Null
$temoin = Join-Path $reglages 'essai-installateur.txt'
if (-not (Test-Path $temoin)) { 'témoin' | Set-Content $temoin }

'== installation ou mise à jour'
Installer
'== réinstallation par-dessus'
Installer

'== désinstallation'
$desinstalleur = Join-Path $dest 'unins000.exe'
$p = Start-Process $desinstalleur -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
Verifier ($p.ExitCode -eq 0) "désinstallation terminée (code $($p.ExitCode))"
# Le désinstalleur se retire après coup.
foreach ($i in 1..20) { if (-not (Test-Path $ctl)) { break }; Start-Sleep -Milliseconds 500 }
Verifier (-not (Test-Path $ctl)) 'programmes retirés'
Verifier (-not (Test-Path $raccourci)) 'raccourci retiré'
Verifier ($null -eq (Get-ItemProperty $run -ErrorAction SilentlyContinue).'OpenHeadset Control') 'démarrage retiré'
Verifier (-not (Test-Path $desinstallation)) 'retiré des Applications installées'
Verifier (Test-Path $temoin) 'réglages gardés'
Remove-Item $temoin
'Essai de l''installateur réussi.'
