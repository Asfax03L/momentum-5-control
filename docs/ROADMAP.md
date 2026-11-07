# Feuille de route

Mise à jour du 07/11/2026. Version publiée : **1.7.0** (04/11/2026). Ce qui
change à chaque version : [CHANGELOG.md](../CHANGELOG.md).

## Règles qui ne changent pas

- Une commande n'est envoyée au casque qu'après avoir été **capturée** dans
  l'application officielle, puis **validée sur le casque** avec relecture.
  Toute commande nouvelle demande l'accord explicite du mainteneur.
- **Jamais** : mise à jour du micrologiciel, réinitialisation d'usine,
  suppression d'un appairage, installation d'un pack de voix, balayage
  d'identifiants de commande.
- Un réglage relu n'est pas une preuve d'effet audible : un effet sonore est
  écrit comme « entendu » seulement après une écoute, sinon « non vérifié ».
- Un autre casque ou micrologiciel reste en lecture seule tant que chaque
  écriture n'y a pas été vérifiée.

## Où en est le projet

| Domaine | État |
|---|---|
| Protocole du MOMENTUM 5 (micrologiciel 6.21.5) | Toutes les fonctions de Smart Control Plus qui passent par le casque sont lues ; celles qui sont sûres sont validées en écriture ([compatibilité](COMPATIBILITE.md)) |
| Windows 11 (WinUI 3) | Complet, essayé sur le casque |
| KDE Plasma (Kirigami, widget, KRunner) | Complet, essayé sur le casque |
| GNOME (libadwaita) | Mêmes pages que KDE, panneau rapide et raccourcis ; affichage essayé sur le casque, changements depuis l'interface et raccourcis à essayer |
| Ligne de commande, Waybar, Decky | `ctl` complet ; Decky essayé hors du mode jeu |
| Distribution | Installateur Windows, Flatpak, `.deb`, `.rpm`, AUR (non soumis), archive ; publications attestées avec SBOM |

## 1.8 — Vérifications sur le casque et robustesse

Objectif : prouver sur le matériel ce qui ne l'est pas encore, sans nouvelle
commande.

