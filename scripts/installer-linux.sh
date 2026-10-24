#!/bin/sh
# Installe OpenHeadset Control pour l'utilisateur courant, sans sudo :
#   - binaires compilés (target/release) dans ~/.local/bin ;
#   - logo dans le thème d'icônes (~/.local/share/icons/hicolor) ;
#   - entrée de menu dans ~/.local/share/applications (KDE y trouve aussi
#     l'icône des fenêtres sous Wayland) ;
#   - widget Plasma, service (unité systemd utilisateur, activation D-Bus).
#   - manuel ctl(1) et complétions bash, zsh et fish.
#
# Tout est vérifié avant de toucher à la version installée (F08 de la
# revue) : un fichier manquant arrête le script sans rien changer. Les
# binaires remplacés sont gardés à côté (*.ancien) et remis si l'une des
# copies échoue.
#
# Désinstallation : sh installer-linux.sh --desinstaller (réglages, scènes
# et historique restent dans ~/.local/share/momentum-ctl-win).
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
config=${XDG_CONFIG_HOME:-$HOME/.config}
bin=$HOME/.local/bin
exes="momentum-daemon momentum-kirigami ctl"
widget_id=org.openheadset.control.plasmoid
# Dépôt ou archive : le dossier plasma est à côté de assets.
widget="$assets/../plasma/$widget_id"

refresh_caches() {
    command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" || true
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
    command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
}

