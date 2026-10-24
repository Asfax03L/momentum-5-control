#!/bin/sh
# Prépare le PKGBUILD de l'AUR pour une version, sans rien publier :
#   sh packaging/aur/preparer.sh 0.14.0 dossier [archive.tar.gz]
# Sans archive, celle de l'étiquette vX.Y.Z est téléchargée depuis GitHub
# (la même que celle que makepkg téléchargera). Écrit dans « dossier » le
# PKGBUILD rempli, l'archive et, si makepkg est là, le .SRCINFO.
set -eu
[ $# -ge 2 ] || { echo "Usage : preparer.sh X.Y.Z dossier [archive.tar.gz]" >&2; exit 2; }
version=$1
sortie=$2
archive=${3:-}
ici=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$sortie"
nom="openheadset-control-$version.tar.gz"
if [ -n "$archive" ]; then
    cp "$archive" "$sortie/$nom"
else
    curl -fsSLo "$sortie/$nom" \
        "https://github.com/Asfax03L/openheadset-control/archive/refs/tags/v$version.tar.gz"
fi
empreinte=$(sha256sum "$sortie/$nom" | cut -d' ' -f1)
sed -e "s/@VERSION@/$version/" -e "s/@SHA256@/$empreinte/" "$ici/PKGBUILD" > "$sortie/PKGBUILD"
! grep -q '@[A-Z0-9]*@' "$sortie/PKGBUILD"
# makepkg refuse de tourner en root (essai Arch : .SRCINFO fait ensuite).
if command -v makepkg >/dev/null && [ "$(id -u)" -ne 0 ]; then
    (cd "$sortie" && makepkg --printsrcinfo > .SRCINFO)
fi
echo "PKGBUILD prêt dans $sortie (rien n'a été publié sur l'AUR)."
