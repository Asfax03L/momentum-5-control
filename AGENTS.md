# Consignes pour les agents de code

Ce fichier s'adresse aux agents (Codex et autres) qui travaillent sur ce
dépôt. Les règles complètes du projet sont dans
[CONTRIBUTING.md](CONTRIBUTING.md) ; ce qui suit les résume et les complète.

## Le projet

OpenHeadset Control pilote un casque réel, le Sennheiser MOMENTUM 5, par le
protocole Qualcomm GAIA V3 (Bluetooth RFCOMM). Projet non officiel, en Rust :

- `crates/momentum-core` : protocole, casque, scènes, modèles d'affichage
  (Rust pur, testable sans casque grâce à `MockTransport`) ;
- `crates/momentum-service` : worker, seul propriétaire de la connexion ;
- `crates/momentum-client` : API du service (`Command`, `Event`,
  `Snapshot`) ;
- `crates/momentum-daemon` : le service (D-Bus sous Linux, canal nommé sous
  Windows) ;
- `crates/momentum-cli` : `ctl` et `probe` ;
- `frontends/winui` (Windows), `frontends/kirigami` (KDE, Qt 6),
  `plasma/` (widget Plasma, QML) : clients du service.

Architecture : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Protocole :
[docs/MOMENTUM5.md](docs/MOMENTUM5.md). Feuille de route :
[docs/ROADMAP.md](docs/ROADMAP.md).

## Interdits absolus

- **N'inventer aucune commande** du casque, aucun identifiant de commande,
  aucun UUID. Ne rien ajouter ni modifier dans `protocol/command.rs`, dans
  les écritures de `device.rs` ou dans la liste `Command` de l'API, sauf si
  la tâche le demande explicitement en citant la capture d'origine.
- **Jamais** : mise à jour du micrologiciel (DFU), réinitialisation
  d'usine, balayage d'identifiants, options de développeur, suppression d'un
  appairage, installation d'un pack de voix.
- **Ne pas affaiblir les garde-fous** : lecture seule hors du modèle et du
  micrologiciel validés, relecture après chaque écriture, refus du service
  quand le casque est absent.
- **Aucun code copié** d'un autre projet ou de l'application officielle.
- **Aucune donnée personnelle** dans le code, les tests, les journaux, les
  captures ou les PR : ni adresse Bluetooth complète (exemples fictifs
  `02:…` ou `AA:BB:CC:…` seulement), ni numéro de série, ni nom d'appareil
  personnel, ni chemin de dossier personnel, ni profil auditif.

## Manière de travailler

- **Français** pour le code (commentaires, messages, textes affichés), les
  commits et les PR, comme le reste du dépôt.
- **Une tâche, une branche, une PR**, ciblée sur `main`. Ne pas toucher aux
  fichiers sans rapport avec la tâche.
- **Ne pas modifier `CHANGELOG.md`** : le mainteneur l'écrit à la fusion.
  Décrire les changements dans la PR, avec deux paragraphes destinés aux
  notes de publication (voir l'en-tête de `CHANGELOG.md`) :
  - **En bref** : ce qui change pour l'utilisateur et qui est concerné
    (Windows, KDE, Flatpak, paquets, `ctl`…), en langage simple, sans
    jargon interne ;
  - **À faire après la mise à jour** : chaque action nécessaire, pas à
    pas et par système (relancer l'interface, mettre à jour le widget,
    adapter un script, désinstaller une ancienne méthode…), ou « Rien de
    particulier » avec la raison.
- **Suivre le style du code voisin** : densité de commentaires, noms en
  anglais pour le code et en français pour les textes, erreurs typées
  (`thiserror`), pas d'`unwrap` hors des tests.
- **Version minimale de Rust** : `rust-version` du `Cargo.toml` (vérifiée
  par la tâche `msrv` de la CI) ; une nouvelle dépendance doit la respecter
  et passer `cargo deny` (`deny.toml`).
- **Le casque n'est pas disponible** pour un agent : rien ne doit exiger un
  essai matériel pour être vérifié. Si une tâche en demande un, le dire
  dans la PR ; le mainteneur le fera.

## Vérifications avant la PR

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

Le client KDE (`momentum-kirigami`) n'est pas compilé par défaut : il
demande Qt 6 et Kirigami (`QMAKE=/usr/bin/qmake6 cargo clippy -p
momentum-kirigami`). L'interface WinUI ne fonctionne que sous Windows.

La PR indique ce qui a été vérifié et comment, et ce qui reste à essayer sur
le casque, le cas échéant.
