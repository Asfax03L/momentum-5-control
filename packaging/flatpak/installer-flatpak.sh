#!/bin/sh
# Installation dans /app, appelée par le manifeste Flatpak après cargo build.
set -eu
app=${FLATPAK_ID:-io.github.asfax03l.openheadset-control}
install -Dm755 -t /app/bin target/release/momentum-kirigami \
    target/release/momentum-daemon target/release/ctl

# Service lancé par activation D-Bus (pas d'unité systemd dans le bac à
# sable). Flatpak n'exporte que les fichiers nommés d'après l'application :
# le service est déclaré sous ce nom, qu'il prend en plus de
# org.openheadset.Control ; les clients le démarrent par ce nom.
install -d /app/share/dbus-1/services
printf '[D-BUS Service]\nName=%s\nExec=/app/bin/momentum-daemon\n' "$app" \
    > "/app/share/dbus-1/services/$app.service"

# Entrée de menu et logo exportés sous l'identifiant de l'application ; le
# nom d'origine (org.openheadset.control) reste disponible pour le client.
sed -e "s/^Icon=.*/Icon=$app/" -e '/^StartupWMClass=/d' \
    assets/linux/momentum-kirigami.desktop > "$app.desktop"
install -Dm644 -t /app/share/applications "$app.desktop"
install -Dm644 "assets/linux/$app.metainfo.xml" \
    "/app/share/metainfo/$app.metainfo.xml"
for name in "$app" org.openheadset.control; do
    install -Dm644 assets/logo/openheadset.svg \
        "/app/share/icons/hicolor/scalable/apps/$name.svg"
    for size in 16 22 24 32 48 64 128 256; do
        install -Dm644 "assets/logo/png/$size.png" \
            "/app/share/icons/hicolor/${size}x${size}/apps/$name.png"
    done
done
