# Contribuer à OpenHeadset Control

Merci de votre intérêt ! Le projet est **non officiel** et sans lien avec
Sennheiser ni Sonova. Il pilote un appareil réel : la prudence passe avant
les fonctions.

## Règles qui ne se discutent pas

- **Rien d'inventé.** Aucune commande, aucun identifiant de commande ni UUID
  qui ne soit documenté ou observé. Ce qui n'est pas vérifié est marqué
  « à valider sur le matériel ».
- **Lecture seule par défaut.** Une écriture n'est ajoutée qu'après avoir
  été capturée dans l'application officielle, puis rejouée et relue sur le
  casque. Les écritures sont réservées au modèle et au micrologiciel sur
  lesquels elles ont été vérifiées (voir le tableau de compatibilité du
  [README](README.md#compatibilité-momentum-5)).
- **Jamais** : mise à jour du micrologiciel (DFU), réinitialisation d'usine,
  balayage d'identifiants de commande, options de développeur, suppression
  d'un appairage, installation d'un pack de voix.
- **Aucun code copié** d'un autre projet ou de l'application officielle :
  tout est réimplémenté à partir des faits documentés ou observés, sources
  citées.
- **Données personnelles** : ni adresse Bluetooth complète, ni numéro de
  série, ni nom d'appareil personnel, ni chemin de dossier personnel, ni
  profil auditif dans le code, les journaux, les captures d'écran, les
  issues ou les PR. Le script `scripts/verifier-confidentialite.sh` (lancé
  par la CI) refuse les cas les plus courants.

## Signaler un problème ou un casque

Utilisez les modèles d'issue :

- **Bogue** : version, système, interface, étapes, et le diagnostic expurgé
  (page « À propos » > « Copier le diagnostic »).
- **Compatibilité** : un autre modèle ou un autre micrologiciel. Joignez
  la sortie de `probe --deep` (adresses et numéro de série sont masqués par
  défaut ; relisez-la avant de la publier). Le projet reste en lecture seule
  sur ce matériel tant qu'aucune écriture n'y a été vérifiée.
- **Idée** : décrivez le besoin ; si la fonction suppose une nouvelle
  écriture, dites d'où vient la commande (capture de l'app, documentation).

## Préparer l'environnement

- **Windows** : Rust stable (chaîne GNU ou MSVC). L'interface WinUI 3
  demande le Windows App Runtime 2.
- **Linux** : Rust stable, `dbus` et `pkg-config` ; pour le client KDE,
  Qt 6 et Kirigami (voir [docs/LINUX.md](docs/LINUX.md) ; sur Bazzite, dans
  une boîte distrobox Fedora).

```bash
cargo build
```

Le client KDE n'est pas compilé par défaut (Qt 6 requis) :

```bash
cargo build -p momentum-kirigami
```

Pour essayer les interfaces sans casque : `--demo` (casque simulé, aucun
service, aucune écriture).

## Avant d'ouvrir une PR

```bash
cargo fmt --check
```
```bash
cargo clippy --all-targets -- -D warnings
```
```bash
cargo test
```
```bash
sh scripts/verifier-confidentialite.sh
```

- Une PR par sujet, en français, avec ce qui a été vérifié et comment.
- Code nouveau testé quand c'est possible : le cœur (`momentum-core`) est
  en Rust pur, testable sans casque grâce à `MockTransport`.
- Toute écriture nouvelle ou modifiée : trames exactes, source (capture),
  essai sur le casque avec relecture et retour à la valeur d'origine.
- Documentation à jour : README, `CHANGELOG.md` (section « Non publié »),
  `docs/MOMENTUM5.md` pour le protocole.

## Architecture en bref

Le service `momentum-daemon` est le seul à parler au casque ; les
interfaces (`frontends/winui`, `frontends/kirigami`) et `ctl` sont ses
clients. Détails : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) et
[frontends/README.md](frontends/README.md).

## Licence

En contribuant, vous acceptez que votre contribution soit publiée sous la
double licence du projet, au choix [MIT](LICENSE-MIT) ou
[Apache-2.0](LICENSE-APACHE).
