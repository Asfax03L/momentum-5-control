# Architecture d'OpenHeadset Control

Direction choisie le 20/10/2026, en place depuis la v0.8.0 : **séparer le
back (un service qui tient le casque) des fronts (interfaces natives de
chaque système)**, tout en Rust.

## Avant (v0.7.0)

Chaque interface (icône Slint, client KDE) **embarquait** le worker : une
seule à la fois pouvait tenir la connexion au casque (il fallait quitter
l'icône pour lancer `ctl` ou le client Kirigami), et chaque interface
devait refaire l'intégration système (notifications, démarrage
automatique…).

## Depuis la v0.8.0 : un service, des interfaces clientes

```
crates/momentum-core      protocole GAIA, casque, scènes, modèles d'affichage
crates/momentum-service   worker (connexion), intégration système
crates/momentum-client    API du service (messages, clients D-Bus et canal nommé)
crates/momentum-daemon    le service
crates/momentum-cli       ctl, probe
frontends/winui           interface Windows (WinUI 3)
frontends/kirigami        interface KDE (Qt 6 / Kirigami)
```

```
                         ┌────────────────────────────────────┐
                         │  momentum-daemon (Rust, sans fenêtre)│
                         │  worker : seul à parler au casque    │
                         │  scènes, historique batterie, alertes│
                         └───────────────┬────────────────────┘
          API locale : Linux → D-Bus (bus de session, zbus, Rust pur)
                       Windows → canal nommé (JSON, utilisateur courant)
        ┌───────────────┬───────────────┼────────────────┐
  momentum-kirigami  momentum-winui      ctl         (widget Plasma,
  KDE : Kirigami,    Windows :           ligne de     autres clients
  icône SNI (zbus),  WinUI 3 par         commande     D-Bus)
  KGlobalAccel       windows-reactor,
                     Mica, toasts,
                     Shell_NotifyIcon
```

- **Un seul propriétaire de la connexion** : le service. Les interfaces
  peuvent tourner en même temps (fenêtre, icône, `ctl`) sans conflit.
- **Une API, deux transports** : les mêmes messages que le worker actuel
  (`WorkerCommand` → service, `WorkerEvent` → interfaces), dans une crate
  partagée `momentum-client` qui cache le transport. Sous Linux, D-Bus est
  le transport natif de KDE : propriétés, méthodes et signaux
  (`org.openheadset.Control1`), utilisables aussi par un widget Plasma ou
  un script. Sous Windows, un canal nommé propre à l'utilisateur porte les
  mêmes messages et les événements.
- **Les règles de sécurité restent dans le service** : liste fermée de
  commandes validées, garde du modèle, relecture après écriture. Une
  interface ne peut rien envoyer d'autre.
- **Les modèles d'affichage restent dans `momentum-core`** : chaque
  interface affiche les mêmes textes et applique les mêmes règles.

## Interfaces natives

| Système | Interface | Natif | Langages |
|---|---|---|---|
| KDE Plasma (Bazzite) | `momentum-kirigami` | Kirigami/Breeze, thème Plasma, icône de zone de notification StatusNotifierItem et DBusMenu (zbus), notifications D-Bus, raccourcis globaux KGlobalAccel (D-Bus), lancement XDG, Wayland natif | Rust ; écran décrit en QML (seul moyen d'utiliser Kirigami) ; cale C++ minimale pour l'icône (absente de cxx-qt) |
| Windows 11 | `momentum-winui` | WinUI 3 (`windows-reactor`, Microsoft), NavigationView, fond Mica, thème du système, toasts, icône de zone de notification `Shell_NotifyIcon`, raccourcis `RegisterHotKey`, lancement au démarrage | 100 % Rust (Windows App Runtime 2.x requis) |

## Étapes (une PR chacune, validée sur le casque)

1. **API et service** — ✅ Linux (20/10/2026) : crate `momentum-client`
   (messages `Command`/`Event`/`Snapshot` en JSON, client D-Bus `zbus`),
   `momentum-daemon` (worker, historique de la batterie, notifications,
   D-Bus), `ctl` passe par le service s'il tourne. Lancement : unité
   systemd utilisateur et **activation D-Bus** (le service démarre au
   premier appel). Windows (21/10/2026) : même service sur un canal
   nommé de l'utilisateur, `ctl` passe aussi par lui.

### API D-Bus (Linux)

| Élément | Contenu |
|---|---|
| Nom, objet | `org.openheadset.Control`, `/org/openheadset/Control` |
| Interface | `org.openheadset.Control1` |
| `Submit(s, t) → t` | une `momentum_client::Command` en JSON et le numéro de connexion vu par l'interface (`Snapshot.session`, 0 : la connexion courante) ; rend le **numéro de la demande**. Refusée tout de suite (erreur D-Bus, rien n'est envoyé) si le casque est absent, en lecture seule, ou si la connexion a changé |
| `Command(s)` | même chose sans numéro (ancienne méthode, gardée pour les scripts) |
| `Snapshot() → s` | `momentum_client::Snapshot` en JSON (nom, état, égaliseur, informations, appareils, dernier incident) |
| `Scenes() → s` | `{"names": [...], "panel": [...]}` : scènes de `scenes.txt` et celles choisies pour le panneau rapide (`panneau.txt`) ; utilisée par le widget Plasma |
| signal `Event(s)` | chaque `momentum_client::Event` en JSON ; une demande se termine par **un seul** `Done { id, outcome, details }` ; `NotFound(raison)` après chaque recherche du casque sans résultat |
| propriétés | `Connected` (b), `Battery` (i, -1 inconnue), `Name` (s), avec `PropertiesChanged` |

