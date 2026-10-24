# Vérifier un fichier publié

Chaque publication (page Releases du dépôt) fournit, pour chaque fichier :

- son **empreinte SHA-256** (`<fichier>.sha256`, au format de
  `sha256sum`) : le fichier n'a pas été abîmé ni remplacé en route ;
- une **attestation de provenance** signée par GitHub (Sigstore) : le
  fichier a bien été construit par le workflow `release.yml` de ce dépôt,
  à partir de la révision de l'étiquette, sur les machines de GitHub.

Elle fournit aussi l'**inventaire des dépendances** (SBOM CycloneDX 1.5,
`…-sbom-cyclonedx.tar.gz`) : un fichier par paquet Rust de l'espace de
travail, avec chaque bibliothèque, sa version et sa licence (cible
Windows et Linux).

Les exécutables ne sont pas signés par un certificat de développeur :
ces vérifications remplacent cette signature, sans la valoir aux yeux de
Windows (SmartScreen peut toujours demander une confirmation).

## Empreinte

Linux, dans le dossier du téléchargement :

```bash
sha256sum -c openheadset-control-v0.11.0-linux-x86_64.tar.gz.sha256
```

Windows (PowerShell) : comparer les deux valeurs affichées.

```powershell
Get-FileHash openheadset-control-v0.11.0-windows-x86_64.zip -Algorithm SHA256
```
```powershell
Get-Content openheadset-control-v0.11.0-windows-x86_64.zip.sha256
```

## Provenance

Avec l'outil `gh` de GitHub (une connexion à GitHub est demandée) :

```bash
gh attestation verify openheadset-control-v0.11.0-linux-x86_64.tar.gz --repo Asfax03L/openheadset-control
```

La réponse indique le dépôt, le workflow et la révision qui ont produit le
fichier. Un fichier modifié, ou construit ailleurs, est refusé. Les
attestations existent à partir de la v0.11.0.

## Inventaire des dépendances

```bash
tar -xzf openheadset-control-v0.11.0-sbom-cyclonedx.tar.gz
```

Les fichiers `*.cdx.json` se lisent avec tout outil CycloneDX (par exemple
pour chercher une bibliothèque touchée par une faille). La CI vérifie déjà
les failles connues à chaque modification (`cargo deny`, base RustSec).

## Compilation reproductible : étude du 23/10/2026

Essai : `ctl` compilé en release deux fois, dans deux dossiers
différents, sur le PC fixe (conteneur Fedora 44, Rust 1.99.0 de
`rust-toolchain.toml`, `Cargo.lock` verrouillé).

| Essai | Résultat |
|---|---|
| Deux dossiers, options par défaut | **binaires identiques** (même SHA-256) ; aucun chemin du dossier de construction dans le binaire |
| Même essai avec `--remap-path-prefix` | identiques entre eux, mais différents du cas précédent : l'option n'est pas nécessaire |

Ce qui reste à établir avant de dire les publications reproductibles :

- **une autre machine** (autre dossier personnel, autre installation de
  Rust) et la machine de la CI ;
- **Windows** : `momentum-winui.exe`, `ctl.exe` (en-têtes PE, ressources) ;
- **le client KDE**, qui compile du C++ (cxx-qt) contre le Qt du système :
  sa reproductibilité dépend aussi de la version de Qt ;
- **les archives** (`.zip`, `.tar.gz`, `.deb`, `.rpm`, Flatpak), qui
  enregistrent des dates de fichiers : il faudrait fixer `SOURCE_DATE_EPOCH`
  et l'ordre des fichiers.

En attendant, l'attestation de provenance dit d'où vient chaque fichier, et
l'empreinte qu'il n'a pas changé.