| Travail | Pourquoi | Comment |
|---|---|---|
| ✅ **Égaliseur après extinction** | Un autre projet observe sur le M5 qu'une courbe réglée par l'application revient après extinction, mais qu'une courbe envoyée par un second appareil peut ne durer que la session | Fait le 07/11/2026 : la courbe écrite depuis le PC survit à l'extinction ([fiche](MOMENTUM5.md#égaliseur-après-extinction-07112026)) |
| ✅ **Longueur du nom du casque** | Une source indique 16 octets sur le M5 ; nous acceptions 32 caractères ASCII | Fait le 07/11/2026 : 16 caractères relus, 17 refusés ; limite ramenée à 16 |
| ✅ **Client GNOME sur un vrai bureau** | Raccourcis globaux (portail `GlobalShortcuts`) et changements de réglage jamais essayés hors démonstration | Fait le 07/11/2026 sur KDE Plasma : réglages changés et relus sur le casque, raccourci accepté puis utilisé. Défaut trouvé et corrigé : le portail exige une entrée de menu nommée d'après l'application (`org.openheadset.ControlGnome.desktop`) |
| **Reconnexion sous Windows** | Comportement documenté, jamais essayé sur le matériel | Casque éteint puis rallumé, interface ouverte |
| ✅ **État porté / non porté dans KDE et GNOME** | Lu et validé, affiché seulement par WinUI | Fait : ligne « charge · port » de l'accueil, même présentation commune (`home_presentation`) |
| ✅ **Robustesse de la session** | Cas relevés par d'autres projets : connexion sans réponse, notification avant la réponse, code d'erreur inconnu, octets parasites, trame coupée | Fait : tests simulés sur `MockTransport` ; un code d'erreur inconnu est gardé brut et la session reste utilisable. Reste : accusé d'abonnement absent, changement de mode lent |
| ✅ **Comparaison avec des vecteurs publics** | Vérifier nos décodages de trames contre une source indépendante | Fait le 07/11/2026 : les 37 vecteurs de trames et de flux de [jarek102/ohr](https://github.com/jarek102/ohr) sont reproduits sans écart (`examples/comparer_vecteurs.rs`, vecteurs non copiés) |

## 1.9 — Parité GNOME et confort

| Travail | Détail |
|---|---|
| Ce qui manque au client GNOME | Saisie des gains en texte, sauvegarde et restauration des réglages, choix de la langue, éditeur de raccourcis ([parité](PARITE.md)) |
| Dépendances de l'audio spatial expliquées | Suivi de tête sans audio spatial, profil personnalisé incompatible : l'interface dit pourquoi un réglage est grisé |
| Barres d'état | Exemples pour Waybar, et modules pour d'autres barres (Noctalia, Omarchy) à partir de `ctl barre` |
| Écoute au lecteur d'écran | Narrateur (Windows) et Orca (Linux), par le mainteneur ; défauts traités comme des bogues |
| Extension Decky | Essai dans le vrai mode jeu de Bazzite |

## Recherche (chaque commande nouvelle demande un feu vert)

Pistes vues dans des captures ou d'autres projets, **à valider sur le matériel**
avant toute écriture :

| Piste | Source | État ici |
|---|---|---|
| Pause de la musique en transparence totale (`1800`/`1801`) | Captures M5 d'un autre projet | Jamais lue ni écrite ici |
| Mode faible latence (`0817`/`0818`) | Carte de commandes de l'application | `0818` lu ; écriture non vérifiée ailleurs non plus |
| Niveau de transparence séparé (`1802`/`1803`) | Modèles intra-auriculaires ; couplage avec `1A03` observé sur M5 par un autre projet | À lire seulement |
| État USB du dongle BTD 700 | Projet tiers, par USB HID | Lecture seule envisageable ; le micrologiciel du dongle reste exclu |

## Bloqué, avec la raison

| Fonction | Raison |
|---|---|
| Égaliseur d'après des mesures publiques, export EasyEffects | La forme des filtres du casque (largeur, type) n'est pas documentée ; la licence des mesures est à vérifier |
| Avis « charge terminée » | Le casque n'a jamais envoyé de valeur « fin de charge » (`02` jamais observé) ; l'avis 100 % câble branché en tient lieu |
| Priorité du mode audio (haute résolution, sans perte) | L'application officielle redémarre le casque et reste souvent bloquée ; écriture non exposée |
| Langue des invites | La changer installe un pack de voix (interdit) |
| Widgets Windows 11, Xbox Game Bar, signature des exécutables | Demandent une identité signée (payante) |

## Abandonné

- Soumissions à **Flathub** (refus des soumissions faites par un agent d'IA)
  et à **winget** : abandonnées le 04/11/2026. Les paquets restent publiés sur
  la page Releases ; les fichiers de préparation restent dans `packaging/`.

## Historique

Les étapes de la série 0.x et des versions 1.0 à 1.7, avec leurs décisions :
[historique/ROADMAP-1.x.md](historique/ROADMAP-1.x.md). Plans terminés :
[consolidation](historique/CONSOLIDATION.md),
[réglages (lot 2)](historique/LOT2-REGLAGES.md),
[assistant EQ (lot 3)](historique/LOT3-ASSISTANT-EQ.md),
[scènes et IPC](historique/PLAN-SCENES-IPC.md),
[accueil WinUI](historique/WINUI-ACCUEIL.md),
[Flathub](historique/FLATHUB.md).

| Version | Date | Apport principal |
|---|---|---|
| 1.7.0 | 04/11/2026 | GNOME : panneau rapide, raccourcis globaux, paquets ; batterie système sous Windows ; avis 100 % |
| 1.6.0 | 04/11/2026 | Client GNOME complet ; choix du codec sous Linux ; batterie annoncée par le système |
| 1.5.0 | 01/11/2026 | Retour de voix (sidetone) ; module Waybar ; commandes dangereuses refusées avant envoi |
| 1.4.0 | 29/10/2026 | Assistant EQ guidé |
| 1.3.0 | 29/10/2026 | Curseurs, préréglages et comparaison A/B fiabilisés |
| 1.1.0 | 25/10/2026 | Autonomie mesurée, journal local, notifications au choix, scènes automatiques |
| 1.0.0 | 25/10/2026 | Version stable : politique de versions et de sécurité |

Étapes de développement avant la 1.0 :

| Phase | Contenu | État |
|---|---|---|
| 1 | Protocole GAIA V3 pur, `MockTransport`, tests sur captures | ✅ |
| 2 | `probe` : diagnostic Bluetooth en lecture seule | ✅ |
| 3 | Réglages du bruit et de l'audio spatial (`ctl`) | ✅ validé sur M5 |
| 4 | Icône dans la barre système, notifications, reconnexion | ✅ validé sur M5 |
| 5 | Fenêtre d'égaliseur | ✅ validé sur M5 |
| A–D | Fenêtre principale complète : Accueil, Mon son, Réglages, À propos | ✅ validé sur M5 |
| E | Raccourcis clavier globaux, notifications Windows, archive publiée | ✅ validé sur le PC (v0.2.0) |
| F | Expériences sur le casque : volume des invites, nom, connexions | ✅ validé sur M5 (v0.3.0) |
| G | Espace de travail Cargo : cœur, service, outils, interfaces séparés | ✅ (v0.7.0) |
| H | Linux : `ctl` et icône par BlueZ, KDE Plasma sous Wayland | ✅ validé sur M5 (v0.7.0) |
| I | Interface OpenHeadset Control (thèmes, disposition adaptative, démo) | ✅ validé sur M5 (v0.7.0) |
| J | Client natif KDE Qt 6 / Kirigami (Linux), logo, entrées de menu ; client du service, icône de la zone de notification, raccourcis KDE | ✅ essayé sur M5 (v0.8.0) |
| K | Service `momentum-daemon` et API D-Bus, `ctl` client du service (Linux) | ✅ essayé sur M5, lecture et écriture (v0.8.0) |
| L | Logo sous Windows : exécutable, menu Démarrer, notifications, icône d'état | ✅ (v0.8.0) |
| M | Service sous Windows (canal nommé) et client WinUI 3 (`windows-reactor`) | ✅ essayé sur M5 (v0.8.0) |
| N | Fonctions de l'icône Slint reprises par WinUI et KDE, puis retrait de Slint | ✅ essayé sur M5 (v0.8.0) |
| O | Consolidation (revue du 21/10) : demandes suivies, identité du casque, installateurs et publication sûrs, Flatpak, widget Plasma | ✅ essayé sur M5 (v0.9.0) |
| P | KDE : panneau rapide, widget Plasma enrichi (demandes suivies, scènes), icône sur zbus 6 | ✅ essayé sur M5 (v0.9.1) |
| Q | Confort et audio Linux : scène depuis l'état, export et import des réglages, reconnexion par le Bluetooth du système, diagnostic PipeWire, `ctl --json`, complétions et `man`, paquets `.deb` et `.rpm` | ✅ essayé sur M5 sous Bazzite (v0.10.0) |
| R | Sécurité et qualité : service Linux durci, provenance et SBOM des publications, `ctl essais`, tests d'interface, couverture, budget de ressources | ✅ suite d'essais rejouée sur M5 (v0.11.0) |
| S | Client GNOME (GTK 4, libadwaita), première étape : page Accueil, demandes suivies, `--demo` ; parité définie avec le client KDE | 🟡 état du M5 lu (v0.12.0) ; écritures essayées en démonstration |
| T | Distribution : Flatpak construit sans réseau (sources figées), identifiant `io.github.asfax03l.openheadset-control`, permissions revues, migration des réglages ; soumission à Flathub préparée | 🟡 installation, mise à jour, migration et désinstallation essayées sur Bazzite (v0.13.0) ; Flathub non soumis |
| U | Paquets natifs : installateur Windows (Inno Setup, par utilisateur), `.deb` et `.rpm` mis à jour par-dessus la version précédente, `PKGBUILD` (AUR), widget `.plasmoid` ; winget, AUR et KDE Store préparés | 🟡 installateur essayé sur Windows 11 avec le M5 (v0.14.0) ; paquets Linux essayés en conteneurs ; catalogues non soumis |
| V | Intégration au bureau : recherche KRunner (« anc off », « scène Transport »), liste de raccourcis Windows (scènes) et pastille de batterie, extension Decky Loader pour le mode jeu ; tout par l'API du service | 🟡 KRunner essayé sur le M5 (Bazzite) et scènes de la liste de raccourcis sur le M5 (Windows 11), v0.15.0 ; extension Decky essayée hors du mode jeu |
| W | Langues et accueil : interfaces en anglais (choix de la langue), accessibilité (noms lus par les lecteurs d'écran, parcours clavier, contrastes), assistant de premier lancement, rapport de bogue prérempli, guide et FAQ, site GitHub Pages | 🟡 vérifié en CI (UI Automation, KDE hors écran) dans les deux langues (v0.16.0) ; écoute au Narrateur et à Orca à faire |
| X | Version stable : politique de versions (`docs/VERSIONS.md`), politique de sécurité (`SECURITY.md`), avis de nouvelle version désactivé par défaut, décisions écrites sur les points ouverts | ✅ v1.0.0 |
| Y | Fonctions avancées : autonomie mesurée (aussi par mode de réduction de bruit), journal local des événements, notifications au choix (dont « casque allumé sans son »), scènes automatiques (connexion, heure fixe), export pour Equalizer APO | ✅ v1.1.0 ; scènes automatiques essayées sur le M5 (Bazzite) |

