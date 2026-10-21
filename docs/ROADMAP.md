# Roadmap : une vraie interface PC pour le MOMENTUM 5

Objectif : une application Windows complète, équivalente à Smart Control Plus
pour tout ce qui passe par le casque, mais pensée pour un PC portable (souris,
clavier, grand écran, barre système). L'icône reste le raccourci vers la
fenêtre principale.

**État actuel (v0.6.1, 12/10/2026)** : phases A à F et étapes 1 à 4 faites, corrections de l'audit indépendant de la v0.5.0 (étape 4 : suivi de tête et Multipoint ajoutés ; sidetone, langue et priorité en lecture seule ; USB relevé). La fenêtre couvre
tout ce que l'app fait sur le casque et qui a été validé ; le reste est en
lecture seule ou absent, avec la raison ci-dessous. Le détail du protocole
est dans [`MOMENTUM5.md`](MOMENTUM5.md), l'historique daté en fin de page.

## Principes

- **Un service, des interfaces natives** (depuis la v0.8.0) : le service
  `momentum-daemon` (thread `worker`) est seul propriétaire de la connexion
  GAIA ; WinUI 3 sous Windows et Kirigami sous KDE en sont clientes, comme
  `ctl`. L'icône Slint des versions 0.7 et avant est retirée. Aucune
  réécriture sans nécessité démontrée.
- **Rien d'inventé** : chaque écriture provient d'une capture de l'app puis
  d'une validation sur le casque. Ce qui n'est pas validé s'affiche en
  lecture seule, ou n'apparaît pas.
- **Jamais** : mise à jour du firmware ni installation d'un pack de voix,
  réinitialisation d'usine, nouveau test auditif (algorithme propriétaire de
  l'app), suppression d'un appairage.
- **Session GAIA fiable** : après un échange interrompu (délai dépassé, envoi
  ou lecture en erreur), la session est abandonnée ; aucune écriture n'est
  renvoyée automatiquement. Le worker se reconnecte et relit l'état.
- **Données privées** : ni adresse Bluetooth, ni numéro de série, ni profil
  auditif dans l'interface, le journal ou le diagnostic à partager. Seul
  l'export du profil (« Mon son »), demandé explicitement, contient le
  profil auditif : c'est un fichier privé, à ne pas joindre à un rapport.
- **Adapté au portable** : fenêtre redimensionnable, navigation latérale,
  tout accessible au clavier, thème clair ou sombre selon Windows, mise à
  l'échelle 100 à 200 %.

## Correspondance actuelle avec l'app Android

Statuts : ✅ écrit et validé sur le M5 (firmware 6.21.5) ; 👁 lu seulement ;
⛔ exclu volontairement ; ➖ propre au téléphone.

