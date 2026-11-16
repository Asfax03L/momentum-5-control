# Installation

*[English version](INSTALLATION.en.md)*

Installer OpenHeadset Control sous Windows ou Linux, depuis une publication ou
depuis les sources. Détails propres à Linux (Flatpak, paquets, service) :
[LINUX.md](LINUX.md).

## Avant de commencer

- Un PC avec Bluetooth, sous Windows 10/11 ou Linux.
- Le casque **appairé et connecté** au PC, dans les paramètres Bluetooth du
  système. OpenHeadset Control n'appaire rien lui-même.
- Le téléphone peut rester connecté au casque en même temps : le MOMENTUM 5
  accepte le PC en Multipoint.

## Depuis une publication (sans Rust)

Téléchargez les fichiers sur la page
[Releases](https://github.com/Asfax03L/momentum-5-control/releases/latest).
La CI les publie à chaque étiquette `v*`, avec une empreinte SHA-256, une
attestation de provenance et l'inventaire des dépendances
([vérifier un fichier](VERIFIER.md)). Les exécutables ne sont pas signés :
SmartScreen peut demander une confirmation.

| Système | Fichier | Comment |
|---|---|---|
| Windows 10/11 | `openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe` | Le lancer. Installation par utilisateur, sans droits administrateur. Désinstallation par Paramètres > Applications |
| Windows, sans l'installateur | `openheadset-control-vX.Y.Z-windows-x86_64.zip` | Décompresser, puis lancer `installer.ps1` |
| Bazzite et autres systèmes en lecture seule | `openheadset-control-vX.Y.Z-x86_64.flatpak` | `flatpak install --user openheadset-control-vX.Y.Z-x86_64.flatpak` |
| Debian, Ubuntu | paquet `.deb` | L'installer avec le gestionnaire de paquets |
| Fedora | paquet `.rpm` | L'installer avec le gestionnaire de paquets |
| Autres Linux | `openheadset-control-vX.Y.Z-linux-x86_64.tar.gz` | Décompresser, puis lancer `./installer-linux.sh` |

L'interface Windows demande le Windows App Runtime 2. L'installateur le
vérifie. Sous Linux, voir aussi le `PKGBUILD` et les détails de chaque paquet
dans [LINUX.md](LINUX.md).

## Windows, depuis les sources

Il faut Rust 1.95 ou plus récent (la version essayée est figée dans
`rust-toolchain.toml`).

```bash
powershell -ExecutionPolicy Bypass -File scripts\installer.ps1
```

Le script installe, sans droits administrateur, dans
`%LOCALAPPDATA%\Programs\momentum-ctl-win` : l'interface **WinUI 3**
(`momentum-winui.exe`), le service `momentum-daemon.exe` qu'elle lance au
besoin, et `ctl.exe`.

Ce que fait le script :

- Il prend les exécutables posés à côté de lui (archive) ou, à défaut, compile
  la version release.
- Il inscrit l'interface au démarrage de Windows (elle démarre cachée : l'icône
  seule), ajoute « OpenHeadset Control » au menu Démarrer et lance l'interface.
  L'ancienne icône Slint est retirée si elle est installée.
- Il vérifie et prépare tout avant d'arrêter la version en place. La version
  remplacée est gardée (`momentum-ctl-win.ancien`) et remise si l'interface
  installée ne démarre pas.
- Sans Windows App Runtime 2, rien n'est changé (`-SansRuntime` pour installer
  quand même).

Pour tout retirer : `scripts\installer.ps1 -Desinstaller`.

Pour un simple essai sans installation :

```bash
cargo run --release --bin momentum-winui
```

L'icône d'un casque apparaît près de l'horloge (au besoin, dans les icônes
cachées, flèche **^**). Avec `--page son` (ou `scenes`, `batterie`, `audio`,
`reglages`, `apropos`), la fenêtre s'ouvre sur cette page. Avec `--cache`, elle
démarre cachée. Avec `--demo`, elle montre un casque simulé et n'envoie rien.

## Linux, depuis les sources

Il faut Rust 1.89 ou plus récent (1.92 pour le client GNOME), `dbus` et
`pkg-config` ; Qt 6 et Kirigami pour le client KDE ; GTK 4.14 et libadwaita 1.5
pour le client GNOME. Sur Bazzite, compiler dans une boîte distrobox. Les
listes de paquets sont dans [LINUX.md](LINUX.md).

```bash
cargo build --release -p momentum-cli -p momentum-daemon -p momentum-kirigami
```
```bash
sh scripts/installer-linux.sh
```

Le script installe pour l'utilisateur courant, sans `sudo` : binaires dans
`~/.local/bin`, logo dans le thème d'icônes, entrées « OpenHeadset Control »
dans le menu des applications (KDE y trouve aussi l'icône des fenêtres sous
Wayland), service avec son unité systemd utilisateur et son activation D-Bus.
L'ancienne icône Slint (`momentum-tray`) est retirée si elle est installée. Un
fichier manquant arrête le script avant tout changement. Les binaires remplacés
sont gardés (`*.ancien`) et remis si une copie échoue.

Pour tout retirer : `sh scripts/installer-linux.sh --desinstaller`. Les
réglages, les scènes et l'historique sont gardés.

Pour le client GNOME, compiler aussi `-p momentum-gtk` avant de lancer le
script.
