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
  effacement), connexion et audio, paramètres (dont thème clair ou sombre :
  jeu Breeze pour l'application seule, même si Plasma est dans l'autre), à
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

### Connexion et audio (PipeWire, lecture seule)

La page lit `pw-dump -N` sur un fil séparé (3 secondes, 16 Mio maximum),
à son ouverture, avec « Relire PipeWire » puis toutes les 15 secondes tant
qu'elle est visible. Elle rapproche les sorties BlueZ de l'empreinte du
casque **courant** donnée par le service : aucun choix par nom, aucune
adresse affichée ou enregistrée. Les profils A2DP, HFP/HSP et LE Audio,
le codec de la sortie et la métadonnée effective `default.audio.sink`
restent inconnus quand ils ne sont pas fournis. Les métadonnées de sortie
configurée ne sont pas prises pour la sortie effective.

Les constats signalent une sortie non choisie par défaut, un profil appel
(qualité réduite) ou une sortie du casque absente du graphe visible. Ils
ne changent ni profil, ni volume, ni routage. Le codec PipeWire décrit ce
PC ; le codec et la fréquence rapportés par le casque restent séparés,
car en Multipoint le flux peut venir de l'autre appareil. Un codec annoncé
ne prouve ni une lecture en cours ni une transmission sans perte.

Avec un service ancien qui ne fournit pas l'empreinte, le rapprochement
est indisponible et signalé ; il faut mettre à jour le service lors d'une
installation normale. Le client ne le redémarre pas. Si `pw-dump` manque
(paquet `pipewire-bin` sur Debian/Ubuntu, `pipewire-utils` sur Fedora), ou
si le socket/session est inaccessible, la page le dit et conserve les
informations de pilotage et de flux déjà connues. Le dump et stderr de
l'outil ne sont jamais journalisés. Le mode démonstration reste fictif et
ne lit pas PipeWire.

