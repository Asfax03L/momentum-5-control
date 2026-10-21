# Architecture d'OpenHeadset Control

État au 20/10/2026 et direction choisie : **séparer le back (un service qui
tient le casque) des fronts (interfaces natives de chaque système)**, tout
en Rust.

## Aujourd'hui (v0.7.0)

```
crates/momentum-core      protocole GAIA, casque, scènes, modèles d'affichage
crates/momentum-service   worker (connexion), intégration système
crates/momentum-cli       ctl, probe
frontends/slint           icône + fenêtre (Windows, Linux) ← contient le worker
frontends/kirigami        client KDE (Linux)               ← contient le worker
```

Chaque interface **embarque** le worker : une seule à la fois peut tenir la
connexion au casque (il faut quitter l'icône pour lancer `ctl` ou le client
Kirigami), et chaque interface doit refaire l'intégration système
(notifications, démarrage automatique…).

## Cible : un service, des interfaces clientes

```
                         ┌────────────────────────────────────┐
                         │  momentum-daemon (Rust, sans fenêtre)│
                         │  worker : seul à parler au casque    │
                         │  scènes, historique batterie, alertes│
                         └───────────────┬────────────────────┘
          API locale : Linux → D-Bus (bus de session, zbus, Rust pur)
                       Windows → canal nommé (JSON, utilisateur courant)
        ┌───────────────┬───────────────┼────────────────┬──────────────┐
  momentum-kirigami  momentum-winui   momentum-tray     ctl        (widget Plasma,
  KDE : Kirigami,    Windows :        Slint, de         ligne de    autres clients
  icône SNI (ksni),  WinUI 3 par      secours sur les   commande    D-Bus)
  KGlobalAccel       windows-reactor, deux systèmes
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
  un script. Sous Windows, le canal nommé existant (déjà utilisé par `ctl`)
  est étendu aux événements.
- **Les règles de sécurité restent dans le service** : liste fermée de
  commandes validées, garde du modèle, relecture après écriture. Une
  interface ne peut rien envoyer d'autre.
- **Les modèles d'affichage restent dans `momentum-core`** : chaque
  interface affiche les mêmes textes et applique les mêmes règles.

## Interfaces natives

| Système | Interface | Natif | Langages |
|---|---|---|---|
| KDE Plasma (Bazzite) | `momentum-kirigami` | Kirigami/Breeze, thème Plasma, icône de zone de notification StatusNotifierItem (`ksni`), notifications D-Bus, raccourcis globaux KGlobalAccel (D-Bus), lancement XDG, Wayland natif | Rust ; écran décrit en QML (seul moyen d'utiliser Kirigami) ; cale C++ minimale pour l'icône (absente de cxx-qt) |
| Windows 11 | `momentum-winui` | WinUI 3 (`windows-reactor`, Microsoft), NavigationView, fond Mica, thème du système, toasts, icône de zone de notification `Shell_NotifyIcon`, raccourcis `RegisterHotKey`, lancement au démarrage | 100 % Rust (Windows App Runtime 2.x requis) |
| Les deux | `momentum-tray` (Slint) | interface commune de secours, jusqu'à ce que les deux interfaces natives couvrent tout | Rust + Slint |

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
| `Command(s)` | une `momentum_client::Command` en JSON (liste fermée des réglages validés ; refusée si le casque est absent ou en lecture seule) |
| `Snapshot() → s` | `momentum_client::Snapshot` en JSON (nom, état, égaliseur, informations, appareils, dernier incident) |
| signal `Event(s)` | chaque `momentum_client::Event` en JSON |
| propriétés | `Connected` (b), `Battery` (i, -1 inconnue), `Name` (s), avec `PropertiesChanged` |

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
| `Command(…)` | `Ok`, ou `Error` avec la raison (mêmes refus que sous Linux) |

Les interfaces relèvent les événements toutes les 150 ms ; le client
(`momentum_client::Client`) offre la même interface que celui de Linux.
2. **KDE** — 🟡 en partie (21/10/2026) : `momentum-kirigami` est client du
   service (D-Bus, reconnexion automatique) ; icône de la zone de
   notification (`ksni`, StatusNotifierItem) au dessin du logo, teintée
   selon le mode, avec infobulle et menu (ouvrir, contrôle du bruit,
   quitter) ; fermer la fenêtre la cache ; raccourcis globaux KGlobalAccel
   (Ctrl+Alt+N, P, M, modifiables dans Configuration du système >
   Raccourcis). Puis : appareils appairés (connecter, déconnecter avec
   confirmation), scènes, courbe de la batterie, diagnostic expurgé,
   lancement à l'ouverture de session (icône seule), instance unique
   (D-Bus `org.openheadset.ControlWindow`). Reste : sorties audio du
   système (PipeWire), édition des scènes dans la fenêtre.
3. **Windows** — 🟡 en partie (21/10/2026) : service sur canal nommé ;
   `momentum-winui` (`windows-reactor` 0.100, chaîne GNU suffisante),
   client du service : NavigationView, Mica, pages Accueil (bruit,
   fonctions rapides, appareils), Égaliseur, Paramètres, À propos ; icône
   de la zone de notification (logo teinté, infobulle, menu), fermeture
   qui cache la fenêtre, instance unique, raccourcis globaux
   (`raccourcis.txt`), lancement à l'ouverture de session (`--cache`),
   service lancé au besoin ; pages Scènes (application et compte rendu),
   Batterie (courbe 24 h / 7 j / 30 j), diagnostic à copier ; panneau
   rapide au clic sur l'icône. Fait (21/10/2026), sauf l'installation
   (étape 5).
4. **Slint** : client du service, ou retrait quand les deux interfaces
   natives couvrent tout.
5. **Distribution** — 🟡 (21/10/2026) : `installer.ps1` installe WinUI,
   le service et `ctl` par défaut (ou l'icône Slint avec `-Interface
   slint`), vérifie le Windows App Runtime 2, inscrit le démarrage caché ;
   `installer-linux.sh` fonctionne aussi depuis l'archive ; la publication
   joint une archive Windows et une archive Linux. Reste : première
   publication (la CI GitHub est bloquée par la facturation du compte).
