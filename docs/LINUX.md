# OpenHeadset Control sous Linux (KDE Plasma, Bazzite)

Trois programmes, essayés sur le MOMENTUM 5 (Bazzite 44, KDE Plasma,
session **Wayland**) :

| Programme | Rôle | État |
|---|---|---|
| `ctl` | ligne de commande, connexion par BlueZ | ✅ lecture validée |
| `momentum-kirigami` | client natif KDE du service : Qt 6, Kirigami, style Breeze, icône de la zone de notification, raccourcis globaux | ✅ essayé sur M5 (v0.8.0) |
| `momentum-daemon` | service : seul propriétaire de la connexion, API D-Bus `org.openheadset.Control1` | ✅ essayé sur M5, lecture et écriture (v0.8.0) |

Le service tient la connexion ; `ctl` et le client Kirigami passent par
lui et peuvent tourner ensemble. Il démarre tout seul au premier appel
(activation D-Bus, unité systemd `momentum-daemon.service`). L'ancienne
icône Slint (`momentum-tray`, v0.7.0 et avant) est retirée par le script
d'installation.

Client Kirigami sous KDE Plasma :
- **icône de la zone de notification** (StatusNotifierItem) : logo teinté
  selon le mode, infobulle (nom, batterie, mode), clic pour ouvrir la
  fenêtre, menu avec le contrôle du bruit et « Quitter ». Fermer la
  fenêtre la cache seulement ;
- **raccourcis globaux** (KGlobalAccel) : Ctrl+Alt+N (mode de bruit
  suivant), Ctrl+Alt+P (profil personnalisé / égaliseur), Ctrl+Alt+M
  (ouvrir la fenêtre). À changer dans Configuration du système >
  Raccourcis > OpenHeadset Control ;
