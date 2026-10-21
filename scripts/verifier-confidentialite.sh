#!/bin/sh
# Refuse ce qui ne doit pas être publié dans ce dépôt public : adresse
# Bluetooth complète (sauf exemples fictifs : 00:…, 02:…, AA:BB:CC:…,
# aa:bb:cc:…, 11:22:33:…), chemin d'un dossier personnel Windows ou Linux,
# APK, captures Bluetooth brutes, export de profil auditif. Lancé par la CI ;
# à lancer aussi avant un commit : sh scripts/verifier-confidentialite.sh
set -u
status=0

# Fichiers suivis par Git (le script lui-même est exclu : il contient les motifs).
files=$(git ls-files | grep -v '^scripts/verifier-confidentialite.sh$')

macs=$(printf '%s\n' "$files" | xargs grep -nIoE '([0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}' 2>/dev/null |
    grep -viE ':(00|02)[:-]|:aa[:-]bb[:-]cc|:11[:-]22[:-]33')
if [ -n "$macs" ]; then
    echo "Adresse Bluetooth complète (remplacer par un exemple fictif 02:…) :"
    echo "$macs"
    status=1
fi

paths=$(printf '%s\n' "$files" | xargs grep -nIE 'C:\\Users\\[A-Za-z0-9_.-]+\\|/home/[a-z_][a-z0-9_-]*/|/var/home/' 2>/dev/null |
    grep -vE 'C:\\Users\\(leo|x|nom|exemple|<[^>]*>)\\|/home/(x|nom|exemple|a b)/')
if [ -n "$paths" ]; then
    echo "Chemin personnel (utiliser %LOCALAPPDATA%, ~ ou un exemple) :"
    echo "$paths"
    status=1
fi

forbidden=$(printf '%s\n' "$files" | grep -iE '\.(apk|xapk|apks|btsnoop|pcap|pcapng|cfa)$|momentum-profil-.*\.json$|(^|/)docs/research/')
if [ -n "$forbidden" ]; then
    echo "Fichier à garder hors du dépôt public :"
    echo "$forbidden"
    status=1
fi

[ "$status" -eq 0 ] && echo "Confidentialité : rien à signaler."
exit "$status"
