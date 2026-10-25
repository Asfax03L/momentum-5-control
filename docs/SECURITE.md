# Sécurité du service

Le service `momentum-daemon` est le seul programme qui parle au casque.
Toutes les interfaces passent par lui. Ce document décrit ce qu'il peut
faire sur le système, et ce qui le limite.

## Ce dont le service a besoin

| Besoin | Linux | Windows |
|---|---|---|
| Recevoir les demandes des interfaces | bus de session D-Bus | canal nommé `\\.\pipe\openheadset-control-<SID>` |
| Joindre le casque | bus système (BlueZ), canal RFCOMM transmis par BlueZ | API Bluetooth de WinRT |
| Garder ses fichiers (historique de la batterie, casque mémorisé, scènes) | `~/.local/share/momentum-ctl-win` | `%LOCALAPPDATA%\momentum-ctl-win` |
| Exporter le profil auditif | `~/Documents` | `Documents` de l'utilisateur |
| Notifications | bus de session | notifications de Windows |

Rien d'autre : ni réseau, ni privilèges. Sous Linux, depuis la v0.15.1, le
reste du dossier personnel est masqué (ni lu ni écrit) ; sous Windows, le
service garde les droits ordinaires de l'utilisateur (voir plus bas).

## Linux : unité systemd durcie

`assets/linux/momentum-daemon.service`, la même pour `installer-linux.sh` et
les paquets `.deb` et `.rpm` (seul le chemin de l'exécutable change) :

- **Fichiers** : `ProtectSystem=strict` (système en lecture seule) et, depuis
  la v0.15.1, `ProtectHome=tmpfs` : le dossier personnel est remplacé par un
  dossier vide, où seuls apparaissent le dossier des données et
  `~/Documents` (lecture et écriture, `BindPaths`), le socket du bus de
  session (`%t/bus`) et le binaire installé par script (lecture seule).
  Le dossier des données est créé avant le démarrage, hors du bac à sable
  (`ExecStartPre=+`). `PrivateTmp`, `PrivateDevices`, `UMask=0077`.
- **Correction (audit du 24/10/2026, A02)** : de la v0.11.0 à la v0.15.0,
  l'unité utilisait `ProtectHome=read-only`. Cette option interdit
  l'**écriture** dans le dossier personnel, **pas la lecture** : un service
  compromis aurait pu lire les autres fichiers de l'utilisateur. Les notes
  de la v0.11.0, qui affirmaient le contraire, étaient fausses sur ce
  point. La v0.15.1 masque réellement ces fichiers.
- **Réseau** : `RestrictAddressFamilies=AF_UNIX AF_BLUETOOTH` (D-Bus et
  Bluetooth seulement, aucun accès IP).
- **Privilèges** : `NoNewPrivileges`, aucune capacité
  (`CapabilityBoundingSet=` vide), `RestrictSUIDSGID`, `RestrictNamespaces`,
  `LockPersonality`, `MemoryDenyWriteExecute`, `RestrictRealtime`.
- **Noyau** : `ProtectKernelTunables`, `ProtectKernelModules`,
  `ProtectKernelLogs`, `ProtectControlGroups`, `ProtectClock`,
  `ProtectHostname`, `ProtectProc=invisible`, `ProcSubset=pid`.
- **Appels système** : `SystemCallArchitectures=native`,
  `SystemCallFilter=@system-service` sans `@privileged` ni `@resources`.

Score de `systemd-analyze --user security` : **9,4 (« UNSAFE ») avant,
1,5 (« OK ») après** (systemd 259, Bazzite, 23/10/2026). Ce score est un
contrôle **statique** de l'unité. La CI (tâche `service-durci`) le recalcule
à chaque PR (seuil 2,5), puis fait un contrôle **à l'exécution** (A06) : le
service est lancé par son unité, sous le gestionnaire systemd de
l'utilisateur, répond sur le bus, et `scripts/essai-confinement.sh`
vérifie avec les mêmes options :
- un fichier personnel est illisible et non inscriptible ;
- le dossier des données est lisible et inscriptible, Documents est
  inscriptible ;
- `/etc` n'est pas inscriptible ;
- les deux bus sont joignables.

Essayé sur le casque (M5, 6.21.5, Bazzite) avec l'unité durcie :
connexion, lecture de l'état, anti-vent coupé puis remis (relu), le
23/10/2026 avec `ProtectHome=read-only`, puis le 25/10/2026 avec
`ProtectHome=tmpfs`. Le 25/10, `scripts/essai-confinement.sh` a vérifié
sous les mêmes options :
- fichier personnel illisible et non inscriptible ;
- dossier des données lisible et inscriptible, Documents inscriptible ;
- `/etc` non inscriptible ;
- bus de session et bus système joignables.

Conséquences pour l'utilisateur :

- l'export du profil auditif (`ctl exporter-profil`, bouton des interfaces)
  doit viser `~/Documents` (dossier par défaut des interfaces) ; un autre
  dossier est refusé par le système ;
- un `XDG_DATA_HOME` personnalisé n'est pas suivi par l'unité, qui autorise
  `~/.local/share/momentum-ctl-win` : lancer alors le service sans systemd ;
- le Flatpak n'utilise pas cette unité : le bac à sable de Flatpak limite
  déjà le service (permissions de `packaging/flatpak`, revues au
  jalon 0.13).

## Windows : canal nommé et droits du service

Déjà en place :

- le canal nommé porte le SID de l'utilisateur dans son nom, n'accepte que
  cet utilisateur (`D:P(A;;GA;;;<SID>)`), refuse les clients distants
  (`PIPE_REJECT_REMOTE_CLIENTS`) et ne peut pas être créé en double
  (`FILE_FLAG_FIRST_PIPE_INSTANCE`) ;
- le service tourne avec les droits de l'utilisateur, sans élévation.

### Étude : AppContainer ou niveau d'intégrité bas

Jalon 0.11 : étude seulement, rien n'est changé. Aucune des deux pistes n'a
encore été essayée ; ce qui suit est **à vérifier sur le matériel**.

| | Niveau d'intégrité bas | AppContainer |
|---|---|---|
| Lancement | jeton abaissé par l'interface qui démarre le service | profil d'AppContainer et liste de capacités |
| Fichiers | écriture seulement là où l'étiquette « bas » est permise (`%LOCALAPPDATA%Low`) : déplacer le dossier des données, ou l'étiqueter | écriture seulement dans le dossier du conteneur : déplacer le dossier des données |
| Canal nommé | les clients (intégrité moyenne) peuvent s'y connecter ; ACL à revoir | ACL du canal à ouvrir au SID du conteneur |
| Bluetooth (WinRT) | à vérifier : l'accès aux API Bluetooth de WinRT à intégrité basse n'est pas documenté | capacité `bluetooth` ; à vérifier hors d'un paquet MSIX |
| Notifications | à vérifier | à vérifier |
| Effort | moyen | élevé (sauf en MSIX, où le conteneur vient avec le paquet) |

**Proposition** : ne rien changer avant la décision sur la signature. Un
paquet MSIX (feuille de route, « Après la signature ») donne l'AppContainer
et la capacité `bluetooth` par sa déclaration, sans code de lancement
particulier ; c'est la voie la plus simple pour réduire les droits du
service sous Windows. D'ici là, le canal nommé reste limité à
l'utilisateur, et le service n'a aucun privilège.

## Signaler une faille

Par un avis de sécurité privé sur GitHub (onglet « Security » du dépôt),
pas par une issue publique.
