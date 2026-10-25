# Construit l'installateur Windows (Inno Setup 6) à partir des exécutables
# déjà compilés : openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe
# dans -Sortie. Inno Setup est pris s'il est installé, sinon installé par
# Chocolatey (runners GitHub).
param(
    [Parameter(Mandatory)][string]$Version,
    [string]$Bin = 'target\release',
    [string]$Sortie = '.'
)
$ErrorActionPreference = 'Stop'
$racine = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$iscc = @(
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe')
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    choco install innosetup --no-progress -y | Out-Null
    $iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
}
foreach ($f in 'momentum-winui.exe', 'momentum-daemon.exe', 'ctl.exe') {
    if (-not (Test-Path (Join-Path $Bin $f))) { throw "Exécutable manquant : $Bin\$f" }
}
New-Item -ItemType Directory -Force $Sortie | Out-Null
& $iscc /Q "/DVersion=$Version" "/DBin=$(Resolve-Path $Bin)" "/DRacine=$racine" "/O$(Resolve-Path $Sortie)" (Join-Path $PSScriptRoot 'openheadset-control.iss')
if ($LASTEXITCODE -ne 0) { throw "ISCC a échoué (code $LASTEXITCODE)" }
$exe = Join-Path $Sortie "openheadset-control-v$Version-windows-x86_64-installateur.exe"
if (-not (Test-Path $exe)) { throw "Installateur introuvable : $exe" }
Get-Item $exe | Select-Object Name, Length
