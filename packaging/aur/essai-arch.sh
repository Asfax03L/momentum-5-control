#!/bin/sh
# Essai du PKGBUILD dans un conteneur Arch Linux jetable (CI) : makepkg
# depuis une archive des sources, namcap, puis installation, chargement de
# l'interface et retrait par verifier-installation.sh.
#   sh packaging/aur/essai-arch.sh X.Y.Z archive.tar.gz
set -eu
if [ ! -f /.dockerenv ] && [ ! -f /run/.containerenv ]; then
    echo "Cet essai doit être lancé dans un conteneur jetable." >&2
    exit 2
fi
version=$1
archive=$2
pacman -Syu --noconfirm --needed base-devel namcap sudo
useradd -m constructeur
echo 'constructeur ALL=(ALL) NOPASSWD: /usr/bin/pacman' > /etc/sudoers.d/constructeur
dossier=/home/constructeur/aur
sh packaging/aur/preparer.sh "$version" "$dossier" "$archive"
chown -R constructeur "$dossier"
su constructeur -c "cd $dossier && makepkg --syncdeps --noconfirm && makepkg --printsrcinfo > .SRCINFO"
paquet=$(find "$dossier" -name "openheadset-control-$version-*-x86_64.pkg.tar.zst")
test "$(printf '%s\n' "$paquet" | wc -l)" -eq 1
namcap "$dossier/PKGBUILD"
namcap "$paquet" | tee /tmp/namcap.txt
# Erreurs de namcap (« E: ») : refusées ; avertissements affichés seulement.
! grep -q ' E: ' /tmp/namcap.txt
sha256sum "$paquet" | cut -d' ' -f1 > "$paquet.sha256"
sh packaging/linux/verifier-installation.sh pacman "$paquet"
mkdir -p target/paquets/aur
cp "$dossier/PKGBUILD" "$dossier/.SRCINFO" target/paquets/aur/