Références : [pw-dump](https://docs.pipewire.org/page_man_pw-dump_1.html),
[clés BlueZ](https://docs.pipewire.org/group__spa__keys.html),
[métadonnées](https://docs.pipewire.org/page_module_metadata.html).

## Installer avec Flatpak (recommandé sur Bazzite)

Bazzite est en lecture seule : le Flatpak s'installe pour l'utilisateur,
sans `sudo` ni boîte distrobox, et se met à jour comme les autres
applications. Télécharger `openheadset-control-vX.Y.Z-x86_64.flatpak` sur
la page Releases (la CI le construit à chaque publication), puis :

```bash
flatpak install --user openheadset-control-v0.13.0-x86_64.flatpak
```

Il contient le client KDE, le service (lancé à la demande par activation
D-Bus) et `ctl`. Identifiant depuis la v0.13.0 :
`io.github.asfax03l.openheadset-control` (avant : `org.openheadset.Control`,
voir « Migration des réglages » plus bas) :

```bash
flatpak run io.github.asfax03l.openheadset-control
```
```bash
flatpak run --command=ctl io.github.asfax03l.openheadset-control etat
```

Le bac à sable n'ouvre que ce qui sert : Wayland, sockets Bluetooth et
BlueZ (bus système), le nom D-Bus du service `org.openheadset.Control` (et
`org.openheadset.ControlWindow` pour la fenêtre unique), l'inscription de
l'icône de la zone de notification (sous le nom unique de la connexion :
aucun nom `org.kde.*` à posséder depuis la v0.13.0), raccourcis KDE,
notifications, le socket natif PipeWire `xdg-run/pipewire-0:ro` (lecture du
graphe) et le dossier Documents (exports, import des réglages). Pas de
réseau, ni à l'exécution ni à la construction. Le suffixe `:ro` limite le montage
du chemin, **pas les opérations du protocole du socket** ; notre client
n'utilise que `pw-dump`, sans capture audio ni commande de modification.
Ce chemin donne accès au graphe global ; le code ne conserve que les
propriétés nécessaires et n'expose aucun nom ou identifiant du graphe.
Il n'existe pas de permission `--socket=pipewire` à ajouter : le manifeste
utilise le chemin XDG précis, sans accès général au répertoire runtime.
La CI vérifie que `pw-dump --help` fonctionne avec le runtime du paquet
et que cette permission figure dans ses métadonnées. Cela ne prouve pas
la visibilité du graphe sur chaque poste : en cas de filtrage ou d'outil
absent, le diagnostic reste indisponible et l'interface l'indique.
Voir les [permissions Flatpak](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
et les [permissions des objets PipeWire](https://docs.pipewire.org/page_access.html).
Différences avec
l'installation par script : réglages et historique dans
`~/.var/app/io.github.asfax03l.openheadset-control/`, lancement à
l'ouverture de session par le portail du bureau, et raccourcis à modifier
dans Configuration du système > Raccourcis. Ne pas installer les deux à la
fois : un seul service peut porter le nom D-Bus `org.openheadset.Control`.

Activation D-Bus : Flatpak n'exporte un fichier d'activation que sous
l'identifiant de l'application. Le service y est déclaré sous
`io.github.asfax03l.openheadset-control` et prend aussi
`org.openheadset.Control` ; le client KDE du Flatpak le démarre par ce nom.
Un client hors du Flatpak (widget Plasma, `ctl` de l'archive) trouve le
service dès qu'il tourne, mais ne peut pas le démarrer : ouvrir
l'application une fois, ou `flatpak run --command=momentum-daemon
io.github.asfax03l.openheadset-control`.

### Migration des réglages

Le Flatpak ne voit pas les dossiers des autres installations : la reprise
se fait une fois, hors du bac à sable, par
`packaging/flatpak/migrer-reglages.sh`. Il copie préréglages, scènes,
raccourcis, panneau rapide, thème, historique de la batterie et profil
depuis l'ancien Flatpak (`~/.var/app/org.openheadset.Control/`), sinon
depuis l'installation par script ou par paquet (`~/.local/share`), vers le
nouveau Flatpak. Copie seulement : rien n'est supprimé, un fichier déjà
présent est gardé, et `--essai` montre d'abord ce qui serait copié.
`--depuis flatpak` ou `--depuis local` choisit la source.

À lancer **avant le premier lancement** du nouveau Flatpak (sinon il crée
ses fichiers par défaut, qui sont alors gardés) et application fermée :

```bash
curl -fsSLO https://raw.githubusercontent.com/Asfax03L/openheadset-control/v0.13.0/packaging/flatpak/migrer-reglages.sh
```
```bash
sh migrer-reglages.sh --essai
```
```bash
sh migrer-reglages.sh
```

Autre voie, sans script : exporter les réglages depuis l'ancienne
installation dans Documents, puis les importer dans le Flatpak (page
Paramètres, ou `ctl reglages exporter` puis `ctl reglages importer` ;
l'historique de la batterie seulement avec `--avec-batterie`).

### Construction locale

Outil `flatpak-builder`, environnement KDE 6.11 de Flathub. Les sources
Cargo sont d'abord figées (réseau et `python3` nécessaires), puis la
construction se fait sans réseau :

```bash
sh packaging/flatpak/sources-cargo.sh
```
```bash
flatpak-builder --user --install --force-clean build-flatpak packaging/flatpak/io.github.asfax03l.openheadset-control.yml
```

Préparation de la soumission à Flathub : [`FLATHUB.md`](FLATHUB.md).


## Installer les paquets Debian et Fedora

Les publications proposent aussi un paquet **x86_64** `openheadset-control`
contenant le client KDE, le service, `ctl`, son manuel et ses complétions,
le widget Plasma, les icônes et la fiche AppStream. Le `.deb` est construit
sur Debian 13 et testé sur Debian 13 et Ubuntu 26.04 ; le `.rpm` est construit
sur Fedora 43 et testé sur Fedora 43 et 44. Les dépendances Qt 6, Kirigami,
le style KDE, D-Bus et BlueZ sont installées par le gestionnaire de paquets.

**Sur Bazzite, utiliser le Flatpak ci-dessus, pas le `.rpm`.** Ces paquets
natifs sont destinés aux distributions classiques modifiables. Éviter de
cumuler paquet natif, Flatpak et installation utilisateur par script :
leurs services portent le même nom D-Bus. Si une ancienne installation
existe, utiliser sa méthode de désinstallation avant de passer au paquet ;
les données personnelles ne sont pas supprimées.

Télécharger le paquet et son fichier `.sha256` depuis la même publication,
puis vérifier l'empreinte (remplacer le nom par celui téléchargé) :

```sh
paquet=openheadset-control_0.13.0-1_amd64.deb
sha256sum -c "$paquet.sha256"
```

Debian 13 / Ubuntu 26.04 :

```sh
sudo apt install ./openheadset-control_0.13.0-1_amd64.deb
```

Fedora 43 et suivantes (vérifiées en CI : 43 et 44) :

```sh
sudo dnf install ./openheadset-control-0.13.0-1.x86_64.rpm
```

Les fichiers vont dans `/usr`. Aucun script du paquet ne démarre le
service, ne l'active à l'ouverture de session ou ne touche au dossier
personnel. Dans la session utilisateur, relire les unités puis lancer
« OpenHeadset Control » depuis le menu :

```sh
systemctl --user daemon-reload
momentum-kirigami
```

D-Bus lance le service à la première demande ; son exécutable est
`/usr/bin/momentum-daemon`. Le widget s'ajoute depuis Plasma. Les données
restent dans `~/.local/share/momentum-ctl-win`.

Désinstallation : `sudo apt purge openheadset-control` ou
`sudo dnf remove openheadset-control`. Les réglages, scènes et historiques
de l'utilisateur sont conservés. Le RPM possède seulement un script de retrait
qui enlève les répertoires vides du paquet sous `/usr` avec `rmdir` ; il
ne supprime aucun fichier ajouté localement et ne touche pas aux services.

### Construire les paquets sans les installer sur l'hôte

Depuis le dépôt, avec Podman (ou Docker en retirant les suffixes `:z`) :

```sh
podman build -t ohc-deb -f packaging/deb/Containerfile packaging/deb
podman run --rm -v "$PWD:/src:z" ohc-deb sh packaging/linux/construire.sh deb
podman build -t ohc-rpm -f packaging/rpm/Containerfile packaging/rpm
podman run --rm -v "$PWD:/src:z" ohc-rpm sh packaging/linux/construire.sh rpm
```

Utiliser une copie du dépôt par famille pour garder leurs dossiers
`target/` distincts. Les paquets et empreintes se trouvent dans
`target/paquets/deb/` et `target/paquets/rpm/`. Les versions de cargo-deb et
cargo-generate-rpm sont fixées dans les Containerfiles ; aucune nouvelle
dépendance Rust n'est ajoutée à l'application.

Le workflow « Paquets Linux natifs » construit les deux formats et vérifie,
dans des conteneurs propres, l'installation, les bibliothèques liées, le
chargement de Qt/QML en démonstration et la désinstallation de tous les
fichiers du paquet, en préservant un témoin de données fictives. À la
publication d'une étiquette, ces mêmes contrôles doivent réussir avant que
les deux paquets et leurs SHA-256 rejoignent les archives existantes.

## Installer par script

```bash
cargo build --release -p momentum-cli -p momentum-kirigami -p momentum-daemon
```
```bash
sh scripts/installer-linux.sh
```

Le script (sans `sudo`) vérifie d'abord que tous les fichiers sont là
(sinon il s'arrête sans rien changer), puis copie les binaires dans
`~/.local/bin` (les précédents sont gardés en `*.ancien` et remis si une
copie échoue), le logo dans `~/.local/share/icons/hicolor`, l'entrée de
menu dans `~/.local/share/applications`, le widget Plasma, l'unité systemd
utilisateur du service et son fichier d'activation D-Bus
(`~/.local/share/dbus-1/services`). Il retire l'ancienne icône Slint. Lancer
ensuite « OpenHeadset Control » depuis le menu, ou `momentum-kirigami`.
Désinstallation : `sh scripts/installer-linux.sh --desinstaller` (réglages,
scènes et historique gardés).

Essais sur le MOMENTUM 5 (micrologiciel 6.21.5), Bazzite 44 (KDE Plasma,
Wayland) :
- 20/10/2026 : `ctl etat` lit tout l'état par le profil client BlueZ, sans
  canal imposé ; la lecture audio continue pendant et après ;
- 20/10/2026 : première écriture sous Linux (anti-vent coupé puis remis par
  `ctl`, relu par le service) ;
- 21/10/2026 : mêmes essais depuis le Flatpak (installé pour l'essai) et
  par l'appel `Command` du widget Plasma ;
- 22/10/2026 : demandes numérotées (« appliqué et relu ») et casque
  retrouvé sans filtre de nom.

## Client GNOME (GTK 4)

Sous GNOME, `momentum-gtk` (libadwaita) est un client du même service :
page Accueil (batterie, mode de bruit, curseur ANC ↔ transparence,
anti-vent, audio spatial) en 0.12, les autres pages suivent
([`PARITE.md`](PARITE.md)). Installé par `installer-linux.sh` depuis
l'archive (entrée « OpenHeadset Control (GTK) ») ; demande GTK 4.14 et
libadwaita 1.5 ou plus récents. Depuis le dépôt :

```bash
cargo build --release -p momentum-gtk
```

Essai sans casque : `momentum-gtk --demo`.

## Widget Plasma

`plasma/org.openheadset.control.plasmoid` (Plasma 6, QML seul, module
`org.kde.plasma.workspace.dbus`) : client de l'API D-Bus du service, comme
le client Kirigami. Il lit `Snapshot` (toutes les 3 s quand il est ouvert,
30 s sinon, et après chaque événement) et envoie des `Command` JSON de
l'API ; `DBusServiceWatcher` suit l'arrivée du service, qu'il ne démarre
qu'à la demande. Essayé sur Bazzite 44 (Plasma 6.7) avec le M5 : lecture
de l'état, et anti-vent inversé puis remis par le même appel `Command`.

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
- Le service tourne dans une unité systemd durcie : il n'écrit que dans
  ce dossier et dans `~/Documents` (export du profil), sans accès réseau
  ni privilège ; détails dans [`SECURITE.md`](SECURITE.md).

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

### Demander une reconnexion Bluetooth

`ctl reconnecter`, l'Accueil déconnecté, le menu de l'icône et le widget
Plasma passent par le service. Seul le casque mémorisé dans `casque.txt`
est visé, parmi les objets déjà appairés de BlueZ. La demande
`Device1.Connect` connecte les profils auto-connectables ; elle ne règle
ni le profil ni la sortie PipeWire. Vérifier ensuite « Connexion et audio ».
Aucun nouvel appairage ni balayage radio n'est lancé. Le refus « aucun
casque mémorisé » invite à une première connexion manuelle dans les
paramètres Bluetooth. Dans le Flatpak, l'autorisation BlueZ existante du
service est réutilisée, sans nouvelle permission.
