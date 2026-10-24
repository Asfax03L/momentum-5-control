#!/bin/sh
# Prépare les fichiers d'une soumission à Flathub pour une version publiée
# (docs/FLATHUB.md), sans rien envoyer :
#   sh packaging/flatpak/flathub.sh v0.13.0 dossier
# Écrit dans « dossier » :
#   - io.github.asfax03l.OpenHeadsetControl.yml : le manifeste du dépôt,
#     source du code remplacée par l'étiquette et sa révision (git) ;
#   - cargo-sources.json : sources Cargo figées de cette révision ;
#   - flathub.json : x86_64 seulement (le seul essayé sur le casque).
set -eu
[ $# -eq 2 ] || { echo "Usage : flathub.sh vX.Y.Z dossier" >&2; exit 2; }
tag=$1
sortie=$2
cd "$(dirname "$0")/../.."
app=io.github.asfax03l.OpenHeadsetControl
revision=$(git rev-parse --verify "refs/tags/$tag^{commit}")
[ "$(git rev-parse HEAD)" = "$revision" ] ||
    { echo "Extraire d'abord l'étiquette : git checkout $tag" >&2; exit 1; }
mkdir -p "$sortie"

# Le bloc « type: dir » (et ses « skip ») devient une source git figée.
awk -v tag="$tag" -v rev="$revision" '
    /^      - type: dir$/ {
        print "      - type: git"
        print "        url: https://github.com/Asfax03L/openheadset-control.git"
        print "        tag: " tag
        print "        commit: " rev
        skip = 1
        next
    }
    skip && /^        / { next }
    { skip = 0; print }
' "packaging/flatpak/$app.yml" > "$sortie/$app.yml"
grep -q "commit: $revision" "$sortie/$app.yml"
! grep -q 'type: dir' "$sortie/$app.yml"

sh packaging/flatpak/sources-cargo.sh
cp packaging/flatpak/cargo-sources.json "$sortie/cargo-sources.json"
printf '{\n  "only-arches": ["x86_64"]\n}\n' > "$sortie/flathub.json"
echo "Fichiers prêts dans $sortie (rien n'a été envoyé à Flathub)."
