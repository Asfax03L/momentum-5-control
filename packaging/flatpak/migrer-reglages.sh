#!/bin/sh
# Migration des réglages vers le Flatpak io.github.asfax03l.openheadset-control
# (v0.13.0 et suivantes), à lancer une fois, hors du bac à sable (le Flatpak
# ne voit pas les dossiers des autres installations).
#
# Reprend préréglages, scènes, raccourcis, panneau rapide, thème, historique
# de la batterie et profil, depuis :
#   - l'ancien Flatpak org.openheadset.Control (v0.12.0 et avant) :
#     ~/.var/app/org.openheadset.Control/data/momentum-ctl-win ;
#   - ou l'installation par script ou par paquet :
#     ~/.local/share/momentum-ctl-win.
# Une seule des deux présente : elle est prise. Les deux : le script
# s'arrête sans rien copier ; --depuis flatpak|local choisit alors.
#
# Copie seulement : rien n'est supprimé, et un fichier déjà présent dans le
# nouveau Flatpak est gardé tel quel. --essai affiche ce qui serait copié
# sans rien copier.
set -eu
umask 077
nouveau=io.github.asfax03l.openheadset-control
ancien=org.openheadset.Control
local_data=${XDG_DATA_HOME:-$HOME/.local/share}/momentum-ctl-win
ancien_data=$HOME/.var/app/$ancien/data/momentum-ctl-win
cible=$HOME/.var/app/$nouveau/data/momentum-ctl-win

essai=false
depuis=""
while [ $# -gt 0 ]; do
    case "$1" in
        --essai) essai=true ;;
        --depuis)
            [ $# -ge 2 ] || { echo "--depuis attend flatpak ou local." >&2; exit 2; }
            depuis=$2
            shift
            ;;
        *) echo "Option inconnue : $1 (--essai, --depuis flatpak|local)." >&2; exit 2 ;;
    esac
    shift
done

case "$depuis" in
    flatpak) source=$ancien_data ;;
    local) source=$local_data ;;
    "")
        if [ -d "$ancien_data" ] && [ -d "$local_data" ]; then
            echo "Deux anciennes installations trouvées, rien n'a été copié :" >&2
            for d in "$ancien_data" "$local_data"; do
                echo "  $d ($(find "$d" -type f | wc -l) fichier(s), modifié le $(date -r "$d" '+%d/%m/%Y %H:%M'))" >&2
            done
            echo "Choisir celle qui sert : --depuis flatpak (la première) ou --depuis local (la seconde)." >&2
            exit 2
        elif [ -d "$ancien_data" ]; then
            source=$ancien_data
        else
            source=$local_data
        fi
        ;;
    *) echo "--depuis attend flatpak ou local." >&2; exit 2 ;;
esac
if [ ! -d "$source" ]; then
    echo "Rien à migrer : $source n'existe pas."
    exit 0
fi

# Le nouveau Flatpak lancé pourrait réécrire ses fichiers pendant la copie.
if ! $essai && command -v flatpak >/dev/null &&
    flatpak ps --columns=application 2>/dev/null | grep -qx "$nouveau"; then
    echo "Le Flatpak $nouveau est lancé : le fermer d'abord" >&2
    echo "  flatpak kill $nouveau" >&2
    exit 1
fi

echo "Source : $source"
echo "Cible : $cible"
copies=0
gardes=0
# Noms de fichiers sans retour à la ligne (ceux de l'application).
liste=$(cd "$source" && find . -type f)
ancien_ifs=$IFS
IFS='
'
for f in $liste; do
    f=${f#./}
    if [ -e "$cible/$f" ]; then
        echo "gardé (déjà présent) : $f"
        gardes=$((gardes + 1))
        continue
    fi
    if $essai; then
        echo "serait copié : $f"
    else
        mkdir -p "$(dirname "$cible/$f")"
        cp -p "$source/$f" "$cible/$f"
        chmod go-rwx "$cible/$f"
        echo "copié : $f"
    fi
    copies=$((copies + 1))
done
IFS=$ancien_ifs

if $essai; then
    echo "Essai : $copies fichier(s) à copier, $gardes déjà présent(s). Rien n'a été copié."
else
    echo "Migration terminée : $copies fichier(s) copié(s), $gardes gardé(s). La source n'a pas été modifiée."
fi
