# Installation

Installer OpenHeadset Control sous Windows ou Linux, depuis une publication ou depuis les sources.
Détails propres à Linux (Flatpak, paquets, service) : [LINUX.md](LINUX.md).


Prérequis : Windows 10/11 avec Bluetooth, et le casque **appairé et
connecté** au PC (Paramètres > Bluetooth et appareils).

**Sans Rust** : télécharger l'archive de la page Releases du dépôt
(publiée par la CI à chaque étiquette `v*`, avec son empreinte SHA-256,
une attestation de provenance et l'inventaire des dépendances :
[`docs/VERIFIER.md`](VERIFIER.md)) :
sous Windows, l'installateur
`openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe` (par
utilisateur, sans droits administrateur, désinstallation par Paramètres >
Applications) ; sinon l'archive
`openheadset-control-vX.Y.Z-windows-x86_64.zip` ou
`openheadset-control-vX.Y.Z-linux-x86_64.tar.gz`, à décompresser avant de
lancer le script qu'elle contient. Sous Linux, voir aussi le Flatpak, les
paquets `.deb` et `.rpm` et le `PKGBUILD` ([`docs/LINUX.md`](LINUX.md)).
Les exécutables ne sont pas signés : SmartScreen peut demander une
confirmation.

**Windows** (depuis les sources : Rust 1.95 ou plus récent, 1.89 pour la partie
Linux ; version testée figée dans `rust-toolchain.toml`) :

```bash
powershell -ExecutionPolicy Bypass -File scripts\installer.ps1
```

Le script installe, sans droits administrateur, dans
`%LOCALAPPDATA%\Programs\momentum-ctl-win` :
l'interface **WinUI 3** (`momentum-winui.exe`), le service
`momentum-daemon.exe` qu'elle lance au besoin, et `ctl.exe`. Il faut le
Windows App Runtime 2 (le script prévient s'il manque).

Il prend les exécutables posés à côté de lui (archive) ou, à défaut,
compile la version release. Il inscrit l'interface au démarrage de Windows
(elle démarre cachée : l'icône seule), retire l'ancienne icône Slint si
elle est installée, ajoute « OpenHeadset Control » au menu Démarrer et
lance l'interface. Tout est vérifié et préparé avant d'arrêter la version
en place (runtime, fichiers, copie) ; la version remplacée est gardée
(`momentum-ctl-win.ancien`) et remise si l'interface installée ne démarre
pas. Sans Windows App Runtime 2, rien n'est changé (`-SansRuntime` pour
installer quand même). Pour tout retirer :
`scripts\installer.ps1 -Desinstaller`.

**Linux (KDE Plasma)** : sur Bazzite et les systèmes en lecture seule, le
**Flatpak** de la page Releases (`flatpak install --user
openheadset-control-vX.Y.Z-x86_64.flatpak`, détails dans
[docs/LINUX.md](LINUX.md)) ; sinon, depuis l'archive,
`./installer-linux.sh` ; depuis
les sources (voir [docs/LINUX.md](LINUX.md) pour les paquets ; sur
Bazzite, dans une boîte distrobox) :

```bash
cargo build --release -p momentum-cli -p momentum-daemon -p momentum-kirigami
```
```bash
sh scripts/installer-linux.sh
```

Le script installe pour l'utilisateur courant, sans `sudo` : binaires dans
`~/.local/bin`, logo dans le thème d'icônes, entrées « OpenHeadset Control »
dans le menu des applications (KDE y trouve aussi l'icône des fenêtres sous
Wayland), service avec son unité systemd utilisateur et son activation
D-Bus. Le client Kirigami demande Qt 6 et Kirigami (présents sous KDE).
L'ancienne icône Slint (`momentum-tray`), si elle est installée, est retirée.
Un fichier manquant arrête le script avant tout changement ; les binaires
remplacés sont gardés (`*.ancien`) et remis si une copie échoue. Pour tout
retirer : `sh scripts/installer-linux.sh --desinstaller` (réglages, scènes
et historique gardés).

Pour un simple essai sans installation (Windows) :

```bash
cargo run --release --bin momentum-winui
```

L'icône d'un casque apparaît près de l'horloge (au besoin, dans les icônes
cachées, flèche **^**). Avec `--page son` (ou `scenes`, `batterie`, `audio`,
`reglages`, `apropos`), la fenêtre s'ouvre sur cette page ; avec `--cache`,
elle démarre cachée (l'icône seule). Le téléphone peut rester connecté au
casque en même temps : le M5 accepte le PC en multipoint.
