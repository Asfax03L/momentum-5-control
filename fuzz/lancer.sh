#!/bin/sh
# Une durée par cible ; les autres cibles passent même si l'une échoue.
set -eu
seconds=${1:-60}
case "$seconds" in
    ''|*[!0-9]*) echo "Durée attendue : un entier de 1 à 600 secondes." >&2; exit 2 ;;
esac
if [ "$seconds" -lt 1 ] || [ "$seconds" -gt 600 ]; then
    echo "Durée attendue : un entier de 1 à 600 secondes." >&2
    exit 2
fi
cd "$(dirname "$0")/.."
# Le choix est local à ce script : rust-toolchain.toml reste stable.
export RUSTUP_TOOLCHAIN=nightly-2026-10-22
python3 fuzz/preparer-corpus.py
cargo fetch --locked --manifest-path fuzz/Cargo.toml
mkdir -p fuzz/logs
status=0
for target in frame_decode frame_stream sdp_element messages; do
    if cargo fuzz run "$target" -- -max_total_time="$seconds" -timeout=10 \
        -max_len=4096 -rss_limit_mb=1024 -print_final_stats=1 \
        > "fuzz/logs/$target.log" 2>&1; then
        echo "$target : terminé ($seconds secondes demandées)."
    else
        echo "$target : échec, voir fuzz/logs/$target.log." >&2
        status=1
    fi
    tail -n 20 "fuzz/logs/$target.log"
done
exit "$status"
