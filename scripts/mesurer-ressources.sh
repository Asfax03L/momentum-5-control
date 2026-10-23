#!/bin/sh
# Budget de ressources (docs/RESSOURCES.md), Linux : lance le service sans
# casque et le client KDE en démonstration, sans écran, sur un bus de
# session privé, puis relève au bout de DUREE secondes de repos :
#   - la mémoire résidente (VmRSS, Mio) ;
#   - le temps processeur consommé depuis le lancement (utilisateur +
#     système, secondes).
# Échoue si un relevé dépasse son seuil (variables ci-dessous).
#
# Usage : sh scripts/mesurer-ressources.sh <dossier des binaires> [durée]
# Aucun accès au casque : le service cherche un casque sans en trouver.
set -eu
bins=$1
duree=${2:-30}
# Seuils larges (relevés du 23/10/2026 dans docs/RESSOURCES.md, multipliés
# par 3 environ) : ils repèrent une dérive, pas une variation normale.
SEUIL_SERVICE_MIO=${SEUIL_SERVICE_MIO:-40}
SEUIL_SERVICE_CPU=${SEUIL_SERVICE_CPU:-3}
SEUIL_CLIENT_MIO=${SEUIL_CLIENT_MIO:-450}
SEUIL_CLIENT_CPU=${SEUIL_CLIENT_CPU:-15}

if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ] || [ -z "${MESURE_BUS_PRIVE:-}" ]; then
    # Bus privé : aucune interface ni service de l'utilisateur n'est touché.
    exec env MESURE_BUS_PRIVE=1 dbus-run-session -- sh "$0" "$@"
fi

releve() {
    # pid nom seuil_mio seuil_cpu
    rss=$(awk '/^VmRSS:/ { printf "%.1f", $2 / 1024 }' "/proc/$1/status")
    ticks=$(awk '{ print $14 + $15 }' "/proc/$1/stat")
    cpu=$(awk -v t="$ticks" -v hz="$(getconf CLK_TCK)" 'BEGIN { printf "%.2f", t / hz }')
    printf '| %s | %s Mio | %s s |\n' "$2" "$rss" "$cpu"
    awk -v r="$rss" -v c="$cpu" -v sr="$3" -v sc="$4" -v n="$2" 'BEGIN {
        if (r > sr) { printf "%s : %s Mio > %s Mio\n", n, r, sr > "/dev/stderr"; exit 1 }
        if (c > sc) { printf "%s : %s s > %s s\n", n, c, sc > "/dev/stderr"; exit 1 }
    }'
}

"$bins/momentum-daemon" > mesure-service.log 2>&1 &
service=$!
QT_QPA_PLATFORM=offscreen "$bins/momentum-kirigami" --demo > mesure-client.log 2>&1 &
client=$!
sleep "$duree"
for pid in $service $client; do
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "processus $pid arrêté avant le relevé" >&2
        cat mesure-service.log mesure-client.log >&2
        exit 1
    fi
done
echo "Relevé après $duree s au repos :"
echo
echo "| Processus | Mémoire résidente | Processeur |"
echo "|---|---|---|"
status=0
releve "$service" "momentum-daemon (sans casque)" "$SEUIL_SERVICE_MIO" "$SEUIL_SERVICE_CPU" || status=1
releve "$client" "momentum-kirigami --demo" "$SEUIL_CLIENT_MIO" "$SEUIL_CLIENT_CPU" || status=1
kill "$service" "$client" 2>/dev/null || true
exit $status
