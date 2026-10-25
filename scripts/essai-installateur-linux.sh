#!/bin/sh
# Essai de installer-linux.sh dans un faux dossier personnel (A08 de
# l'audit du 24/10/2026), binaires de target/release :
#   1. première installation (aucune version à restaurer) ;
#   2. panne injectée après le remplacement des binaires : sortie en échec
#      et « Installation partielle » annoncée, jamais « terminée » ;
#   3. nouvelle installation une fois la cause levée : réussite ;
#   4. désinstallation : binaires et intégration retirés, données gardées.
# Ne touche jamais au vrai dossier personnel.
set -eu
depot=$(cd "$(dirname "$0")/.." && pwd)
faux=$(mktemp -d)
export HOME="$faux"
unset XDG_DATA_HOME XDG_CONFIG_HOME
installer() { sh "$depot/scripts/installer-linux.sh" "$@" > "$faux/sortie.txt" 2>&1; }
echo "témoin" > "$faux/temoin.txt"
mkdir -p "$faux/.local/share/momentum-ctl-win"
echo "données" > "$faux/.local/share/momentum-ctl-win/scenes.txt"

echo "== première installation"
installer
for exe in momentum-daemon momentum-kirigami ctl; do test -x "$faux/.local/bin/$exe"; done
grep -q "Installation terminée" "$faux/sortie.txt"

echo "== panne injectée (dossier des entrées de menu remplacé par un fichier)"
mv "$faux/.local/share/applications" "$faux/applications.vrai"
echo "bloque" > "$faux/.local/share/applications"
if installer; then
    echo "Échec : l'installation a réussi malgré la panne." >&2
    exit 1
fi
cat "$faux/sortie.txt"
grep -q "Installation partielle" "$faux/sortie.txt"
if grep -q "Installation terminée" "$faux/sortie.txt"; then
    echo "Échec : installation partielle annoncée comme terminée." >&2
    exit 1
fi
for exe in momentum-daemon momentum-kirigami ctl; do test -x "$faux/.local/bin/$exe"; done

echo "== nouvelle installation, cause levée"
rm "$faux/.local/share/applications"
mv "$faux/applications.vrai" "$faux/.local/share/applications"
installer
grep -q "Installation terminée" "$faux/sortie.txt"

echo "== désinstallation"
installer --desinstaller
for exe in momentum-daemon momentum-kirigami ctl; do test ! -e "$faux/.local/bin/$exe"; done
test ! -e "$faux/.local/share/applications/momentum-kirigami.desktop"
grep -qx "données" "$faux/.local/share/momentum-ctl-win/scenes.txt"
grep -qx "témoin" "$faux/temoin.txt"
echo "Installateur Linux : installation, panne annoncée, reprise et désinstallation réussies."
