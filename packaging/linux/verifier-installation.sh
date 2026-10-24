#!/bin/sh
# Conteneur propre et jetable uniquement : aucune session ni socket de l'hôte.
set -eu
if [ ! -f /.dockerenv ] && [ ! -f /run/.containerenv ]; then
    echo "Cet essai doit être lancé dans un conteneur jetable." >&2
    exit 2
fi
test "$(id -u)" -eq 0
family=${1:-}
package=${2:-}
# Facultatif : paquet de la version précédente, installé d'abord pour
# essayer la mise à jour par-dessus.
previous=${3:-}
test -f "$package"
test "$(sha256sum "$package" | cut -d' ' -f1)" = "$(cat "$package.sha256")"
test_root=$(mktemp -d)
trap 'rm -rf "$test_root"' EXIT
export HOME="$test_root/home"
export XDG_DATA_HOME="$HOME/.local/share"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_RUNTIME_DIR="$test_root/runtime"
mkdir -p "$XDG_DATA_HOME/momentum-ctl-win" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
echo "Données fictives à préserver" > "$XDG_DATA_HOME/momentum-ctl-win/temoin.txt"
sha256sum "$XDG_DATA_HOME/momentum-ctl-win/temoin.txt" > "$test_root/temoin.sha256"
case "$family" in
    deb)
        # Interdire les scripts du paquet, notamment le démarrage d'un service.
        dpkg-deb -e "$package" "$test_root/controle"
        for hook in preinst postinst prerm postrm; do
            test ! -e "$test_root/controle/$hook"
        done
        # L'image Ubuntu minimale exclut les manuels et la documentation.
        # Les conserver dans ce conteneur pour vérifier aussi ces fichiers.
        printf '%s\n' 'path-include=/usr/share/man/*' \
            'path-include=/usr/share/doc/openheadset-control/*' \
            > /etc/dpkg/dpkg.cfg.d/zz-openheadset-controle
        apt-get update -qq
        if [ -n "$previous" ]; then
            DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$previous"
            echo "version précédente : $(/usr/bin/ctl --version)"
        fi
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
            "$package" binutils
        dpkg-query -L openheadset-control > "$test_root/contenu"
        ;;
    rpm)
        # Seul le retrait des répertoires vides est autorisé, à contenu vérifié.
        for hook in PREIN POSTIN PREUN PRETRANS POSTTRANS; do
            test "$(rpm -qp --qf "%{$hook}" "$package")" = "(none)"
        done
        test "$(rpm -qp --qf '%{POSTUN}' "$package")" = "$(cat packaging/rpm/apres-retrait.sh)"
        if [ -n "$previous" ]; then
            dnf install -y "$previous"
            echo "version précédente : $(/usr/bin/ctl --version)"
        fi
        dnf install -y "$package" binutils
        rpm -ql openheadset-control > "$test_root/contenu"
        ;;
    pacman)
        # Aucun script d'installation (.INSTALL) dans le paquet.
        ! bsdtar -tf "$package" | grep -qx '.INSTALL'
        pacman -U --noconfirm "$package"
        # Réinstallation par-dessus (pas encore de version précédente sur
        # l'AUR).
        pacman -U --noconfirm "$package"
        pacman -Qlq openheadset-control > "$test_root/contenu"
        ;;
    *) echo "Famille attendue : deb, rpm ou pacman." >&2; exit 2 ;;
esac
sha256sum -c "$test_root/temoin.sha256"
# L'installation n'a ajouté aucun fichier dans le dossier personnel.
test "$(find "$HOME" -type f | wc -l)" -eq 1
while IFS= read -r path; do
    if [ -f "$path" ] || [ -L "$path" ]; then
        echo "$path"
    fi
done < "$test_root/contenu" > "$test_root/fichiers"
for executable in ctl momentum-daemon momentum-kirigami; do
    test -x "/usr/bin/$executable"
    ldd "/usr/bin/$executable" > "$test_root/liaison"
    if grep -q 'not found' "$test_root/liaison"; then cat "$test_root/liaison"; exit 1; fi
done
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
test "$(/usr/bin/ctl --version)" = "ctl $version"
/usr/bin/ctl --help > "$test_root/aide"
/usr/bin/ctl man > "$test_root/manuel"
grep -q etat "$test_root/aide"
grep -q scene "$test_root/manuel"
test -s /usr/share/man/man1/ctl.1.gz
test -s /usr/share/bash-completion/completions/ctl
test -s /usr/share/zsh/site-functions/_ctl
test -s /usr/share/fish/vendor_completions.d/ctl.fish
test -s /usr/share/doc/openheadset-control/LICENSE-MIT
test -s /usr/share/doc/openheadset-control/LICENSE-APACHE
test -s /usr/share/applications/momentum-kirigami.desktop
test -s /usr/share/icons/hicolor/scalable/apps/org.openheadset.control.svg
for size in 16 22 24 32 48 64 128 256; do
    test -s "/usr/share/icons/hicolor/${size}x${size}/apps/org.openheadset.control.png"
done
grep -qx 'ExecStart=/usr/bin/momentum-daemon' /usr/lib/systemd/user/momentum-daemon.service
grep -qx 'Exec=/usr/bin/momentum-daemon' /usr/share/dbus-1/services/org.openheadset.Control.service
test -s /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid/metadata.json
test -s /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid/contents/ui/main.qml
grep -q 'momentum-kirigami.desktop' /usr/share/metainfo/io.github.asfax03l.openheadset-control.metainfo.xml
# Charger réellement QML et ses modules, sans casque ni écran, en démonstration.
result=0
QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_QUICK_CONTROLS_STYLE=org.kde.desktop \
    timeout 8s dbus-run-session -- /usr/bin/momentum-kirigami --demo \
    > "$test_root/interface.log" 2>&1 || result=$?
cat "$test_root/interface.log"
test "$result" -eq 124
if grep -Ei 'failed to load component|module .*not installed|Type .* unavailable|Cannot load library|error while loading shared libraries' "$test_root/interface.log"; then
    exit 1
fi
case "$family" in
    deb) DEBIAN_FRONTEND=noninteractive apt-get purge -y openheadset-control ;;
    rpm) dnf remove -y openheadset-control ;;
    pacman) pacman -Rns --noconfirm openheadset-control ;;
esac
while IFS= read -r path; do
    test ! -e "$path"
    test ! -L "$path"
done < "$test_root/fichiers"
# Les répertoires propres au paquet sont retirés eux aussi.
test ! -e /usr/share/doc/openheadset-control
test ! -e /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid
sha256sum -c "$test_root/temoin.sha256"
echo "Installation, chargement Qt/QML et désinstallation : réussite."
