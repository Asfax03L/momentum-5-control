# Budget de ressources (docs/RESSOURCES.md), Windows : lance l'interface
# WinUI en démonstration (casque simulé, aucun service), puis relève au bout
# de -Duree secondes de repos la mémoire (plage de travail, Mio) et le temps
# processeur consommé (secondes). Échoue au-delà des seuils.
#
# Usage : .\scripts\mesurer-ressources.ps1 -Binaires target\release [-Duree 30]
# Le service n'est pas mesuré ici : sous Windows, il prendrait le canal
# nommé du service déjà installé (relevé Linux dans la tâche ressources).
param(
    [Parameter(Mandatory = $true)][string]$Binaires,
    [int]$Duree = 30,
    # Seuils larges (relevés du 23/10/2026 multipliés par 3 environ).
    [double]$SeuilMio = 400,
    [double]$SeuilCpu = 20
)

$ErrorActionPreference = 'Stop'
$exe = Join-Path $Binaires 'momentum-winui.exe'
$p = Start-Process $exe -ArgumentList '--demo' -PassThru
try {
    Start-Sleep $Duree
    $p.Refresh()
    if ($p.HasExited) { throw "WinUI s'est arrêté avant le relevé (code $($p.ExitCode))" }
    $mio = [math]::Round($p.WorkingSet64 / 1MB, 1)
    $cpu = [math]::Round($p.TotalProcessorTime.TotalSeconds, 2)
    $table = @(
        "Relevé après $Duree s au repos :",
        '',
        '| Processus | Mémoire (plage de travail) | Processeur |',
        '|---|---|---|',
        "| momentum-winui --demo | $mio Mio | $cpu s |"
    )
    $table | Write-Output
    if ($env:GITHUB_STEP_SUMMARY) { $table | Out-File -Append -Encoding utf8 $env:GITHUB_STEP_SUMMARY }
    if ($mio -gt $SeuilMio) { throw "momentum-winui : $mio Mio > $SeuilMio Mio" }
    if ($cpu -gt $SeuilCpu) { throw "momentum-winui : $cpu s > $SeuilCpu s" }
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
