# Consolidation 2026 : suivi des constats de revue

Revue indépendante du 21/10/2026 (constats F01 à F14), requalifiés le
22/10/2026 sur ce dépôt (`main` après la v0.8.0 et les PR #11 à #14). Statuts :
**Confirmé** (le défaut existe dans le code actuel), **Corrigé** (avec la PR
et la preuve), **En cours**, **Non reproduit**. Une correction n'est notée
« Corrigé » qu'avec un test automatique ou un essai daté ; un essai sur le
casque est indiqué comme tel.

| Constat | Résumé | Requalification (22/10) | Statut | Correction et preuve |
|---|---|---|---|---|
| F01 | `ctl` (par le service) prend un événement sans rapport pour la confirmation de sa commande | `ctl.rs` : tout `State`, `Equalizer`, `Info`, `Connections` termine la commande en succès ; l'API n'a pas d'identifiant de demande | Corrigé | Demandes numérotées (`Submit` → `id`, résultat unique `Event::Done { id, outcome }`) ; `ctl` n'attend que le résultat de sa demande ; tests `api_json_contract`, `only_its_own_result_ends_a_request` ; essayé sur le M5 le 22/10 (Bazzite et Windows : anti-vent inversé puis remis, « appliqué et relu ») |
| F02 | Écritures permises avant identification ; firmware non vérifié | Firmware vérifié depuis la PR #11 ; mais `Headset::new` autorisait les écritures avant `identify` | Corrigé | `Headset::new` refuse toute écriture avant `identify` ; constructeur validé réservé aux tests (fonctionnalité `essais`, absente des binaires) ; test `writes_are_refused_before_identification` (aucune trame envoyée) |
| F03 | WinUI ignore les erreurs d'envoi au service | `app.rs` `spawn_sender` : `let _ = Client.send(&command)` | Corrigé | WinUI et client KDE : le numéro ou le refus immédiat revient à la fenêtre, l'attente ne finit que sur le résultat de la demande (`momentum_client::tracker`) ; un refus n'est plus pris pour une panne du service (KDE) ; essai : scène inexistante refusée tout de suite, code 1 pour `ctl` |
| F04 | Redécouverte filtrée par le nom (« momentum », « sennheiser ») | `worker.rs` `NAME_FILTERS`, filtre dans les deux transports | Corrigé | Les transports listent les appareils **connectés** qui annoncent GAIA, sans filtre de nom ; choix dans `transport::choice` : casque mémorisé (`casque.txt`, empreinte FNV-1a de l'identifiant, sans adresse), sinon seul candidat, sinon seul nom distinctif, jamais au hasard ; `ctl --device` reste un choix explicite ; tests `a_renamed_headset_is_still_found`, `several_candidates_are_never_picked_at_random` ; essai Bazzite 22/10 : casque trouvé, empreinte mémorisée, journal sans nom ni adresse |
| F05 | Multipoint ciblé par position et nom | `service.rs` : `ToggleConnection { index, name }` vérifie seulement le nom | Confirmé | — |
| F06 | Commandes en attente non liées à une session | `worker.rs` : le canal survit à la reconnexion ; les commandes restées dans la file partent à la session suivante | Corrigé | Numéro de connexion sur chaque demande ; demandes d'une connexion précédente, en attente pendant une coupure ou remplacées : annulées sans envoi (`Cancelled`) ; demande coupée après l'envoi : `Unknown`, jamais renvoyée ; tests `a_lost_session_gives_unknown_then_cancelled_results`, `requests_waiting_for_a_lost_connection_are_cancelled`, `a_request_for_a_previous_connection_is_not_sent` |
| F07 | Snapshot gardant des données de l'ancienne session | `Snapshot::apply` : `Searching`/`Connected` gardent égaliseur, informations, connexions | Corrigé | `Snapshot` : `Searching`, `Connected` et `Lost` oublient égaliseur, informations et appareils ; test `a_new_connection_forgets_the_old_headset` |
| F08 | Installateurs sans préparation ni retour arrière | `installer.ps1` arrête l'app avant de vérifier les fichiers ; `installer-linux.sh` ignore un binaire absent | Confirmé | — |
| F09 | Publication visible avant le paquet Linux | `release.yml` : la publication est créée par le job Windows, Linux et Flatpak ajoutent ensuite | Confirmé | — |
| F10 | Nom du casque dans les journaux | `worker.rs` et `service.rs` : `info!(%name, …)` | En partie | Nom retiré des messages `info` du worker et du service (PR des demandes suivies) ; reste : niveaux `debug`/`trace` (trames brutes) |
| F11 | Fréquences de l'égaliseur et fréquence d'échantillonnage sans provenance | Fréquences : constante du profil M5 (le README ne dit plus qu'elles sont lues) ; `keep_stream_info` garde une ancienne fréquence | En partie | — |
| F12 | Documentation contradictoire | `docs/ROADMAP.md` ouvre encore sur la v0.6.1 ; `docs/LINUX.md` garde une note « aucun réglage écrit sous Linux » | Confirmé | — |
| F13 | Peu de tests à la frontière (CLI, IPC, interfaces) | Aucun test du CLI ni de WinUI ; pas de faux service multiclient | Confirmé | — |
| F14 | Cas d'usage dans les interfaces ; fichiers partagés sans contrôle de concurrence | `write_user_file` : nom temporaire fixe, pas de révision | Confirmé | — |

## Ordre de traitement

1. Sûreté des commandes : F02, puis F01 + F03 + F06 + F07 (identifiant de
   demande, génération de session, résultat exact, snapshot cohérent).
2. Identité : F04 (casque retrouvé sans dépendre de son nom), F05 (cible
   multipoint opaque).
3. Confidentialité des journaux : F10.
4. Publication et installation : F09, F08.
5. Documentation et tests de frontière : F12, F13, puis F11 et F14.
