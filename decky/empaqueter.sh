#!/bin/sh
# Construit l'extension Decky et son archive à installer :
#   sh decky/empaqueter.sh vX.Y.Z
# → openheadset-control-vX.Y.Z-decky.zip (dossier openheadset-control/ avec
# plugin.json, package.json, main.py, dist/index.js, licences, LISEZMOI).
# Demande Node.js (pnpm est pris par npx, version figée).
set -eu
version=$1
cd "$(dirname "$0")"
npx --yes pnpm@10.34.8 install --frozen-lockfile
npx --yes pnpm@10.34.8 run typecheck
npx --yes pnpm@10.34.8 run build
python3 -m unittest discover -s tests
test -s dist/index.js
travail=$(mktemp -d)
trap 'rm -rf "$travail"' EXIT
dossier="$travail/openheadset-control"
mkdir -p "$dossier/dist"
cp plugin.json package.json main.py LISEZMOI.md "$dossier/"
cp dist/index.js "$dossier/dist/"
cp ../LICENSE-MIT ../LICENSE-APACHE "$dossier/"
archive="$(pwd)/../openheadset-control-$version-decky.zip"
rm -f "$archive"
(cd "$travail" && zip -qrX "$archive" openheadset-control)
unzip -l "$archive" | grep -q 'openheadset-control/dist/index.js$'
echo "écrit : $archive"
