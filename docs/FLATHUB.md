# Soumission à Flathub (préparée, non envoyée)

## Renommage GitHub et contrôle local (29 octobre 2026)

L'identifiant `io.github.asfax03l.openheadset-control` reste inchangé pour
préserver les installations et leur dossier de données. L'ancienne URL du
dépôt redirige vers `Asfax03L/momentum-5-control`. Le workflow vérifie cette
redirection HTTPS et l'identifiant numérique du dépôt avant d'employer une
exception locale limitée à `appid-url-not-reachable`.

Cette exception suit le mécanisme
[local du linter](https://github.com/flathub-infra/flatpak-builder-lint#local-exceptions).
Elle ne constitue pas un accord de Flathub : une soumission à Flathub
demanderait sa propre validation. Les permissions, les autres erreurs du
manifeste et les contrôles du paquet restent vérifiés.

État : **non soumis**. La soumission ouvre une PR publique sur
`flathub/flathub` depuis le compte du mainteneur ; elle n'est faite
qu'avec son accord. Ce document rassemble ce qui est prêt et ce qu'il
reste à faire.

## Ce qui est prêt (v0.13.0)

| Exigence de Flathub | Où | État |
|---|---|---|
| Identifiant d'un domaine contrôlé par l'auteur | `io.github.asfax03l.openheadset-control` (compte GitHub `Asfax03L`) | ✅ |
| Construction sans réseau, sources figées | manifeste sans `--share=network` ; `cargo-sources.json` produit depuis `Cargo.lock` (empreinte de chaque crate) | ✅ CI |
| Contrôle `flatpak-builder-lint` (manifeste et dépôt construit) | tâche `flatpak` de la CI ; seules restent, pour le dépôt, les deux erreurs de copie des captures d'écran, que fait l'infrastructure de Flathub | ✅ CI |
| Fiche AppStream valide, en anglais avec traduction française | `assets/linux/io.github.asfax03l.openheadset-control.metainfo.xml` | ✅ CI |
| Captures d'écran, notes de version, couleurs, classement OARS | fiche AppStream | ✅ |
| Fichier d'activation D-Bus, entrée de menu et icône sous l'identifiant | `packaging/flatpak/installer-flatpak.sh`, vérifié par la CI | ✅ CI |
| Permissions justifiées, sans joker | manifeste (commentaire par permission) | ✅ |
| Migration depuis l'ancien identifiant | `<replaces>` dans la fiche ; `packaging/flatpak/migrer-reglages.sh` | ✅ essayé sur Bazzite |

### Permissions à justifier dans la PR

- `--allow=bluetooth`, `--system-talk-name=org.bluez` : connexion RFCOMM au
  casque, par BlueZ.
- `--own-name=org.openheadset.Control` : nom de l'API D-Bus du service,
  utilisé aussi par le widget Plasma et les scripts (`ctl`), le même dans
  toutes les installations. `--own-name=org.openheadset.ControlWindow` :
  fenêtre unique.
- `--talk-name=org.kde.StatusNotifierWatcher` : icône de la zone de
  notification, inscrite sous le nom unique de la connexion.
- `--talk-name=org.kde.kglobalaccel`, `--talk-name=org.freedesktop.Notifications` :
  raccourcis globaux, notifications du service.
- `--filesystem=xdg-run/pipewire-0:ro` : diagnostic audio en lecture seule
  (`pw-dump`), sans capture.
- `--filesystem=xdg-documents` : exports (batterie, profil, réglages) et
  import des réglages.

## Préparer les fichiers

Depuis l'étiquette publiée :

```bash
git checkout v0.13.0
```
```bash
sh packaging/flatpak/flathub.sh v0.13.0 ../flathub-soumission
```

Le dossier contient le manifeste (source git : étiquette et révision),
`cargo-sources.json` et `flathub.json` (x86_64 seulement).

## Soumettre (avec l'accord du mainteneur seulement)

Procédure de Flathub (<https://docs.flathub.org/docs/for-app-authors/submission>) :
dupliquer `flathub/flathub` en gardant toutes les branches, partir de la
branche `new-pr`, y ajouter les trois fichiers, puis ouvrir la PR vers
`new-pr` avec le titre `Add io.github.asfax03l.openheadset-control`.

Texte de la PR, prêt à coller (Flathub demande l'anglais) :

```text
Add io.github.asfax03l.openheadset-control

OpenHeadset Control is an unofficial desktop controller for the Sennheiser
MOMENTUM 5 headphones (KDE client, background service and `ctl` CLI). I am
the upstream author (https://github.com/Asfax03L/openheadset-control).

Permissions:
- --allow=bluetooth, --system-talk-name=org.bluez: RFCOMM link to the
  headphones through BlueZ.
- --own-name=org.openheadset.Control: the service's D-Bus API name, shared
  with the Plasma widget and scripts across all installation methods;
  --own-name=org.openheadset.ControlWindow: single window instance.
- --talk-name=org.kde.StatusNotifierWatcher: tray icon, registered under the
  connection's unique name (no org.kde.* name owned).
- --talk-name=org.kde.kglobalaccel, --talk-name=org.freedesktop.Notifications:
  global shortcuts and notifications.
- --filesystem=xdg-run/pipewire-0:ro: read-only audio diagnostic (pw-dump),
  no capture.
- --filesystem=xdg-documents: exports and settings import.

The build is offline (cargo-sources.json generated from Cargo.lock) and
passes flatpak-builder-lint for the manifest; for the built repository, only
the screenshot-mirroring errors remain (mirroring is done by Flathub).
```

## Après l'acceptation

- Le dépôt `flathub/io.github.asfax03l.openheadset-control` reçoit chaque
  nouvelle version : étiquette, révision et `cargo-sources.json` mis à
  jour par `flathub.sh`.
- Vérification du domaine (badge « vérifié ») : par le compte GitHub
  `Asfax03L` sur le site de Flathub.
- La documentation passera de « installer le fichier `.flatpak` » à
  `flatpak install flathub io.github.asfax03l.openheadset-control`.
