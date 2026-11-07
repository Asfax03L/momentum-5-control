# Compatibilité

Casques et micrologiciels essayés, et commandes validées sur le MOMENTUM 5.
Fiche complète du protocole : [MOMENTUM5.md](MOMENTUM5.md).


## Modèles et micrologiciels testés

| Casque (modèle lu) | Micrologiciel | Systèmes essayés | Écritures |
|---|---|---|---|
| MOMENTUM 5 Wireless (`M5AEBT Black`) | 6.21.5 | Windows 11 ; Bazzite 44 (KDE Plasma 6, Wayland) | ✅ validées (tableau ci-dessous) |
| MOMENTUM 5, autre micrologiciel | — | — | 🔒 lecture seule (non vérifié) |
| Autre casque GAIA (ACCENTUM, MOMENTUM 4…) | — | jamais essayé | 🔒 lecture seule |

Un autre modèle ou micrologiciel reste en lecture seule (aucun réglage
envoyé) tant que chaque écriture n'y a pas été vérifiée. Pour en signaler
un, ouvrez une issue « Compatibilité » avec la sortie de `probe --deep`
(voir [CONTRIBUTING.md](../CONTRIBUTING.md)).

## Détail des commandes

Relevé sur un M5 « M5AEBT Black », firmware 6.21.5, en octobre 2026.
Transport : Bluetooth Classic RFCOMM, service SDP « GAIA »
(UUID `A2129FF3-081B-4C45-8AFE-469D9C4842EC`). Windows n'a joint aucun
service GATT (ce qui ne prouve pas qu'il n'en existe aucun).

| Fonctionnalité | Commande | Validée sur M5 ? |
|---|---|---|
| Connexion RFCOMM GAIA (en parallèle du téléphone) | — | ✅ |
| Lire batterie, modèle, firmware, codec | `0603`, `1206`, `1201`, `0800` | ✅ |
| Lire le mode Adaptatif / Personnalisé / ARRÊT | `1A05` + `1A01` | ✅ |
| Lire le curseur ANC ↔ transparence | `1A03` | ✅ 0 = ANC max, 50 = neutre, 100 = transparence max |
| Lire anti-vent, audio spatial | `1A01` (paire 1), `3605` | ✅ |
| Lire l'égaliseur et l'Amplification des basses | `1000`, `1002 [bande]`, `1009` | ✅ 8 bandes, ±6 dB |
| Lire Smart Pause, port, appels, invites, extinction auto… | voir la fiche | ✅ (`probe --deep`) |
| Changer le mode | `1A04` + `1A00 [03 x]` | ✅ |
| Régler le curseur ANC ↔ transparence | `1A02 [0-100]` | ✅ |
| Activer / désactiver l'anti-vent | `1A00 [01 x]` | ✅ |
| Activer / désactiver l'audio spatial | `3604 [0/1]` | ✅ |
| Notifications (batterie, bruit, audio spatial, égaliseur) | `0495:0007 [feature]` | ✅ |
| Régler une bande de l'égaliseur | `1001 [bande, gain]` | ✅ |
| Activer / désactiver l'Amplification des basses | `1008 [0/1]` | ✅ |
| Speech Clarity (mode audio) | `0803 [00, 02]` ; égaliseur : `[00, 01]` | ✅ |
| Commandes tactiles | `1606 [1 = désactivées]`, lecture `1607` | ✅ |
| Protection de la batterie | `060A [0/1]`, lecture `060B` | ✅ |
| Smart Pause, réponse automatique, appels confortables | `080C`, `080A`, `0814` `[0/1]` | ✅ |
| Extinction automatique | `0600 [00, secondes u16]` | ✅ jamais / 15 / 30 / 60 min |
| Invites (tonalités / tonalités et voix) | `0801 [1/2]` | ✅ |
| Volume des invites | `081C [i8]` (−20 à +6), lecture `081D` | ✅ validé sur M5 (échelle relevée, `ctl volume-invites`) |
| Nom du casque | `2801 [nom]`, lecture `2802` | ✅ validé sur M5 (`ctl nom`, page Réglages ; ASCII, 32 caractères au plus) |
| Gestion des connexions : connecter, déconnecter un appareil appairé | `140F` / `1410 [adresse]`, liste `140D`/`140E`, ce PC `1413` | ✅ validé sur M5 (`ctl connexions`, Accueil) ; suppression d'un appairage et Multipoint non exposés |
| Suivi de tête (Dolby Atmos) | `3602 [0/1]`, lecture `3603` | ✅ validé sur M5 (`ctl suivi-tete`, audio spatial activé) |
| Multipoint | `001D:0E00 [0/1]`, lecture `0495:1409` (1 ou 2 connexions) | ✅ validé sur M5 (`ctl multipoint` ; le couper ne garde qu'un appareil) |
| Langue des invites | lecture `0807` | 🟡 lecture seule : changer de langue télécharge et installe un pack de voix (non fait) |
| Détection de port (désactivation) | `0400` ? | ➖ non capturé : désactive aussi 3 autres fonctions |
| Priorité du mode audio (haute résolution, sans perte) | `0405`/`0407` puis redémarrage `060F` | 🟡 lecture seule : le redémarrage bloque souvent l'app du téléphone (voir la fiche) |
| Gestes (personnalisation) | — | ➖ non personnalisables sur le M5 (indiqué par l'app) |
| Retrouver le casque, Zones sonores | — | ➖ fonctions de l'app (position du téléphone), rien côté casque |
| Personnalisation du son : sélection du profil existant | `0803 [00 03]` (après `3604 [00]` si Dolby Atmos) | ✅ validé sur M5 (capture HCI + relecture PC) |
| Personnalisation du son : lecture et export du profil | `2001`, `2003`…`2007` | ✅ lu par `probe --deep`, exporté brut par `ctl exporter-profil` (identique octet pour octet) |
| Personnalisation du son : nouveau test, écriture du profil, persistance | `2000`…`2006` | 🟡 capturé, non exposé (voir la fiche) |

Fiche détaillée (méthode, trames, séquence de démarrage de l'app, pistes) :
[docs/MOMENTUM5.md](MOMENTUM5.md).


## État porté / non porté

L'état physique « porté / non porté » est transmis par l'API du service,
distinct de l'activation du capteur et de Smart Pause. Sa lecture et ses
notifications sont essayées sur le casque depuis le PC, sous Windows
(27/10/2026) et sous Linux (01/11/2026) ; l'accueil de WinUI, de KDE et de
GNOME l'affiche (KDE et GNOME depuis la 1.8). La pause musicale au retrait n'est pas essayée. Voir la
[provenance et le contrat](MOMENTUM5.md#état-physique-du-port--observation-complémentaire-du-27102026).

## Sources

- [PROTOCOL.md de Oein/sennheiser-desktop-client](https://github.com/Oein/sennheiser-desktop-client/blob/main/PROTOCOL.md)
  (analyse de Smart Control, vérifiée sur ACCENTUM Wireless).
- Relevés sur un M5 réel : `probe --deep`, relevé croisé avec l'app via ADB,
  capture du journal HCI Bluetooth d'Android (détails dans la fiche).
- Aucun code tiers n'est copié : tout est réimplémenté à partir des faits
  documentés ou observés.
