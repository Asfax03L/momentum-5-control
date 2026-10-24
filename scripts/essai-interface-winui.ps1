# Essai de l'interface WinUI par UI Automation (CI, windows-latest) :
# momentum-winui --demo (casque simulé, aucun service), chaque page ouverte
# par la navigation et reconnue à son repère, puis l'anti-vent simulé
# basculé et relu à l'affichage. Capture de la fenêtre en cas d'échec.
#
# Usage : .\scripts\essai-interface-winui.ps1 -Exe target\debug\momentum-winui.exe
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Capture = 'essai-interface-winui.png'
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
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

# Interrupteur par son nom accessible : l'étiquette voisine porte le même
# nom, seul l'interrupteur sait basculer (TogglePattern).
function Find-Toggle($root, $name, [int]$secondes = 10) {
    $condition = New-Object System.Windows.Automation.AndCondition(
        (New-Object System.Windows.Automation.PropertyCondition($AE::NameProperty, $name)),
        (New-Object System.Windows.Automation.PropertyCondition($AE::IsTogglePatternAvailableProperty, $true)))
    $limite = (Get-Date).AddSeconds($secondes)
    do {
        $trouve = $root.FindFirst($Scope::Descendants, $condition)
        if ($trouve) { return $trouve }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $limite)
    return $null
}

function Save-Capture($handle) {
    try {
        $r = [System.Windows.Automation.AutomationElement]::FromHandle($handle).Current.BoundingRectangle
        $bmp = New-Object System.Drawing.Bitmap([int]$r.Width, [int]$r.Height)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen([int]$r.X, [int]$r.Y, 0, 0, $bmp.Size)
        $bmp.Save($Capture)
        "Capture : $Capture"
    } catch {
        "Capture impossible : $_"
    }
}

$p = Start-Process $Exe -ArgumentList '--demo' -PassThru
try {
    $limite = (Get-Date).AddSeconds(30)
    while ($p.MainWindowHandle -eq 0 -and (Get-Date) -lt $limite) {
        Start-Sleep -Milliseconds 500
        $p.Refresh()
    }
    if ($p.MainWindowHandle -eq 0) { throw "fenêtre de WinUI introuvable" }
    $fenetre = $AE::FromHandle($p.MainWindowHandle)
    try {
        $pages = 'accueil', 'son', 'scenes', 'batterie', 'audio', 'reglages', 'apropos'
        foreach ($tag in $pages) {
            $item = Find-First $fenetre $AE::AutomationIdProperty "nav-$tag"
            if (-not $item) { throw "élément de navigation nav-$tag introuvable" }
            $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
            if (-not (Find-First $fenetre $AE::AutomationIdProperty "page-$tag")) {
                throw "page $tag non affichée après la navigation"
            }
            "page $tag : affichée"
        }
        # Réglage simulé : l'anti-vent change, l'affichage le relit.
        $item = Find-First $fenetre $AE::AutomationIdProperty 'nav-accueil'
        $item.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
        $interrupteur = Find-Toggle $fenetre 'Anti-vent'
        if (-not $interrupteur) { throw "interrupteur « Anti-vent » introuvable (nom accessible)" }
        $bascule = $interrupteur.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
        $avant = $bascule.Current.ToggleState
        $bascule.Toggle()
        $limite = (Get-Date).AddSeconds(10)
        do {
            Start-Sleep -Milliseconds 300
            $interrupteur = Find-Toggle $fenetre 'Anti-vent'
            $apres = $interrupteur.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern).Current.ToggleState
        } while ($apres -eq $avant -and (Get-Date) -lt $limite)
        if ($apres -eq $avant) { throw "anti-vent simulé non relu à l'affichage ($avant)" }
        "anti-vent : $avant -> $apres"
        'ESSAI INTERFACE WINUI RÉUSSI'
    } catch {
        Save-Capture $p.MainWindowHandle
        throw
    }
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
