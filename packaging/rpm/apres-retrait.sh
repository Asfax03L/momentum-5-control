#!/bin/sh
# À la suppression finale seulement : retirer les répertoires devenus vides.
# Aucun service, aucun dossier personnel, aucun fichier conservé localement.
if [ "${1:-1}" -eq 0 ]; then
    rmdir /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid/contents/ui \
        /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid/contents \
        /usr/share/plasma/plasmoids/org.openheadset.control.plasmoid \
        /usr/share/doc/openheadset-control 2>/dev/null || :
fi
