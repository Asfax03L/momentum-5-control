#!/bin/sh
# Lance une commande (la suite de tests) avec un dossier de données témoin à
# la place de celui de l'utilisateur, puis vérifie qu'aucun fichier témoin
# n'a changé : aucun test n'écrit dans les données réelles (A01 de l'audit
# du 24/10/2026).
#   sh scripts/essai-temoin.sh cargo test
set -eu
temoin=$(mktemp -d)
donnees="$temoin/momentum-ctl-win"
mkdir -p "$donnees"
for f in batterie.csv scenes.txt raccourcis.txt prereglages.txt panneau.txt casque.txt; do
    echo "témoin $f" > "$donnees/$f"
done
empreintes() { (cd "$temoin" && find . -type f | sort | xargs sha256sum); }
avant=$(empreintes)
# Linux : $XDG_DATA_HOME ; Windows : %LOCALAPPDATA% (chemin Windows).
export XDG_DATA_HOME="$temoin"
if command -v cygpath >/dev/null; then
    LOCALAPPDATA=$(cygpath -w "$temoin")
    export LOCALAPPDATA
fi
"$@"
apres=$(empreintes)
if [ "$avant" != "$apres" ]; then
    echo "Échec : la commande a modifié le dossier de données témoin." >&2
    echo "$avant" > "$temoin.avant"
    echo "$apres" > "$temoin.apres"
    diff "$temoin.avant" "$temoin.apres" >&2 || true
    exit 1
fi
echo "Dossier de données témoin intact."
