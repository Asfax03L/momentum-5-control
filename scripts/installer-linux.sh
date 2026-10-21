#!/bin/sh
# Installe OpenHeadset Control pour l'utilisateur courant, sans sudo :
#   - binaires compilés (target/release) dans ~/.local/bin ;
#   - logo dans le thème d'icônes (~/.local/share/icons/hicolor) ;
#   - entrées de menu dans ~/.local/share/applications (KDE y trouve aussi
#     l'icône des fenêtres sous Wayland).
# Une version déjà installée est remplacée ; seule l'ancienne icône Slint
# (momentum-tray, son entrée de menu et son démarrage automatique) est retirée.
#
# Depuis le dépôt : binaires de target/release. Depuis une archive publiée :
# binaires et dossier assets posés à côté de ce script.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
if [ -x "$here/momentum-daemon" ] && [ -d "$here/assets" ]; then
    bins=$here
    assets=$here/assets
else
    repo=$(cd "$here/.." && pwd)
    bins=$repo/target/release
    assets=$repo/assets
fi
data=${XDG_DATA_HOME:-$HOME/.local/share}
bin=$HOME/.local/bin
mkdir -p "$bin" "$data/applications" "$data/icons/hicolor/scalable/apps"

for exe in momentum-daemon momentum-kirigami ctl; do
    if [ -x "$bins/$exe" ]; then
        install -m 755 "$bins/$exe" "$bin/$exe"
        echo "installé : $bin/$exe"
    fi
done
install -m 644 "$assets/logo/openheadset.svg" \
    "$data/icons/hicolor/scalable/apps/org.openheadset.control.svg"
# Tailles fixes : certains éléments de Plasma (barre des tâches, zone de
# notification) préfèrent une image à la bonne taille.
for size in 16 22 24 32 48 64 128 256; do
    mkdir -p "$data/icons/hicolor/${size}x${size}/apps"
    install -m 644 "$assets/logo/png/$size.png" \
        "$data/icons/hicolor/${size}x${size}/apps/org.openheadset.control.png"
done
install -m 644 "$assets/linux/momentum-kirigami.desktop" "$data/applications/momentum-kirigami.desktop"
echo "logo et entrée de menu installés"

# Ancienne icône Slint (versions 0.7 et avant) : remplacée par le client KDE.
config=${XDG_CONFIG_HOME:-$HOME/.config}
for old in "$bin/momentum-tray" "$data/applications/momentum-tray.desktop" \
    "$config/autostart/momentum-ctl-win.desktop"; do
    if [ -e "$old" ]; then
        rm -f "$old"
        echo "retiré (ancienne icône Slint) : $old"
    fi
done

# Widget Plasma (client de l'API D-Bus du service), à ajouter au panneau
# ou au bureau depuis « Ajouter des composants graphiques ».
# Dépôt ou archive : le dossier plasma est à côté de assets.
widget="$assets/../plasma/org.openheadset.control.plasmoid"
if command -v kpackagetool6 >/dev/null && [ -d "$widget" ]; then
    if kpackagetool6 --type Plasma/Applet --show org.openheadset.control.plasmoid >/dev/null 2>&1; then
        kpackagetool6 --type Plasma/Applet --upgrade "$widget" >/dev/null
    else
        kpackagetool6 --type Plasma/Applet --install "$widget" >/dev/null
    fi
    echo "widget Plasma installé (org.openheadset.control.plasmoid)"
fi

# Service : unité systemd utilisateur et activation D-Bus (le service
# démarre tout seul dès qu'une interface ou ctl l'appelle).
if [ -x "$bin/momentum-daemon" ]; then
    mkdir -p "$HOME/.config/systemd/user" "$data/dbus-1/services"
    install -m 644 "$assets/linux/momentum-daemon.service" \
        "$HOME/.config/systemd/user/momentum-daemon.service"
    install -m 644 "$assets/linux/org.openheadset.Control.service" \
        "$data/dbus-1/services/org.openheadset.Control.service"
    systemctl --user daemon-reload 2>/dev/null || true
    # Service déjà lancé : il reprend le nouveau binaire.
    systemctl --user try-restart momentum-daemon.service 2>/dev/null || true
    # Le bus relit ses fichiers d'activation.
    busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
        org.freedesktop.DBus ReloadConfig >/dev/null 2>&1 || true
    echo "service installé (activation D-Bus : org.openheadset.Control)"
fi

# Rafraîchit les caches (facultatif selon le bureau).
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
