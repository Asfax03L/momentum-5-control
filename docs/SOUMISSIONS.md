# Soumissions aux catalogues (préparées, non envoyées)

Chaque soumission publie le projet sous le compte du mainteneur, dans un
dépôt ou un site qui n'est pas le nôtre : elle n'est faite qu'avec son
accord. Flathub a sa propre page : [`FLATHUB.md`](FLATHUB.md).

| Catalogue | Fichiers préparés | Essai | État |
|---|---|---|---|
| **winget** (Windows) | `packaging/winget/preparer.ps1` : manifestes version, installateur (Inno Setup, par utilisateur, dépendance `Microsoft.WindowsAppRuntime.2`), fiche en français | `winget validate` réussi | non soumis |
| **AUR** (Arch, CachyOS) | `packaging/aur/PKGBUILD`, rempli par `packaging/aur/preparer.sh` | CI : `makepkg`, `namcap`, installation, chargement de l'interface, retrait | non publié |
| **KDE Store** (widget Plasma) | `openheadset-control-vX.Y.Z.plasmoid` joint à chaque publication | CI : métadonnées à la racine ; Bazzite : installation, mise à jour (0.13 → 0.14) et retrait par `kpackagetool6`, dans un dossier de données temporaire | non publié |

## winget

Depuis une version publiée (l'installateur est téléchargé et empreint) :

```powershell
.\packaging\winget\preparer.ps1 -Version 0.14.0 -Sortie ..\winget-soumission
```
```powershell
winget validate --manifest ..\winget-soumission\a\Asfax03L\OpenHeadsetControl\0.14.0
```

Soumission (avec l'accord seulement) : PR sur `microsoft/winget-pkgs` qui
ajoute le dossier `manifests/a/Asfax03L/OpenHeadsetControl/0.14.0`, par
exemple avec `wingetcreate submit`. Les relecteurs de winget installent
le paquet dans une machine propre : l'installateur doit y passer sans
interaction (`/VERYSILENT`, géré par le type `inno`).

## AUR

```bash
sh packaging/aur/preparer.sh 0.14.0 ../aur-openheadset-control
```

Le dossier contient le `PKGBUILD` (version et empreinte de l'archive de
l'étiquette) et le `.SRCINFO` (si `makepkg` est présent). Publication
(avec l'accord seulement) : compte AUR avec clé SSH, puis
`git clone ssh://aur@aur.archlinux.org/openheadset-control.git`, copie des
deux fichiers, commit et push.

## KDE Store

Le widget seul est publié à chaque version (`.plasmoid`, une archive zip
avec `metadata.json` à la racine). Installation et mise à jour à la main :

```bash
kpackagetool6 --type Plasma/Applet --upgrade openheadset-control-v0.14.0.plasmoid || kpackagetool6 --type Plasma/Applet --install openheadset-control-v0.14.0.plasmoid
```

Publication (avec l'accord seulement) : compte sur store.kde.org,
catégorie « Plasma 6 Applets », envoi du fichier `.plasmoid` ; le widget
apparaît ensuite dans « Obtenir de nouveaux composants graphiques ».
