#!/bin/sh
# Essai du confinement du service à l'exécution (A02 et A06 de l'audit du
# 24/10/2026) : les options de durcissement de l'unité sont reprises telles
# quelles dans un service temporaire (systemd-run --user), qui vérifie
# séparément lecture et écriture, sans créer aucun fichier :
#   - fichier ailleurs dans le dossier personnel : ni lisible ni inscriptible ;
#   - dossier des données : lisible et inscriptible ;
#   - Documents (s'il existe) : inscriptible ; /etc : non inscriptible ;
#   - bus de session et bus système (BlueZ) joignables ;
#   - binaire du service visible (s'il est installé dans ~/.local/bin).
#   sh scripts/essai-confinement.sh [unité] [fichier témoin] [complément]
# Avec un complément (drop-in), ses options remplacent celles de l'unité qui
# portent le même nom. Si AppArmor restreint les espaces de noms utilisateur
# (Ubuntu 24.04 et suivants), systemd n'applique pas le masquage des
# fichiers : ces contrôles sont alors signalés « non appliqués », pas
# comptés en échec.
# Le témoin est un fichier personnel existant (par défaut ~/.bashrc) : il
# est seulement lu, rien n'est créé ni supprimé.
set -eu
unite=${1:-assets/linux/momentum-daemon.service}
temoin=${2:-$HOME/.bashrc}
complement=${3:-}
restreint=false
if [ "$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null)" = 1 ]; then
    restreint=true
fi
: "${XDG_RUNTIME_DIR:=/run/user/$(id -u)}"
donnees="$HOME/.local/share/momentum-ctl-win"
mkdir -p "$donnees"
test -r "$temoin" || { echo "Fichier témoin illisible hors du bac à sable : $temoin" >&2; exit 2; }

# Options de [Service] après le commentaire « Durcissement », spécificateurs
# remplacés (%h : dossier personnel, %t : dossier d'exécution).
extraire() {
    awk '
        /^\[/ { section = $0 }
        section == "[Service]" && /^[A-Za-z]+=/ && !/^(Type|BusName|ExecStart|ExecStartPre|Restart|RestartSec)=/ { print }
    ' "$1" | sed -e "s|%h|$HOME|g" -e "s|%t|$XDG_RUNTIME_DIR|g"
}
options=$(extraire "$unite")
if [ -n "$complement" ]; then
    remplacees=$(extraire "$complement")
    cles=$(printf '%s\n' "$remplacees" | sed 's/=.*//' | sort -u)
    options=$(printf '%s\n' "$options" | grep -v -x -E "($(printf '%s' "$cles" | tr '\n' '|'))=.*"; printf '%s\n' "$remplacees")
fi
set --
nl='
'
ancien_ifs=$IFS
IFS=$nl
for option in $options; do
    set -- "$@" -p "$option"
done
IFS=$ancien_ifs
echo "Options reprises de $unite :"
printf '  %s\n' $options

controle=$(cat <<EOF
ok=0
verifier() { if eval "\$2"; then echo "ok : \$1"; else echo "ÉCHEC : \$1"; ok=1; fi; }
masquage() {
    if $restreint; then
        if eval "\$2"; then echo "ok : \$1"; else echo "non appliqué (AppArmor) : \$1"; fi
    else
        verifier "\$1" "\$2"
    fi
}
masquage "fichier personnel illisible" "! cat '$temoin' >/dev/null 2>&1"
masquage "fichier personnel non inscriptible" "! test -w '$temoin'"
verifier "dossier des données lisible" "test -r '$donnees'"
verifier "dossier des données inscriptible" "test -w '$donnees'"
if [ -d '$HOME/Documents' ]; then verifier "Documents inscriptible" "test -w '$HOME/Documents'"; fi
verifier "/etc non inscriptible" "! test -w /etc"
verifier "bus de session joignable" "busctl --user status org.freedesktop.DBus >/dev/null 2>&1"
verifier "bus système joignable" "busctl --system status org.freedesktop.DBus >/dev/null 2>&1"
if [ -e '$HOME/.local/bin/momentum-daemon' ]; then verifier "binaire du service visible" "test -x '$HOME/.local/bin/momentum-daemon'"; fi
exit \$ok
EOF
)
systemd-run --user --wait --pipe --quiet "$@" -- sh -c "$controle"
echo "Confinement vérifié à l'exécution."
