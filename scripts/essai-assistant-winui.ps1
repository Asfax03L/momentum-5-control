# Essai de l'assistant de premier lancement de WinUI par UI Automation (CI,
# windows-latest) : momentum-winui --demo --assistant, chaque étape passée
# avec « Suivant » (titre de niveau 2 lu), « Terminer » rend la page
# d'accueil. Aucun fichier écrit en démonstration.
#
# Usage : .\scripts\essai-assistant-winui.ps1 -Exe target\debug\momentum-winui.exe [-Langue en]
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [ValidateSet('fr', 'en')][string]$Langue = 'fr'
)
$env:OPENHEADSET_LANG = $Langue
$Noms = @{
    fr = @{ Suivant = 'Suivant'; Terminer = 'Terminer'; Bluetooth = 'Ouvrir les paramètres Bluetooth'; AntiVent = 'Anti-vent' }
    en = @{ Suivant = 'Next'; Terminer = 'Finish'; Bluetooth = 'Open Bluetooth settings'; AntiVent = 'Anti-wind' }
}[$Langue]

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$AE = [System.Windows.Automation.AutomationElement]
$Scope = [System.Windows.Automation.TreeScope]

function Find-First($root, $property, $value, [int]$secondes = 10) {
    $condition = New-Object System.Windows.Automation.PropertyCondition($property, $value)
    $limite = (Get-Date).AddSeconds($secondes)
    do {
        $trouve = $root.FindFirst($Scope::Descendants, $condition)
        if ($trouve) { return $trouve }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $limite)
    return $null
}

# Titre de niveau 2 affiché (titre de l'étape).
function Get-Titre($root) {
    foreach ($e in $root.FindAll($Scope::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
        if ("$($e.GetCurrentPropertyValue($AE::HeadingLevelProperty))" -eq 'Level2') { return $e.Current.Name }
    }
}

function Invoke-Bouton($root, $nom) {
    $bouton = Find-First $root $AE::NameProperty $nom
    if (-not $bouton) { throw "bouton « $nom » introuvable" }
    $bouton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
}

$p = Start-Process $Exe -ArgumentList '--demo', '--assistant' -PassThru
try {
    $limite = (Get-Date).AddSeconds(30)
    while ($p.MainWindowHandle -eq 0 -and (Get-Date) -lt $limite) {
        Start-Sleep -Milliseconds 500
        $p.Refresh()
    }
    if ($p.MainWindowHandle -eq 0) { throw "fenêtre de WinUI introuvable" }
    $fenetre = $AE::FromHandle($p.MainWindowHandle)
    $vus = @()
    $bluetooth = $false
    for ($n = 1; $n -le 10; $n++) {
        Start-Sleep -Milliseconds 500
        $titre = Get-Titre $fenetre
        if (-not $titre) { throw "étape $n sans titre" }
        if ($vus -contains $titre) { throw "étape « $titre » vue deux fois" }
        $vus += $titre
        "étape $n : $titre"
        if (Find-First $fenetre $AE::NameProperty $Noms.Bluetooth 1) { $bluetooth = $true }
        if (Find-First $fenetre $AE::NameProperty $Noms.Terminer 1) { break }
        Invoke-Bouton $fenetre $Noms.Suivant
    }
    if (-not $bluetooth) { throw "étape Bluetooth sans bouton « $($Noms.Bluetooth) »" }
    if ($vus.Count -lt 5) { throw "seulement $($vus.Count) étape(s)" }
    Invoke-Bouton $fenetre $Noms.Terminer
    if (-not (Find-First $fenetre $AE::NameProperty $Noms.AntiVent)) { throw "accueil non rendu après « $($Noms.Terminer) »" }
    'ESSAI ASSISTANT WINUI RÉUSSI'
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