| Smart Control Plus | Application PC | Statut |
|---|---|---|
| Mon appareil : nom, batterie, Bluetooth | En-tête : nom lu sur le casque, batterie, codec et fréquence | 👁 |
| Contrôle du bruit : Adaptatif / Personnalisé / ARRÊT, curseur, anti-vent | Accueil | ✅ |
| Spatial audio (Dolby Atmos) : marche / arrêt, suivi de tête | Accueil | ✅ (`3604`, `3602`) |
| Commandes (tactiles) | Accueil | ✅ |
| Gestion des connexions : connecter, déconnecter | Accueil : appareils appairés, boutons (sauf ce PC) | ✅ (`140F` / `1410`) |
| Gestion des connexions : Multipoint | Accueil : interrupteur (confirmation avant la coupure) | ✅ (`001D:0E00`) |
| Gestion des connexions : suppression d'un appairage | — | ⛔ |
| My sound : égaliseur, préréglages, basses | Mon son | ✅ (préréglages : courbes côté PC) |
| My sound : Speech Clarity, sélection du profil personnalisé | Mon son | ✅ |
| Personnalisation du son : export du profil | Mon son (fichier dans Documents) | 👁 (lecture brute) |
| Personnalisation du son : nouveau test, restauration d'un profil | — | ⛔ |
| Smart Pause, réponse auto, appels confortables | Réglages | ✅ |
| Mise hors tension automatique | Réglages | ✅ |
| Invites : mode, volume | Réglages | ✅ |
| Invites : langue | Réglages | 👁 (changer = installer un pack de voix ⛔) |
| Mode de protection de la batterie | Réglages | ✅ |
| Donner un nom au casque | Réglages | ✅ (ASCII, 32 caractères au plus) |
| Priorité du mode audio | Réglages | 👁 (redémarre le casque ; l'app du téléphone reste souvent bloquée) |
| Codec, micrologiciel, informations produit | À propos | 👁 |
| Mise à jour du micrologiciel, réinitialisation d'usine | — | ⛔ |
| Zones sonores, Retrouver le casque, Nouvelles | — | ➖ |

## Prochaines étapes (proposées, à confirmer une par une)

1. **Fiabilité et documentation** : session abandonnée après un envoi ambigu
   (fait, publié en v0.4.0), roadmap à jour, notes de version v0.3.0 détaillées
   (`CHANGELOG.md`).
2. **Scènes et `ctl` pendant que l'icône tourne** (fait, publié en v0.4.0 ; plan
   dans [`PLAN-SCENES-IPC.md`](PLAN-SCENES-IPC.md)) : scènes nommées (Travail,
   Transport, Jeu…) composées uniquement de commandes validées, appliquées
   une par une avec relecture et affichage d'un résultat partiel ; canal
   local (named pipe limité à l'utilisateur) entre `ctl` et le worker, avec
   une liste fermée d'actions, pour garder un seul propriétaire de RFCOMM.
   Exclus des scènes : priorité audio, langue, profil auditif, connexions
   sans choix explicite.
3. **Confort** (fait, publié en v0.4.0 : historique de la batterie, diagnostic,
   saisie et fichiers de courbes, A/B, raccourcis éditables, panneau rapide) :
   historique local de la batterie (sans « santé » ni nombre de
   cycles inventés), panneau rapide au clic sur l'icône, saisie numérique et
   import/export des courbes d'égaliseur, raccourcis éditables, bouton
   « Copier un diagnostic » expurgé.
4. **Recherche séparée** (faite, publiée en v0.5.0, voir [`RECHERCHE-ETAPE4.md`](RECHERCHE-ETAPE4.md) ;
   lecture d'abord, écriture seulement après capture
   et accord) : intensité du sidetone, suivi de tête, audio USB, paramètres
   du profil de personnalisation. Reste non qualifié : les notifications de
   la détection de port (aucun essai dédié).
5. **Distribution** : signature des exécutables (SmartScreen), CONTRIBUTING
   et formulaires d'issues, matrice de validation modèle/firmware.
6. **Plusieurs interfaces et Linux** (fait en v0.7.0, sauf mention) :
   espace de travail (cœur, service, outils, interfaces), `ctl` et icône
   Slint sous Linux (BlueZ, KDE Plasma, Wayland), interface OpenHeadset
   Control (thèmes, disposition adaptative, démonstration), client Qt 6 /
   Kirigami (non publié). Reste : panneau rapide, raccourcis globaux et
   sorties audio sous Linux ; canal local (D-Bus) pour qu'une interface et
   `ctl` partagent la connexion ; archive Linux dans les publications ;
   limiter aussi les écritures au micrologiciel validé (6.21.5) ; essai
   d'une interface WinUI 3.
7. **Séparer le service des interfaces** (fait, publié en v0.8.0 ;
   détail et étapes dans [ARCHITECTURE.md](ARCHITECTURE.md)) : `momentum-daemon` seul
   propriétaire de la connexion, API D-Bus (Linux) ou canal nommé
   (Windows), interfaces natives clientes (Kirigami + icône SNI sous KDE,
   WinUI 3 sous Windows), tout en Rust ; puis retrait de l'icône Slint,
   après reprise de toutes ses fonctions par les deux interfaces.
8. **Migration vers zbus 6** (plus tard, quand elle apporte un gain
   concret) : zbus 6.0 (16/10/2026) supprime l'API bloquante et les proxys
   bloquants utilisés par `momentum-daemon` et `momentum-client`, et
   remplace `async-io` par son propre moteur (`zruntime`). La migration
   demande de passer le service et le client en asynchrone ; d'ici là, le
   projet reste sur zbus 5.19. Guide amont :
   « Upgrading to zbus 6.0 ».

## Historique

Chaque phase s'est terminée par : tests (`cargo test`, fmt, clippy), essai
sur le M5, captures d'écran de la fenêtre, PR vers `main` après CI verte.

### Phase A : socle de la fenêtre principale — ✅ 09/10/2026 (PR #6)
- Fenêtre `MainWindow` Slint à navigation latérale (Accueil, Mon son,
  Réglages, À propos), à la place de la fenêtre d'égaliseur seule.
- Modèle de vue unique en Rust alimenté par le worker.
- Écrans « recherche du casque », « déconnecté », « lecture seule (modèle
  non validé) » ; clic sur l'icône pour ouvrir la fenêtre.

### Phase B : Accueil — ✅ 09/10/2026 (PR #7)
- Carte du casque, contrôle du bruit, audio spatial, commandes tactiles.
- Appareils appairés au casque, d'abord en lecture seule (`1400`/`1401`),
  remplacés en F6 par la liste avec actions (`140D`/`140E`).

### Phase C : Mon son — ✅ 09/10/2026 (PR #8)
- Égaliseur intégré, basses, remise à plat, préréglages (Neutre et
  Audiophile relevés dans l'app, plus les préréglages personnels).
- Choix du mode audio et export du profil.

### Phase D : Réglages et À propos — ✅ 09/10/2026 (PR #9)
- Smart Pause, réponse automatique, appels confortables, extinction
  automatique, invites, protection de la batterie.
- Langue, volume des invites et priorité affichés en lecture seule en
  attendant la phase F.

### Phase E : intégration au PC portable — ✅ 09/10/2026 (PR #10, v0.2.0)
- Raccourcis clavier globaux, notifications Windows, archive publiée par la
  CI avec empreinte SHA-256, installateur par utilisateur.
- Validé sur le PC : Ctrl+Alt+M, notification de test, archive v0.2.0. À
  vérifier en usage réel : Ctrl+Alt+N, Ctrl+Alt+P, alertes de batterie et
  de connexion.

### Phase F : expériences sur le casque — ✅ 09/10/2026 (PR #11 à #14, v0.3.0)

Méthode : journal HCI du téléphone, appuis dans l'app horodatés par ADB,
relecture depuis le PC, puis restauration de la valeur initiale. Une
expérience à la fois, avec accord.

| Expérience | Résultat |
|---|---|
| F1 Volume des invites | ✅ échelle relevée (−20 à +6, 0 = 77 %), écriture validée, curseur dans Réglages |
| F2 Persistance du profil | ✅ identique et toujours sélectionné après extinction complète |
| F3 Nom du casque | ✅ `2801` capturé, aller-retour validé depuis le PC, champ dans Réglages |
| F4 Langue des invites | 👁 changer de langue installe un pack de voix : arrêté avant de confirmer, aucune trame envoyée |
| F5 Priorité du mode audio | 👁 le PC se reconnecte seul, mais l'app du téléphone reste bloquée deux fois sur trois |
| F6 Gestion des connexions | ✅ `140F` / `1410` capturés et validés (`ctl`, puis boutons de l'Accueil) ; Multipoint et suppression non touchés |

Avant la phase F : sélection du profil personnalisé (mode 3), capturée et
validée le 09/10/2026 (PR #4).

