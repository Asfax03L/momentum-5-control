#!/bin/sh
# À lancer dans le conteneur de construction correspondant, depuis le dépôt.
set -eu
case "${1:-}" in
    deb|rpm) family=$1 ;;
    *) echo "Famille attendue : deb ou rpm." >&2; exit 2 ;;
esac
cd "$(dirname "$0")/../.."
cargo build --release --locked -p momentum-cli -p momentum-daemon -p momentum-kirigami
integration=target/paquets/integration
mkdir -p "$integration/man" "$integration/completions" "target/paquets/$family"
# Les fichiers de l'installation utilisateur et du Flatpak restent inchangés.
sed 's|ExecStart=%h/.local/bin/momentum-daemon|ExecStart=/usr/bin/momentum-daemon|' \
    assets/linux/momentum-daemon.service > "$integration/momentum-daemon.service"
sed 's|Exec=/bin/false|Exec=/usr/bin/momentum-daemon|' \
    assets/linux/org.openheadset.Control.service > "$integration/org.openheadset.Control.service"
sed 's|io.github.asfax03l.openheadset-control.desktop|momentum-kirigami.desktop|' \
    assets/linux/io.github.asfax03l.openheadset-control.metainfo.xml > "$integration/io.github.asfax03l.openheadset-control.metainfo.xml"
target/release/ctl man > "$integration/man/ctl.1"
gzip -n -c "$integration/man/ctl.1" > "$integration/man/ctl.1.gz"
target/release/ctl completions bash > "$integration/completions/ctl.bash"
target/release/ctl completions zsh > "$integration/completions/_ctl"
target/release/ctl completions fish > "$integration/completions/ctl.fish"
if [ "$family" = deb ]; then
    cargo deb -p momentum-kirigami --no-build --no-strip --locked \
        --output target/paquets/deb/
else
    cargo generate-rpm -p frontends/kirigami -o target/paquets/rpm/
fi
# Échouer s'il n'y a aucun paquet et produire une empreinte par fichier.
for package in target/paquets/"$family"/*."$family"; do
    test -s "$package"
    sha256sum "$package" | cut -d' ' -f1 > "$package.sha256"
done
