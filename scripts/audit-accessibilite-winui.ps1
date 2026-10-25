# Audit d'accessibilité de WinUI par UI Automation (CI, windows-latest) :
# momentum-winui --demo, chaque page ouverte par la navigation, puis chaque
# commande (bouton, interrupteur, curseur, liste, champ…) doit porter un nom
# accessible et être atteignable au clavier. Puis parcours réel au clavier
# sur l'accueil : Tab jusqu'à l'interrupteur anti-vent, Espace le bascule,
# et la tabulation fait le tour de la fenêtre sans piège. Échec avec la
# liste des commandes fautives.
#
# Usage : .\scripts\audit-accessibilite-winui.ps1 -Exe target\debug\momentum-winui.exe [-Langue en]
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [ValidateSet('fr', 'en')][string]$Langue = 'fr'
)
$env:OPENHEADSET_LANG = $Langue
$NomAntiVent = @{ fr = 'Anti-vent'; en = 'Anti-wind' }[$Langue]

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Premier {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
$AE = [System.Windows.Automation.AutomationElement]
$CT = [System.Windows.Automation.ControlType]
$Scope = [System.Windows.Automation.TreeScope]

# Types de commandes qu'un lecteur d'écran annonce et qu'on actionne.
$Commandes = @(
    $CT::Button, $CT::CheckBox, $CT::ComboBox, $CT::Edit, $CT::Hyperlink,
    $CT::ListItem, $CT::MenuItem, $CT::RadioButton, $CT::Slider, $CT::Spinner,
    $CT::SplitButton, $CT::TabItem, $CT::TreeItem
)

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

# Commandes visibles de la page sans nom accessible ou hors du parcours
# clavier.
function Get-Fautes($page, $tag) {
    $fautes = @()
    foreach ($e in $page.FindAll($Scope::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) {
        $c = $e.Current
        if ($c.IsOffscreen -or -not $c.IsEnabled) { continue }
        if ($Commandes -notcontains $c.ControlType) { continue }
        # Flèches des barres de défilement : parcourues par la molette et les
        # touches de la zone défilée, pas par la tabulation.
        if ($c.AutomationId -match '^(Vertical|Horizontal)(Small|Large)(Increase|Decrease)$') { continue }
        $type = $c.ControlType.ProgrammaticName -replace '^ControlType\.', ''
        $id = if ($c.AutomationId) { " #$($c.AutomationId)" } else { '' }
        if ([string]::IsNullOrWhiteSpace($c.Name)) {
            $fautes += "page $tag : $type$id sans nom accessible"
        }
        if (-not $c.IsKeyboardFocusable -and $c.ControlType -ne $CT::ListItem) {
            $fautes += "page $tag : $type « $($c.Name) »$id hors du parcours clavier"
        }
    }
    return $fautes
}

# Touche envoyée seulement si la fenêtre essayée est au premier plan :
# jamais de frappe dans une autre fenêtre.
function Send-Touche($handle, $touche) {
    if ([Premier]::GetForegroundWindow() -ne $handle) {
        [void][Premier]::SetForegroundWindow($handle)
        Start-Sleep -Milliseconds 300
        if ([Premier]::GetForegroundWindow() -ne $handle) { throw "fenêtre de WinUI plus au premier plan : parcours clavier arrêté" }
    }
    [System.Windows.Forms.SendKeys]::SendWait($touche)
    Start-Sleep -Milliseconds 150
}

function Get-Cle($e) { ($e.GetRuntimeId() -join '.') }

$p = Start-Process $Exe -ArgumentList '--demo' -PassThru
try {
    $limite = (Get-Date).AddSeconds(30)
    while ($p.MainWindowHandle -eq 0 -and (Get-Date) -lt $limite) {
        Start-Sleep -Milliseconds 500
        $p.Refresh()
    }
    if ($p.MainWindowHandle -eq 0) { throw "fenêtre de WinUI introuvable" }
    $fenetre = $AE::FromHandle($p.MainWindowHandle)
    $fautes = @()
    $pages = 'accueil', 'son', 'scenes', 'batterie', 'audio', 'reglages', 'apropos'
    foreach ($tag in $pages) {
        $item = Find-First $fenetre $AE::AutomationIdProperty "nav-$tag"
        if (-not $item) { throw "élément de navigation nav-$tag introuvable" }
        if ([string]::IsNullOrWhiteSpace($item.Current.Name)) { $fautes += "navigation nav-$tag sans nom accessible" }
        $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
        $page = Find-First $fenetre $AE::AutomationIdProperty "page-$tag"
        if (-not $page) { throw "page $tag non affichée après la navigation" }
        Start-Sleep -Milliseconds 500
        $f = @(Get-Fautes $page $tag)
        "page $tag : $($f.Count) faute(s)"
        $fautes += $f
    }
    if ($fautes.Count) {
        $fautes | ForEach-Object { "  $_" }
        throw "$($fautes.Count) commande(s) inaccessible(s)"
    }

    # Parcours clavier sur l'accueil.
    $handle = $p.MainWindowHandle
    $item = Find-First $fenetre $AE::AutomationIdProperty 'nav-accueil'
    $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    [void](Find-First $fenetre $AE::AutomationIdProperty 'page-accueil')
    [void][Premier]::SetForegroundWindow($handle)
    $item.SetFocus()
    Start-Sleep -Milliseconds 300
    $depart = Get-Cle $AE::FocusedElement
    $vus = @{}
    $bascule = $null
    $tour = $false
    for ($n = 1; $n -le 120; $n++) {
        Send-Touche $handle '{TAB}'
        $f = $AE::FocusedElement
        $cle = Get-Cle $f
        if ($cle -eq $depart) { $tour = $true; break }
        $vus[$cle] = $f.Current.Name
        if (-not $bascule -and $f.Current.Name -eq $NomAntiVent -and
            [bool]$f.GetCurrentPropertyValue($AE::IsTogglePatternAvailableProperty)) {
            $etat = $f.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
            Send-Touche $handle ' '
            $limite = (Get-Date).AddSeconds(10)
            do {
                Start-Sleep -Milliseconds 300
                $apres = $AE::FocusedElement.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
            } while ($apres -eq $etat -and (Get-Date) -lt $limite)
            if ($apres -eq $etat) { throw "Espace sur « $NomAntiVent » sans effet ($etat)" }
            $bascule = "$etat -> $apres"
            "clavier : « $NomAntiVent » atteint en $n tabulation(s), Espace : $bascule"
        }
    }
    if (-not $bascule) { throw "« $NomAntiVent » jamais atteint au clavier ($($vus.Count) arrêts)" }
    if (-not $tour) { throw "la tabulation ne revient pas au départ en 120 arrêts (piège clavier ?)" }
    "clavier : tour complet en $n tabulation(s), $($vus.Count) arrêt(s)"
    'AUDIT ACCESSIBILITÉ WINUI RÉUSSI'
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
