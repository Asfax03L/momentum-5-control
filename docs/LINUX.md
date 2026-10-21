# OpenHeadset Control sous Linux (KDE Plasma, Bazzite)

Trois programmes, essayés sur le MOMENTUM 5 le 20/10/2026 (Bazzite 44, KDE
Plasma, session **Wayland**) :

| Programme | Rôle | État |
|---|---|---|
| `ctl` | ligne de commande, connexion par BlueZ | ✅ lecture validée |
| `momentum-tray` | icône de la zone de notification + fenêtre Slint (OpenHeadset Control) | ✅ validé (v0.7.0) |
| `momentum-kirigami` | client natif KDE du service : Qt 6, Kirigami, style Breeze, icône de la zone de notification, raccourcis globaux | 🟡 essayé, non publié |
| `momentum-daemon` | service : seul propriétaire de la connexion, API D-Bus `org.openheadset.Control1` | 🟡 essayé (lecture et écriture), non publié |

Le service tient la connexion ; `ctl` et le client Kirigami passent par
lui et peuvent tourner ensemble. Il démarre tout seul au premier appel
(activation D-Bus, unité systemd `momentum-daemon.service`). L'icône Slint
embarque encore sa propre connexion jusqu'à l'étape 4 de
[ARCHITECTURE.md](ARCHITECTURE.md) : ne pas la lancer en même temps que le
service.

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
  déconnecter après confirmation), égaliseur, scènes (`scenes.txt`,
  modifiable depuis la page), batterie (courbe enregistrée par le
  service), paramètres, à propos avec « Copier le diagnostic » ;
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

**Wayland** : l'icône Slint et le client Kirigami sont des clients Wayland
natifs (aucune connexion X11/XWayland, vérifié sur les sockets du
processus). Sous Wayland, KDE trouve l'icône d'une fenêtre dans le fichier
`.desktop` du même nom : d'où les entrées installées par le script.

Pas encore sous Linux : panneau rapide, lecture des sorties audio
(PipeWire). Le client Kirigami applique les scènes mais ne les édite pas
encore (fichier `scenes.txt` ou fenêtre Slint).

## Installer

```bash
cargo build --release -p momentum-tray -p momentum-cli -p momentum-kirigami -p momentum-daemon
```
```bash
sh scripts/installer-linux.sh
```

Le script (sans `sudo`, rien n'est supprimé) copie les binaires dans
`~/.local/bin`, le logo dans `~/.local/share/icons/hicolor`, les entrées de
menu dans `~/.local/share/applications`, l'unité systemd utilisateur du
service et son fichier d'activation D-Bus
(`~/.local/share/dbus-1/services`). Lancer ensuite
« OpenHeadset Control » depuis le menu, ou `momentum-tray --fenetre`.

État : **lecture validée** le 20/10/2026 sur Bazzite 44 (KDE Plasma) :
`ctl etat` lit tout l'état du MOMENTUM 5 par le profil client BlueZ, sans
canal imposé, et la lecture audio continue pendant et après. Aucun
réglage n'a encore été écrit sous Linux.

## Compiler sur Bazzite

Bazzite est en lecture seule : on compile dans une boîte distrobox Fedora
(`momentum-dev`, sans effet sur le système), avec Rust (`rustup`) et :

- icône Slint et `ctl` : `gcc gcc-c++ pkgconf-pkg-config dbus-devel
  libxkbcommon-devel wayland-devel fontconfig-devel freetype-devel
  gtk3-devel libxdo-devel` ;
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
