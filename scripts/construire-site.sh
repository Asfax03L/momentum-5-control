#!/bin/sh
# Construit le site GitHub Pages dans un dossier (défaut : _site) avec
# pandoc : accueil, guide d'utilisation et FAQ, en français (racine) et en
# anglais (en/). Sources : site/ et docs/GUIDE*.md, docs/FAQ*.md.
#
# Usage : sh scripts/construire-site.sh [dossier]
set -eu
# Arguments accentués : pandoc les lit selon la locale.
export LC_ALL=C.UTF-8
sortie=${1:-_site}
cd "$(dirname "$0")/.."
mkdir -p "$sortie/en" "$sortie/captures"

# page SOURCE DESTINATION LANGUE TITRE DESCRIPTION
page() {
    source=$1
    destination=$2
    langue=$3
    nom=$(basename "$destination")
    if [ "$langue" = fr ]; then
        set -- -V lang=fr -M racine= \
            -V evitement="Aller au contenu" -V nav-label="Navigation principale" \
            -V nav-accueil="Accueil" -V nav-guide="Guide" -V nav-faq="Questions fréquentes" \
            -V nav-telecharger="Télécharger" \
            -V accueil=index.html -V guide=guide.html -V faq=faq.html \
            -V autre-langue=en -V autre-nom=English -V autre-page="en/$nom" \
            -V pied="Projet non officiel, sans lien avec Sennheiser ni Sonova. Licence MIT ou Apache-2.0." \
            -M pagetitle="$4" -V description="$5"
    else
        set -- -V lang=en -M racine=../ \
            -V evitement="Skip to content" -V nav-label="Main navigation" \
            -V nav-accueil="Home" -V nav-guide="Guide" -V nav-faq="FAQ" \
            -V nav-telecharger="Download" \
            -V accueil=index.html -V guide=guide.html -V faq=faq.html \
            -V autre-langue=fr -V autre-nom=Français -V autre-page="../$nom" \
            -V pied="Unofficial project, not affiliated with Sennheiser or Sonova. MIT or Apache-2.0 licence." \
            -M pagetitle="$4" -V description="$5"
    fi
    # Identifiants des titres à la manière de GitHub : les ancres des
    # documents restent valables sur le site.
    pandoc --from markdown+gfm_auto_identifiers --to html5 \
        --template site/modele.html --lua-filter site/liens.lua \
        "$@" --output "$sortie/$destination" "$source"
}

page site/index.md index.html fr "Accueil" \
    "Réglages du Sennheiser MOMENTUM 5 sous Windows et Linux (non officiel)"
page docs/GUIDE.md guide.html fr "Guide d'utilisation" \
    "Installer et utiliser OpenHeadset Control"
page docs/FAQ.md faq.html fr "Questions fréquentes" \
    "Questions fréquentes sur OpenHeadset Control"
page site/en/index.md en/index.html en "Home" \
    "Sennheiser MOMENTUM 5 settings on Windows and Linux (unofficial)"
page docs/GUIDE.en.md en/guide.html en "User guide" \
    "Install and use OpenHeadset Control"
page docs/FAQ.en.md en/faq.html en "FAQ" \
    "Frequently asked questions about OpenHeadset Control"

cp site/style.css assets/logo/openheadset.svg "$sortie/"
for capture in windows-accueil-sombre windows-son-sombre kde-accueil-sombre kde-scenes-sombre; do
    cp "docs/captures/$capture.jpg" "$sortie/captures/"
done
echo "site construit dans $sortie"
