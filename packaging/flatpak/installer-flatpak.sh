#!/bin/sh
# Installation dans /app, appelée par le manifeste Flatpak après cargo build.
set -eu
install -Dm755 -t /app/bin target/release/momentum-kirigami \
    target/release/momentum-daemon target/release/ctl

# Service lancé par activation D-Bus (pas d'unité systemd dans le bac à
# sable) ; Flatpak réécrit Exec pour le lancer dans l'application.
install -d /app/share/dbus-1/services
printf '[D-BUS Service]\nName=org.openheadset.Control\nExec=/app/bin/momentum-daemon\n' \
    > /app/share/dbus-1/services/org.openheadset.Control.service

# Entrée de menu et logo exportés sous l'identifiant de l'application ; le
# nom d'origine (org.openheadset.control) reste disponible pour le client.
sed -e 's/^Icon=.*/Icon=org.openheadset.Control/' -e '/^StartupWMClass=/d' \
    assets/linux/momentum-kirigami.desktop > org.openheadset.Control.desktop
install -Dm644 -t /app/share/applications org.openheadset.Control.desktop
install -Dm644 assets/linux/org.openheadset.Control.metainfo.xml \
    /app/share/metainfo/org.openheadset.Control.metainfo.xml
for name in org.openheadset.Control org.openheadset.control; do
    install -Dm644 assets/logo/openheadset.svg \
        "/app/share/icons/hicolor/scalable/apps/$name.svg"
    for size in 16 22 24 32 48 64 128 256; do
        install -Dm644 "assets/logo/png/$size.png" \
            "/app/share/icons/hicolor/${size}x${size}/apps/$name.png"
    done
done
