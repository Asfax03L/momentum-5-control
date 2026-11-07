# Ligne de commande

`ctl` règle le casque par le service ; `probe` fait un diagnostic en lecture seule.

## Ligne de commande : `ctl`

> **Linux** : `ctl` passe par le service, ou se connecte lui-même par
> BlueZ. Voir [docs/LINUX.md](LINUX.md).

```bash
cargo run --release --bin ctl -- etat
```
```bash
cargo run --release --bin ctl -- mode adaptatif
```
```bash
cargo run --release --bin ctl -- niveau 50
```
```bash
cargo run --release --bin ctl -- bande 8 -2.5
```

Sous-commandes : `etat [--json]`, `scenes`, `scene <nom>`,
`mode arret|adaptatif|personnalise`, `niveau <0-100>`
(0 = ANC max, 50 = neutre, 100 = transparence max), `anti-vent on|off`,
`spatial on|off`, `eq`, `bande <1-8> <dB>`, `plat`, `basses on|off`,
`nom [nouveau]`, `connexions`, `connecter <n>`, `deconnecter <n>`,
`suivi-tete on|off` (audio spatial activé), `multipoint on|off`,
`volume-invites <0-100>`, `voix on|off` (Speech Clarity), `profil on|off` (profil de personnalisation
du son déjà enregistré), `tactile on|off`, `protection-batterie on|off`,
`smart-pause on|off`, `reponse-auto on|off`, `appels-confort on|off`,
`extinction jamais|15|30|60`, `invites tonalites|voix`. Chaque réglage reproduit à l'octet près les trames de Smart
Control Plus, puis l'état est relu : `ctl` échoue si le casque ne l'a pas
appliqué.

`exporter-profil <fichier.json>` sauvegarde en lecture seule le profil de
personnalisation du son : octets bruts des blocs `2003`/`2005`/`2007`,
date, modèle, firmware, état `2001` et mode audio bruts, empreinte. Ni numéro
de série ni adresse Bluetooth, mais le profil est une donnée auditive
personnelle : ne pas publier ce fichier. Sous Linux, le service (unité
durcie, [`docs/SECURITE.md`](SECURITE.md)) n'écrit que dans
`~/Documents`. Un fichier existant n'est jamais
écrasé, et aucune restauration n'est proposée.

### Sortie JSON pour les scripts

`--json` est global : `ctl --json etat` et `ctl etat --json` sont
équivalents. La sortie standard contient **un seul objet JSON**, en version
`1` ; les messages humains et les journaux restent sur stderr. Un échec
donne `ok: false`, `erreur` et un code de sortie non nul, y compris en cas
d'arguments invalides. Sans cette option, l'affichage reste textuel.

- `etat` conserve exactement son format historique (ordre des champs
  compris) : `connecte`, `batterie`, `bruit`, etc.
- `eq` renvoie `egaliseur` (gains et bornes en **dixièmes de dB**, et
  `basses`) ; `connexions` renvoie `connexions` (nom, `connecte`, `ce_pc`,
  sans adresse ni identifiant opaque) ; `nom` sans argument renvoie `nom`.
- `scenes` renvoie la liste des noms valides dans `scenes` et les erreurs
  de lecture ou d'analyse du fichier dans `avertissements`, sans service
  ni casque.