- **pages** : accueil avec les appareils appairés au casque (connecter,
  déconnecter après confirmation), égaliseur, scènes (application et
  éditeur), batterie (courbe enregistrée par le service, export,
  effacement), connexion et audio, paramètres (dont thème clair ou sombre,
  pas encore appliqué si le jeu de couleurs de Plasma est l'inverse), à
  propos avec « Copier le diagnostic » ;
- **mode démonstration** : `momentum-kirigami --demo` (casque simulé, sans
  service) ;
- **ouverture de session** (Paramètres > Application) : lance le client
  caché (`momentum-kirigami --cache`), l'icône seule apparaît ;
- **une seule fenêtre** : relancer le client montre la fenêtre déjà
  ouverte.

```bash
systemctl --user status momentum-daemon
```
```bash
busctl --user introspect org.openheadset.Control /org/openheadset/Control
```

**Wayland** : le client Kirigami est un client Wayland natif (aucune connexion X11/XWayland, vérifié sur les sockets du
processus). Sous Wayland, KDE trouve l'icône d'une fenêtre dans le fichier
`.desktop` du même nom : d'où les entrées installées par le script.

Pas encore sous Linux : panneau rapide, lecture des sorties audio
(PipeWire).

## Installer avec Flatpak (recommandé sur Bazzite)

Bazzite est en lecture seule : le Flatpak s'installe pour l'utilisateur,
sans `sudo` ni boîte distrobox, et se met à jour comme les autres
applications. Télécharger `openheadset-control-vX.Y.Z-x86_64.flatpak` sur
la page Releases (la CI le construit à chaque publication), puis :

```bash
flatpak install --user openheadset-control-v0.8.0-x86_64.flatpak
```

Il contient le client KDE, le service (lancé à la demande par activation
D-Bus) et `ctl` :

```bash
flatpak run org.openheadset.Control
```
```bash
flatpak run --command=ctl org.openheadset.Control etat
```

Le bac à sable n'ouvre que ce qui sert : Wayland, sockets Bluetooth et
BlueZ (bus système), icône de la zone de notification, raccourcis KDE,
notifications, et le dossier Documents (exports). Différences avec
l'installation par script : réglages et historique dans
`~/.var/app/org.openheadset.Control/` (pas repris de `~/.local/share`),
lancement à l'ouverture de session par le portail du bureau, et raccourcis
à modifier dans Configuration du système > Raccourcis. Ne pas installer les
deux à la fois : un seul service peut porter le nom D-Bus
`org.openheadset.Control`.

Construction locale (outil `flatpak-builder`, environnement KDE 6.11 de
Flathub) :

```bash
flatpak-builder --user --install --force-clean build-flatpak packaging/flatpak/org.openheadset.Control.yml
```

## Installer par script

```bash
cargo build --release -p momentum-cli -p momentum-kirigami -p momentum-daemon
```
```bash
sh scripts/installer-linux.sh
```

Le script (sans `sudo` ; seule l'ancienne icône Slint est retirée) copie
les binaires dans
`~/.local/bin`, le logo dans `~/.local/share/icons/hicolor`, les entrées de
menu dans `~/.local/share/applications`, l'unité systemd utilisateur du
service et son fichier d'activation D-Bus
(`~/.local/share/dbus-1/services`). Lancer ensuite
« OpenHeadset Control » depuis le menu, ou `momentum-kirigami`.

État : **lecture validée** le 20/10/2026 sur Bazzite 44 (KDE Plasma) :
`ctl etat` lit tout l'état du MOMENTUM 5 par le profil client BlueZ, sans
canal imposé, et la lecture audio continue pendant et après. Aucun
réglage n'a encore été écrit sous Linux.

## Compiler sur Bazzite

Bazzite est en lecture seule : on compile dans une boîte distrobox Fedora
(`momentum-dev`, sans effet sur le système), avec Rust (`rustup`) et :

- service et `ctl` : `gcc gcc-c++ pkgconf-pkg-config dbus-devel` ;
- client Kirigami : `qt6-qtbase-devel qt6-qtdeclarative-devel
  kf6-kirigami-devel kf6-qqc2-desktop-style`.

Les versions de Fedora 44 sont celles de Bazzite 44 (Qt 6.11, Kirigami
6.31) : un binaire compilé dans la boîte tourne sur le bureau.

## Comment ça marche

- BlueZ ne donne pas le nom SDP des services, seulement leurs UUID. `ctl`
  cherche donc un appareil **appairé et connecté**, dont le nom contient
  « momentum » ou « sennheiser » (ou le filtre `--device`), et qui annonce
  l'UUID GAIA relevé sur le MOMENTUM 5 :
  `A2129FF3-081B-4C45-8AFE-469D9C4842EC`.
- Par défaut, `ctl` enregistre un profil client BlueZ et demande la
  connexion (`ConnectProfile`). BlueZ fait la recherche SDP et choisit
  l'un des deux enregistrements GAIA (canaux 11 et 12).
- Avec `--canal 12` (ou 11), `ctl` ouvre directement ce canal RFCOMM, sans
  passer par SDP.
- Données (scènes, préréglages) : `~/.local/share/momentum-ctl-win`.

## Dual boot Windows + Bazzite : la clé d'appairage

Windows et Linux utilisent le même adaptateur Bluetooth, donc la même
adresse, mais chacun garde sa propre clé d'appairage. Le casque ne retient
qu'une clé pour cette adresse : celle du dernier système qui l'a appairé.
Appairer sous Bazzite peut donc faire échouer la connexion sous Windows,
et inversement.

Solutions, au choix :
1. **Réappairer** à chaque changement de système. C'est le plus simple pour
   un premier essai.
2. **Recopier la clé Windows dans BlueZ**, pour que les deux systèmes
   partagent la même. Il faut lire le registre Windows depuis Linux, par
   exemple avec l'outil `bt-dualboot`, puis éditer `/var/lib/bluetooth`
   avec `sudo`. C'est à faire soi-même, et seulement si l'étape 1
   fonctionne.

## Essai de l'étape 1 (lecture seule)

1. **Binaire** : récupérer `ctl` (artefact `ctl-linux-x86_64` de la CI),
   le copier dans `~/.local/bin/`, puis :
   ```bash
   chmod +x ~/.local/bin/ctl
   ```
2. **Casque** : l'appairer et le connecter dans *Paramètres système →
   Bluetooth* de KDE. Lancer un peu de musique pour vérifier qu'il est bien
   connecté.
3. **Vérifier que BlueZ voit le service GAIA** (lecture seule) :
   ```bash
   bluetoothctl devices Paired
   ```
   puis, avec l'adresse du casque :
   ```bash
   bluetoothctl info XX:XX:XX:XX:XX:XX
   ```
   La liste `UUID:` doit contenir `a2129ff3-081b-4c45-8afe-469d9c4842ec`.
   Ne partager ni l'adresse ni le numéro de série.
4. **Lire l'état** :
   ```bash
   ctl etat
   ```
   En cas d'échec, relancer avec le journal détaillé, puis sur un canal
   imposé :
   ```bash
   ctl -v etat
   ```
   ```bash
   ctl -v etat --canal 12
   ```
5. **Vérifier que la musique continue** pendant et après `ctl etat`.

À relever : la sortie de chaque commande, et si le son a été coupé. Ne
lancer **aucune commande d'écriture** (bruit, égaliseur, scène…) avant que
la lecture fonctionne et que l'essai suivant ait été convenu.

## Limites connues

- Pas d'icône ni de fenêtre (étapes 2 et 3).
- Choix de l'enregistrement GAIA par BlueZ : non vérifié. Sous Windows,
  c'est le canal 12 qui a toujours fonctionné.
- Profils BlueZ : si un autre programme a déjà enregistré un profil pour
  cet UUID, l'enregistrement échoue. Le message d'erreur de BlueZ est
  affiché tel quel.