**Résultat d'une demande** (`momentum_core::request::Outcome`, revue F01,
F03, F06, F07) : `Applied` (acquittée puis relue avec la valeur
demandée), `AlreadySatisfied`, `Acknowledged` (acquittée, effet suivi par
les notifications : connexion d'un appareil), `Refused`, `NotApplied`
(relue autrement), `Cancelled` (jamais envoyée : remplacée par une demande
plus récente, connexion changée ou perdue, casque absent) et `Unknown`
(connexion perdue après l'envoi : jamais renvoyée automatiquement). Chaque
connexion au casque a un numéro (`Connected { session }`) : une demande
faite pour une connexion précédente n'est pas envoyée, et le `Snapshot`
oublie l'égaliseur, les informations et les appareils de l'ancienne
connexion. Les interfaces suivent leurs demandes avec
`momentum_client::tracker::Tracker`.

Les adresses Bluetooth des appareils appairés ne sortent jamais du
service : une interface désigne un appareil par sa position et son nom.

Essai : `busctl --user get-property org.openheadset.Control
/org/openheadset/Control org.openheadset.Control1 Battery`.

### API par canal nommé (Windows)

`\\.\pipe\openheadset-control-<SID>` : propre à l'utilisateur (SID dans
le nom, accès réservé, clients distants refusés, première instance exigée).
Une requête JSON par connexion (`momentum_client::wire`) :

| Requête | Réponse |
|---|---|
| `Snapshot` | état courant et numéro du dernier événement |
| `Events { after }` | événements numérotés après `after` (`None` : numéro courant) ; `reset` si l'interface a trop de retard (512 gardés) |
| `Submit { command, session }` | `Submitted { id }`, ou `Error` avec la raison (mêmes refus que sous Linux) ; résultat dans l'événement `Done { id, … }` |
| `Command(…)` | `Ok`, ou `Error` (sans numéro, gardée pour compatibilité) |

Les interfaces relèvent les événements toutes les 150 ms ; le client
(`momentum_client::Client`) offre la même interface que celui de Linux.
2. **KDE** — 🟡 en partie (21/10/2026) : `momentum-kirigami` est client du
   service (D-Bus, reconnexion automatique) ; icône de la zone de
   notification (StatusNotifierItem et DBusMenu sur zbus) au dessin du
   logo, teintée selon le mode, avec infobulle et menu (ouvrir, contrôle du bruit,
   quitter) ; fermer la fenêtre la cache ; raccourcis globaux KGlobalAccel
   (Ctrl+Alt+N, P, M, modifiables dans Configuration du système >
   Raccourcis). Puis : appareils appairés (connecter, déconnecter avec
   confirmation), scènes, courbe de la batterie, diagnostic expurgé,
   lancement à l'ouverture de session (icône seule), instance unique
   (D-Bus `org.openheadset.ControlWindow`), éditeur de scènes, thème
   clair ou sombre, mode démonstration. Sorties du système : instantané
   PipeWire lu par `pw-dump`, sur un fil séparé et avec délai/taille bornés.
   Le service transmet l'empreinte de son identifiant de connexion dans
   `Connected.headset_id` et `Snapshot.headset_id` (facultatif, compatible
   avec les anciennes versions). Le client compare cette empreinte aux
   adresses BlueZ du graphe ; aucune adresse n'est ajoutée à l'API.
   L'empreinte est oubliée avec les autres données à la perte de connexion.
3. **Windows** — 🟡 en partie (21/10/2026) : service sur canal nommé ;
   `momentum-winui` (`windows-reactor` 0.100, chaîne GNU suffisante),
   client du service : NavigationView, Mica, pages Accueil (bruit,
   fonctions rapides, appareils), Égaliseur, Paramètres, À propos ; icône
   de la zone de notification (logo teinté, infobulle, menu), fermeture
   qui cache la fenêtre, instance unique, raccourcis globaux
   (`raccourcis.txt`), lancement à l'ouverture de session (`--cache`),
   service lancé au besoin ; pages Scènes (application et compte rendu),
   Batterie (courbe 24 h / 7 j / 30 j), diagnostic à copier ; panneau
   rapide au clic sur l'icône ; puis éditeur de scènes, sortie audio du
   système, thème clair ou sombre, mode démonstration. ✅ (21/10/2026)
4. **Slint** — ✅ retiré (v0.8.0) : ses fonctions ont été reprises par les
   deux interfaces natives ; `ctl` ne passe plus que par le service.
5. **Distribution** — ✅ (v0.8.0) : `installer.ps1` installe WinUI, le
   service et `ctl`, vérifie le Windows App Runtime 2, inscrit le
   démarrage caché ; `installer-linux.sh` fonctionne aussi depuis
   l'archive ; les deux retirent l'ancienne icône Slint ; la publication
   joint une archive Windows et une archive Linux.

## Reconnexion demandée au système

`Command::ReconnectHeadset` est une action du système, sans trame GAIA.
`Submit` lui attribue un numéro et publie exactement un `Done`. Elle est
indépendante de la session GAIA : elle reste disponible casque absent ou
en lecture seule, sans modifier les gardes des écritures existantes. Un
fil séparé et un verrou de demande empêchent de bloquer le service ou de
lancer plusieurs connexions simultanées. Aucune demande n'est rejouée.

Le service lit l'empreinte de `casque.txt` au moment de la demande. Sans
casque mémorisé, `Done` vaut `Refused` sans appeler le système. Linux lit
les objets appairés en cache sur tous les adaptateurs BlueZ (5 s maximum),
compare les empreintes et exige une correspondance unique. Il appelle
ensuite [`org.bluez.Device1.Connect`](https://github.com/bluez/bluez/blob/master/doc/org.bluez.Device.rst)
(20 s maximum). Ni découverte, association, déconnexion, changement de
profil ou de routage, ni UUID nouveau. Un échec après l'appel est
`Unknown` : un profil peut déjà avoir été connecté, le délai côté client
n'annule pas nécessairement l'opération dans BlueZ.

Windows initialise COM en STA sur le fil dédié, comme le recommande
[`ShellExecuteW`](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew),
puis l'utilise pour ouvrir l'URI documentée
[`ms-settings:bluetooth`](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings).
Cette solution demande une connexion manuelle dans les paramètres, plutôt
que d'assimiler une connexion RFCOMM à la connexion audio. L'ouverture est
signalée explicitement, sans prétendre que le casque est connecté.

Le nouveau résultat `Outcome::SystemRequested(message)` signifie que
l'action système a été exécutée, **sans confirmation GAIA ou audio**.
Les interfaces affichent son message seulement pour leur propre numéro de
demande (même si `Done` arrive avant `Submitted`). `ctl` attend jusqu'à
40 s pour cette action, sort avec 0 pour une demande système exécutée,
1 pour un refus, 3 pour un résultat inconnu. Aucun état du casque n'est
ajouté artificiellement au snapshot. Ce nouveau type de résultat demande
des clients mis à jour avec le service ; un service ancien refusera la
nouvelle commande et ce refus reste visible.

Tests : sélection pure avec Bluetooth simulé ; vrai adaptateur `bluer`
contre un faux BlueZ sur bus privé, dans un processus séparé ; faux service
D-Bus avec casque absent, résultats étrangers et refus ; transport GAIA
simulé laissé intact ; suivis concurrents et résultat arrivé en avance.
Les erreurs système sont remplacées par des messages fixes pour ne pas
publier d'adresse ou de nom dans les résultats.
