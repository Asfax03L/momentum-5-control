# Prépare les manifestes winget d'une version publiée, sans rien soumettre :
#   .\packaging\winget\preparer.ps1 -Version 0.14.0 -Sortie dossier
# L'installateur est téléchargé depuis la publication GitHub et son
# empreinte calculée ; les trois manifestes (version, installateur, fiche
# en français) sont écrits dans dossier\a\Asfax03L\OpenHeadsetControl\X.Y.Z,
# l'arborescence du dépôt microsoft/winget-pkgs. Validation :
#   winget validate --manifest <dossier de la version>
# Soumission (docs/SOUMISSIONS.md) : seulement avec l'accord du mainteneur.
param(
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$Sortie,
    # Installateur local (essai avant publication) ; l'URL reste celle de
    # la publication.
    [string]$Installateur
)
$ErrorActionPreference = 'Stop'
$id = 'Asfax03L.OpenHeadsetControl'
$nom = "openheadset-control-v$Version-windows-x86_64-installateur.exe"
$url = "https://github.com/Asfax03L/openheadset-control/releases/download/v$Version/$nom"
$dossier = Join-Path $Sortie "a\Asfax03L\OpenHeadsetControl\$Version"
New-Item -ItemType Directory -Force $dossier | Out-Null
if (-not $Installateur) {
    $Installateur = Join-Path $dossier $nom
    Invoke-WebRequest $url -OutFile $Installateur
}
$empreinte = (Get-FileHash -Algorithm SHA256 $Installateur).Hash
$schema = '1.10.0'

function Ecrire($fichier, $type, $corps) {
    $texte = "# yaml-language-server: `$schema=https://aka.ms/winget-manifest.$type.$schema.schema.json`n" +
        "PackageIdentifier: $id`nPackageVersion: $Version`n" + $corps +
        "ManifestType: $type`nManifestVersion: $schema`n"
    [IO.File]::WriteAllText((Join-Path $dossier $fichier), $texte, (New-Object Text.UTF8Encoding $false))
}

Ecrire "$id.yaml" 'version' "DefaultLocale: fr-FR`n"
Ecrire "$id.installer.yaml" 'installer' @"
InstallerType: inno
Scope: user
UpgradeBehavior: install
ProductCode: '{67997F7F-2010-488F-9966-CE13D6A42173}_is1'
ReleaseDate: $(Get-Date -Format 'yyyy-MM-dd')
Dependencies:
  PackageDependencies:
  - PackageIdentifier: Microsoft.WindowsAppRuntime.2
Installers:
- Architecture: x64
  InstallerUrl: $url
  InstallerSha256: $empreinte

"@
Ecrire "$id.locale.fr-FR.yaml" 'defaultLocale' @"
PackageLocale: fr-FR
Publisher: Asfax03L
PublisherUrl: https://github.com/Asfax03L
PublisherSupportUrl: https://github.com/Asfax03L/openheadset-control/issues
PackageName: OpenHeadset Control
PackageUrl: https://github.com/Asfax03L/openheadset-control
License: MIT OR Apache-2.0
LicenseUrl: https://github.com/Asfax03L/openheadset-control/blob/main/LICENSE-MIT
ShortDescription: Réglages du casque Sennheiser MOMENTUM 5 (non officiel)
Description: Contrôle léger du casque Sennheiser MOMENTUM 5 depuis le bureau (réduction de bruit, égaliseur, scènes, batterie). Projet non officiel, sans lien avec Sennheiser ni Sonova ; seules les écritures validées sur le casque sont envoyées.
Moniker: openheadset-control
Tags:
- casque
- sennheiser
- momentum
- bluetooth
ReleaseNotesUrl: https://github.com/Asfax03L/openheadset-control/releases/tag/v$Version

"@
Get-ChildItem $dossier -Filter '*.yaml' | Select-Object Name