if [ "${1:-}" = "--desinstaller" ]; then
    systemctl --user stop momentum-daemon.service 2>/dev/null || true
    for f in "$bin/momentum-daemon" "$bin/momentum-kirigami" "$bin/ctl" \
        "$bin/momentum-gtk" "$data/applications/momentum-gtk.desktop" \
        "$bin/momentum-daemon.ancien" "$bin/momentum-kirigami.ancien" "$bin/ctl.ancien" \
        "$data/applications/momentum-kirigami.desktop" \
        "$config/autostart/momentum-kirigami.desktop" \
        "$config/systemd/user/momentum-daemon.service" \
        "$data/dbus-1/services/org.openheadset.Control.service" \
        "$data/man/man1/ctl.1" "$data/bash-completion/completions/ctl" \
        "$data/zsh/site-functions/_ctl" "$data/fish/vendor_completions.d/ctl.fish"; do
        if [ -e "$f" ]; then
            rm -f "$f"
            echo "retiré : $f"
        fi
    done
    for f in "$data"/icons/hicolor/*/apps/org.openheadset.control.*; do
        [ -e "$f" ] && rm -f "$f"
    done
    if command -v kpackagetool6 >/dev/null &&
        kpackagetool6 --type Plasma/Applet --show "$widget_id" >/dev/null 2>&1; then
        kpackagetool6 --type Plasma/Applet --remove "$widget_id" >/dev/null
        echo "widget Plasma retiré"
    fi
    systemctl --user daemon-reload 2>/dev/null || true
    refresh_caches
    echo "OpenHeadset Control désinstallé (réglages, scènes et historique gardés dans $data/momentum-ctl-win)."
    exit 0
fi

# 1. Vérification : tout ce qui sera installé doit être là.
missing=""
for exe in $exes; do
    [ -x "$bins/$exe" ] || missing="$missing $bins/$exe"
done
for f in "$assets/logo/openheadset.svg" "$assets/linux/momentum-kirigami.desktop" \
    "$assets/linux/momentum-daemon.service" "$assets/linux/org.openheadset.Control.service"; do
    [ -f "$f" ] || missing="$missing $f"
done
for size in 16 22 24 32 48 64 128 256; do
    [ -f "$assets/logo/png/$size.png" ] || missing="$missing $assets/logo/png/$size.png"
done
if [ -n "$missing" ]; then
    echo "Installation annulée, rien n'a été changé. Fichiers manquants :$missing" >&2
    echo "(Depuis le dépôt : cargo build --release -p momentum-cli -p momentum-daemon -p momentum-kirigami.)" >&2
    exit 1
fi

# Depuis le dépôt, générer avec le binaire qui sera installé. Depuis une
# archive, utiliser les fichiers publiés. Tout est prêt avant les copies.
documentation=$bins
if [ "$bins" != "$here" ]; then
    documentation=$(mktemp -d)
    trap 'rm -rf "$documentation"' EXIT
    trap 'exit 1' HUP INT TERM
    mkdir -p "$documentation/man" "$documentation/completions"
    "$bins/ctl" man > "$documentation/man/ctl.1"
    "$bins/ctl" completions bash > "$documentation/completions/ctl.bash"
    "$bins/ctl" completions zsh > "$documentation/completions/_ctl"
    "$bins/ctl" completions fish > "$documentation/completions/ctl.fish"
fi
for f in man/ctl.1 completions/ctl.bash completions/_ctl completions/ctl.fish; do
    if [ ! -s "$documentation/$f" ]; then
        echo "Installation annulée, rien n'a été changé. Fichier manquant ou vide : $documentation/$f" >&2
        exit 1
    fi
done

# 2. Binaires : copie à côté, puis remplacement ; en cas d'échec, les
# anciens binaires sont remis.
mkdir -p "$bin"
# Seule la version précédente est gardée comme sauvegarde.
for exe in $exes; do
    rm -f "$bin/$exe.ancien"
done
restore() {
    for exe in $exes; do
        rm -f "$bin/.$exe.nouveau"
        if [ -e "$bin/$exe.ancien" ]; then
            mv -f "$bin/$exe.ancien" "$bin/$exe"
        fi
    done
    echo "Échec : ancienne version remise." >&2
}
for exe in $exes; do
    install -m 755 "$bins/$exe" "$bin/.$exe.nouveau" || { restore; exit 1; }
done
for exe in $exes; do
    if [ -e "$bin/$exe" ]; then
        cp -p "$bin/$exe" "$bin/$exe.ancien" || { restore; exit 1; }
    fi
done
for exe in $exes; do
    mv -f "$bin/.$exe.nouveau" "$bin/$exe" || { restore; exit 1; }
    echo "installé : $bin/$exe"
done

# Manuel et complétions : uniquement dans les dossiers de l'utilisateur.
mkdir -p "$data/man/man1" "$data/bash-completion/completions" \
    "$data/zsh/site-functions" "$data/fish/vendor_completions.d"
install -m 644 "$documentation/man/ctl.1" "$data/man/man1/ctl.1"
install -m 644 "$documentation/completions/ctl.bash" "$data/bash-completion/completions/ctl"
install -m 644 "$documentation/completions/_ctl" "$data/zsh/site-functions/_ctl"
install -m 644 "$documentation/completions/ctl.fish" "$data/fish/vendor_completions.d/ctl.fish"
echo "manuel ctl(1) et complétions bash, zsh et fish installés"

# 3. Logo, entrée de menu, widget.
mkdir -p "$data/applications" "$data/icons/hicolor/scalable/apps"
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

# Client GNOME (GTK 4, libadwaita), facultatif : installé s'il est fourni
# (archive publiée, ou cargo build --release -p momentum-gtk).
if [ -x "$bins/momentum-gtk" ] && [ -f "$assets/linux/momentum-gtk.desktop" ]; then
    install -m 755 "$bins/momentum-gtk" "$bin/momentum-gtk"
    install -m 644 "$assets/linux/momentum-gtk.desktop" "$data/applications/momentum-gtk.desktop"
    echo "client GNOME installé : $bin/momentum-gtk"
fi

# Ancienne icône Slint (versions 0.7 et avant) : remplacée par le client KDE.
for old in "$bin/momentum-tray" "$data/applications/momentum-tray.desktop" \
    "$config/autostart/momentum-ctl-win.desktop"; do
    if [ -e "$old" ]; then
        rm -f "$old"
        echo "retiré (ancienne icône Slint) : $old"
    fi
done

# Widget Plasma (client de l'API D-Bus du service), à ajouter au panneau
# ou au bureau depuis « Ajouter des composants graphiques ».
if command -v kpackagetool6 >/dev/null && [ -d "$widget" ]; then
    if kpackagetool6 --type Plasma/Applet --show "$widget_id" >/dev/null 2>&1; then
        kpackagetool6 --type Plasma/Applet --upgrade "$widget" >/dev/null
    else
        kpackagetool6 --type Plasma/Applet --install "$widget" >/dev/null
    fi
    echo "widget Plasma installé ($widget_id)"
fi

# 4. Service : unité systemd utilisateur et activation D-Bus (le service
# démarre tout seul dès qu'une interface ou ctl l'appelle).
mkdir -p "$config/systemd/user" "$data/dbus-1/services"
install -m 644 "$assets/linux/momentum-daemon.service" \
    "$config/systemd/user/momentum-daemon.service"
install -m 644 "$assets/linux/org.openheadset.Control.service" \
    "$data/dbus-1/services/org.openheadset.Control.service"
systemctl --user daemon-reload 2>/dev/null || true
# Service déjà lancé : il reprend le nouveau binaire.
systemctl --user try-restart momentum-daemon.service 2>/dev/null || true
# Le bus relit ses fichiers d'activation.
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus ReloadConfig >/dev/null 2>&1 || true
echo "service installé (activation D-Bus : org.openheadset.Control)"

refresh_caches
echo "Installation terminée (ancienne version gardée en $bin/*.ancien)."
