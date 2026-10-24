#!/bin/sh
# Sources Cargo figées du Flatpak : packaging/flatpak/cargo-sources.json,
# produit depuis Cargo.lock par flatpak-cargo-generator (flatpak-builder-
# tools), à une révision et une empreinte fixées. Chaque crate y figure
# avec son empreinte SHA-256 : la construction se fait ensuite sans réseau.
#
# Demande python3 (avec venv) et le réseau ; à lancer depuis le dépôt.
set -eu
cd "$(dirname "$0")/../.."
revision=41c20aa10819cdb2a4f3ca171758a96d1955c018
empreinte=0a2db6be87d75910facef28ab46d4d6460802e8419ab850d0caa6a364d26b380
outils=$(mktemp -d)
trap 'rm -rf "$outils"' EXIT
trap 'exit 1' HUP INT TERM
curl -fsSLo "$outils/flatpak-cargo-generator.py" \
    "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$revision/cargo/flatpak-cargo-generator.py"
echo "$empreinte  $outils/flatpak-cargo-generator.py" | sha256sum -c -
python3 -m venv "$outils/venv"
"$outils/venv/bin/pip" install --quiet "aiohttp==3.14.5" "PyYAML==6.0.3" "tomlkit==0.15.2"
"$outils/venv/bin/python" "$outils/flatpak-cargo-generator.py" Cargo.lock \
    -o packaging/flatpak/cargo-sources.json
echo "écrit : packaging/flatpak/cargo-sources.json"