- Les réglages et `exporter-profil` renvoient `ok`, `etat` (valeurs relues),
  `resultat` (issue donnée par le service, si disponible) et `details`
  (rapport d'une scène ou fichier exporté, si disponible).

Dans le champ `etat` des réponses aux réglages, les valeurs non relues
sont `null` : batterie, bruit, audio
spatial, suivi de tête, mode audio, statut du profil, protection de la
batterie, commandes tactiles, Smart Pause, réponse automatique, appels
confortables, extinction en secondes, mode des invites, codec, fréquence,
égaliseur, connexions, nom, Multipoint et volume des invites en pourcentage.
Les clés exactes sont documentées dans
[`momentum_core::json`](../crates/momentum-core/src/json.rs).
Une relecture partielle reste partielle : `niveau` en connexion directe
relit le bruit, pas toute la batterie ni l'égaliseur. Le JSON n'ajoute
aucune commande au casque et ne réessaie pas une écriture.

Une lecture absente vaut `null`, une liste effectivement lue et vide vaut
`[]`. Le service ne distingue pas encore une liste de connexions vide
d'une liste non lue : elle est donc rendue par `null` dans ce cas. Un
acquittement conserve `resultat: "accepté par le casque"` et les champs
non relus à `null` ; il ne devient pas une confirmation d'application.
Les codes du service sont conservés : `0` succès, `1` refus ou annulation,
`2` non appliqué, `3` résultat inconnu. Les autres erreurs d'exécution
sortent avec `1` ; les arguments invalides avec `2`.

Exemple shell (avec `jq` pour lire le résultat) :

```sh
if resultat=$(ctl --json mode adaptatif); then
    printf '%s\n' "$resultat" | jq -r '.etat.bruit.mode'
else
    printf '%s\n' "$resultat" | jq -r '.erreur' >&2
fi
```

Exemple Stream Deck, dans une action de **lanceur de commande** : choisir
`ctl` comme exécutable et `--json scene Travail` comme arguments. Le
lanceur peut récupérer stdout pour lire `ok`, `etat` et `details`, et le
code de sortie pour signaler l'échec. La scène reste définie dans
`scenes.txt` et applique les mêmes réglages que `ctl scene Travail`.

Avec `--json`, `man`, `completions`, l'aide et la version sont encapsulés
dans un objet `version`, `type`, `contenu`. Pour installer directement un
script de complétion ou un manuel, utiliser les commandes sans `--json`.

### Suite d'essais sur le casque

Avant chaque publication, `ctl essais` rejoue les essais faits à la main :
pour chaque réglage validé (niveau, anti-vent, bande 1, basses, tactile,
Smart Pause, réponse automatique, appels confortables, extinction,
invites, volume des invites, Speech Clarity), une autre valeur est écrite
et relue, puis la valeur d'origine est remise et relue. La première
anomalie arrête la suite, après une tentative de remise. Jamais : nom,
multipoint, connexions, profil personnalisé, langue, priorité du mode
audio. Service requis.

```bash
ctl essais --simulation
```
```bash
ctl essais --rapport essais-casque.md
```

`--liste niveau,anti-vent` choisit les essais ; `--json` rend un objet avec
chaque résultat et le compte rendu. Le compte rendu Markdown (date,
version, système, modèle, micrologiciel) ne contient ni nom, ni adresse,
ni numéro de série. Code de sortie 1 en cas d'anomalie.

### Complétion et manuel

`ctl completions <shell>` écrit un script sur la sortie standard pour
`bash`, `zsh`, `fish`, `powershell` ou `elvish`. `ctl man` écrit la page
`ctl.1` au format roff. Ces deux commandes fonctionnent sans casque ni
service et utilisent la même définition Clap que les commandes de réglage.

```bash
# Activer la complétion bash dans le shell courant.
source <(ctl completions bash)
# Consulter le manuel généré sans l'installer.
ctl man > ctl.1
man -l ./ctl.1
```

L'archive Linux contient `man/ctl.1` et les complétions bash, zsh et fish.
`installer-linux.sh` les installe sans sudo dans les dossiers suivants
(sous `$XDG_DATA_HOME`, ou `~/.local/share` par défaut) :

- `man/man1/ctl.1` : consultable avec `man ctl` ;
- `bash-completion/completions/ctl` : chargé par bash-completion ;
- `zsh/site-functions/_ctl` : ajouter ce dossier à `fpath` avant `compinit` ;
- `fish/vendor_completions.d/ctl.fish` : chargé par fish.

Pour zsh, placer dans `~/.zshrc`, avant un éventuel appel existant à
`compinit` :

```zsh
fpath=("${XDG_DATA_HOME:-$HOME/.local/share}/zsh/site-functions" $fpath)
autoload -Uz compinit
compinit
```

Si `man` ne trouve pas le dossier utilisateur, utiliser
`man -M "${XDG_DATA_HOME:-$HOME/.local/share}/man" ctl`. Depuis les sources,
l'installateur génère ces fichiers avec `target/release/ctl` avant toute
copie ; depuis l'archive, il vérifie leur présence avant l'installation.

## Diagnostic : `probe`

```bash
cargo run --release --bin probe -- --all-gets --output rapport.txt
```

| Option | Effet |
|---|---|
| `--device <nom>` | filtre sur le nom (par défaut « momentum » ou « sennheiser ») |
| `--service <UUID>` | service RFCOMM à utiliser si aucun ne s'appelle « GAIA » |
| `--all-gets` | envoie aussi les autres GET documentés (modèle, firmware, codec, bruit…) |
| `--deep` | inventaire complet en lecture seule : réglages, égaliseur, appareils appairés au casque, features Qualcomm |
| `--output <fichier>` | enregistre aussi le rapport |
| `--verbose` | journal détaillé sur stderr (trames brutes) |
| `--show-address` | affiche adresses Bluetooth et numéro de série (masqués par défaut) |
| `--timeout-ms <n>` | délai d'attente de chaque réponse (3000 par défaut) |

Le rapport liste les appareils appairés, les services RFCOMM (avec attributs
SDP bruts) et GATT, puis chaque échange `TX` / `RX` en hexadécimal avec son
interprétation. Les fichiers `rapport*.txt` sont ignorés par git : ils
contiennent les noms de vos appareils.
