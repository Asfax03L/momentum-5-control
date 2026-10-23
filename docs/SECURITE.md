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

Rien d'autre : ni réseau, ni privilèges, ni accès aux fichiers du reste du
dossier personnel.

## Linux : unité systemd durcie

`assets/linux/momentum-daemon.service`, la même pour `installer-linux.sh` et
les paquets `.deb` et `.rpm` (seul le chemin de l'exécutable change) :

- **Fichiers** : `ProtectSystem=strict` et `ProtectHome=read-only` ; seuls le
  dossier des données et `~/Documents` sont accessibles en écriture
  (`ReadWritePaths`). Le dossier des données est créé avant le démarrage, hors
  du bac à sable (`ExecStartPre=+`). `PrivateTmp`, `PrivateDevices`,
  `UMask=0077`.
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
1,5 (« OK ») après** (systemd 259, Bazzite, 23/10/2026). La CI recalcule le
score à chaque PR (tâche `service-durci`, seuil 2,5) et démarre le service
compilé, sans casque, sur un bus privé.

Essayé sur le casque (M5, 6.21.5, Bazzite, 23/10/2026) avec l'unité
durcie : connexion, lecture de l'état, anti-vent coupé puis remis (relu).
Écriture vérifiée sous les mêmes options : possible dans le dossier des
données et dans `~/Documents`, refusée dans le reste du dossier personnel
et dans `/etc`.

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
