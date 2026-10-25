# Roadmap : une vraie interface PC pour le MOMENTUM 5

Objectif : une application Windows complète, équivalente à Smart Control Plus
pour tout ce qui passe par le casque, mais pensée pour un PC portable (souris,
clavier, grand écran, barre système). L'icône reste le raccourci vers la
fenêtre principale.

**État au 25/10/2026** : dernière publication **v0.16.0** (jalon 0.16 :
interfaces en anglais avec choix de la langue, noms accessibles et
parcours clavier vérifiés en CI, contrastes, assistant de premier
lancement, rapport de bogue prérempli, guide et FAQ en deux langues, site
GitHub Pages, Discussions ; écoute réelle au Narrateur et à Orca à faire).
Avant elle, v0.15.1, version de
fiabilisation qui traite les constats A01 à A12 de l'audit du 24/10/2026
(données de l'utilisateur, confinement Linux, publication). Avant elle,
v0.15.0 (jalon 0.15 :
recherche KRunner servie par le service, liste de raccourcis Windows pour
les scènes et pastille de batterie sur l'icône de la barre des tâches,
extension Decky Loader pour le mode jeu ; tout passe par l'API du service.
Widgets Windows 11 et Xbox Game Bar reportés après la signature : ils
demandent une application empaquetée, MSIX). Avant elle, v0.14.0 (jalon 0.14 :
installateur Windows par utilisateur, `.deb` et `.rpm` essayés en mise à
jour par-dessus la version précédente, `PKGBUILD` pour l'AUR essayé dans
Arch, widget Plasma seul en `.plasmoid` ; soumissions à winget, à l'AUR et
au KDE Store préparées dans `SOUMISSIONS.md`, pas encore envoyées). Avant
elle, v0.13.0 (jalon 0.13-bêta : Flatpak construit sans réseau à partir de sources figées,
nouvel identifiant `io.github.asfax03l.openheadset-control`, permissions
revues, migration des réglages ; soumission à Flathub préparée dans
`FLATHUB.md`, pas encore envoyée). Avant elle, v0.12.0 (jalon 0.12 :
client GNOME en GTK 4 et libadwaita, première étape — page Accueil — et
parité définie avec le client KDE dans `PARITE.md`, étapes suivantes
comprises). Avant elle, v0.11.0 (jalon 0.11 :
service Linux durci, provenance et SBOM de chaque fichier publié, suite
d'essais `ctl essais` rejouée sur le casque, tests d'interface, couverture
et budget de ressources en CI ; droits réduits du service Windows étudiés
dans `SECURITE.md`, en attente de la signature). Avant elle, v0.10.0 (jalon 0.10 :
scène depuis l'état, export et import des réglages, reconnexion par le
Bluetooth du système, diagnostic PipeWire, `ctl` scriptable ; en avance
sur leur jalon : fuzzing des décodeurs, prévu en 0.11, et paquets `.deb`
et `.rpm`, prévus en 0.14). Avant elle, v0.9.1 (panneau rapide KDE,
widget Plasma enrichi) et v0.9.0 : service unique,
WinUI 3, client KDE, Flatpak, widget Plasma, écritures réservées au
micrologiciel 6.21.5, et la consolidation qui suit la revue du 21/10/2026
(demandes numérotées et résultats exacts, identité du casque, cible
multipoint opaque, installateurs récupérables, publication complète ou
rien, journaux sans données personnelles ; suivi dans
[`CONSOLIDATION.md`](CONSOLIDATION.md) ; fonctions de chaque interface dans
[`PARITE.md`](PARITE.md)). Le détail du protocole est dans
[`MOMENTUM5.md`](MOMENTUM5.md), le matériel validé dans le
[tableau de compatibilité du README](../README.md#compatibilité-momentum-5),
l'historique daté en fin de page.

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

## Prochaines étapes (proposées ; dates indicatives)

La qualité passe avant l'étendue : chaque jalon a un critère de sortie, et
une fonction n'est dite « validée » qu'avec modèle, micrologiciel, système
et date de l'essai sur le casque.

| Jalon | Contenu | Critère de sortie |
|---|---|---|
| **0.9.0** (consolidation) ✅ | Constats F01 à F14 de la revue ([`CONSOLIDATION.md`](CONSOLIDATION.md)) ; CI élargie (versions verrouillées, dépendances auditées) ; documentation alignée | Aucune fausse réussite d'une commande ; publication complète ou rien ; installateurs récupérables |
| **0.9.1** (KDE) ✅ | Panneau rapide KDE, widget enrichi | Commandes usuelles en deux clics, état relu affiché |
| **0.10** (confort et audio Linux) ✅ | Audio Linux en lecture seule, petites fonctions à fort gain, `ctl` plus scriptable | Observations PipeWire cohérentes avec les réglages son du bureau ; aucun changement de routage silencieux ; chaque fonction essayée sur le casque |
| **0.11** (sécurité et qualité) ✅ | Service durci, chaîne de confiance, tests automatisés | Service limité au Bluetooth et à ses fichiers ; suite d'essais rejouée avant la publication ; provenance vérifiable de chaque paquet |
| **0.12** (GNOME) ✅ | Interface GTK 4 / libadwaita cliente du même service | Parité définie avec le client KDE, sans connexion directe au casque |
| **0.13-bêta** (distribution) 🟡 | Flatpak à sources figées et Flathub | Installation, mise à jour et désinstallation propres sur Bazzite et Windows ; paquet accepté sur Flathub |
| **0.14** (paquets natifs) 🟡 | Installateur Windows, winget, `.deb`, `.rpm`, AUR, KDE Store | Pour chaque format : installation, mise à jour par-dessus la version précédente et désinstallation propres, puis essai sur le casque |
| **0.15** (intégration au bureau) 🟡 | KRunner, widgets Windows, mode jeu | Chaque intégration passe par l'API du service, avec les mêmes refus que les interfaces |
| **0.16** (langues, accessibilité, accueil) 🟡 | Anglais, accessibilité, premier lancement, documentation | Interfaces complètes en anglais ; parcours au clavier et au lecteur d'écran sans blocage |
| **1.0** | Version stable | Critères de publication remplis, politique de versions publiée, aucun constat ouvert sans décision |
| **1.1** (fonctions avancées) | Mesures, égaliseur, journal, automatisations | Fonctions désactivables ; aucune donnée envoyée ; seules les commandes validées |
| **1.2** (recherche sur le matériel) | Nouvelles écritures, autres casques | Capture dans l'application officielle, puis essai sur le casque avec relecture et retour à la valeur d'origine ; accord avant chaque essai |
| **Après la signature** | MSIX, COPR, widgets Windows 11 et Xbox Game Bar | Décision sur la signature prise ; mêmes critères que 0.14 |

Formats écartés : `.rpm` sur Bazzite (il faudrait modifier le système
avec `rpm-ostree` ; le Flatpak est la voie prévue), et Kubuntu 24.04 LTS
(encore sous Plasma 5, alors que le client KDE demande Plasma 6).

### Contenu des jalons

**0.10 — confort et audio Linux**
- Lecture PipeWire/WirePlumber de la sortie, du profil et du codec, en
  lecture seule ; diagnostic guidé.
- **Bouton « Reconnecter le casque »** : passe par le Bluetooth du système
  (BlueZ, Windows), pas par une commande du casque.
- **Enregistrer l'état actuel comme scène** : tous les réglages validés
  relus sur le casque, en un clic.
- **Exporter et importer mes réglages** : un fichier unique (scènes,
  préréglages, raccourcis, panneau rapide, thème) pour retrouver la même
  configuration sur un autre PC ou un autre système.
- **`ctl` scriptable** : sortie JSON pour toutes les sous-commandes,
  exemples pour Stream Deck et macros.
- **Complétion du shell et page `man` pour `ctl`** (`clap_complete`,
  `clap_mangen`).

**0.11 — sécurité et qualité**
- **Service Linux durci par systemd** : `NoNewPrivileges`,
  `ProtectSystem=strict`, `ProtectHome` en lecture seule sauf le dossier
  des données, `RestrictAddressFamilies=AF_UNIX AF_BLUETOOTH`.
- **Service Windows aux droits réduits** : étude d'un AppContainer ou d'un
  niveau d'intégrité bas pour `momentum-daemon`.
- **Chaîne de confiance sans certificat** : attestations de provenance
  GitHub (`actions/attest-build-provenance`) et SBOM joints aux
  publications, compilation reproductible.
- **Robustesse du protocole** : fuzzing du décodeur de trames et du
  parseur SDP (`cargo-fuzz`), sur les captures existantes.
- **Essais sur le casque automatisés** : les scripts d'essai (lecture,
  écriture validée, relecture, retour à la valeur d'origine) réunis en une
  suite rejouable avant chaque publication, avec un compte rendu daté.
- **Tests d'interface automatisés** en mode démonstration (UI Automation
  sous Windows, tests QML sous KDE), lancés par la CI.
- **Couverture de tests** affichée par la CI, avec un seuil minimal pour le
  cœur du protocole.
- **Budget de ressources** : mémoire et processeur du service mesurés à
  chaque version.

**0.12 — GNOME**
- Interface GTK 4 / libadwaita cliente du même service.

**0.13-bêta — distribution**
- Flatpak à sources figées (hors ligne), permissions revues, migration des
  réglages ; soumission à **Flathub**.
- Fait en v0.13.0 : tout sauf la soumission, préparée (`FLATHUB.md`) et
  attendant l'accord du mainteneur ; le critère « paquet accepté sur
  Flathub » reste ouvert.

**0.14 — paquets natifs**
- Windows : installateur par utilisateur, sans droits administrateur
  (`.exe` Inno Setup ou `.msi` WiX), puis publication **winget** avec la
  dépendance au Windows App Runtime 2.
- Linux : `.deb` (Debian 13, Ubuntu 26.04 et suivantes) et `.rpm` (Fedora
  43 et suivantes), compilés dans un conteneur par famille de distribution
  (`cargo-deb`, `cargo-generate-rpm`) et joints aux publications ;
  `PKGBUILD` pour l'**AUR** (Arch, CachyOS) ; widget Plasma sur le
  **KDE Store**.
- Fait en v0.14.0 : installateur Inno Setup (`.exe`, par utilisateur),
  `PKGBUILD`, `.plasmoid` et manifestes winget, avec leurs essais
  d'installation, de mise à jour et de retrait. Publications sur winget,
  l'AUR et le KDE Store préparées (`SOUMISSIONS.md`), en attente de
  l'accord du mainteneur.

**0.15 — intégration au bureau**
- **KRunner (KDE)** : « anc off », « scène Transport » depuis la recherche
  de Plasma, par l'API D-Bus comme le widget.
- **Liste de raccourcis Windows** (clic droit sur l'icône de la barre des
  tâches) pour appliquer une scène.
- **Pastille de batterie** sur l'icône de la barre des tâches Windows.
- **Widget Windows 11** (tableau des widgets) et **widget Xbox Game Bar** :
  batterie et réduction de bruit, sur le modèle du widget Plasma.
  Reportés après la signature : ces deux widgets exigent une identité de
  paquet (MSIX), que l'application n'a pas tant que ses exécutables ne
  sont pas signés.
- **Mode jeu de Bazzite et Steam Deck** : extension Decky Loader cliente
  de l'API D-Bus (batterie, réduction de bruit, scènes).

**0.16 — langues, accessibilité, accueil** (v0.16.0 ; reste l'écoute
réelle au Narrateur et à Orca)
- **Interface en anglais** (interfaces et documentation principale ; le
  français reste la langue de référence).
- **Accessibilité** : relecture au lecteur d'écran (Narrateur, Orca),
  navigation complète au clavier, contrastes des deux thèmes.
- **Assistant de premier lancement** : vérifie le Bluetooth, l'appairage
  et le Windows App Runtime, explique le multipoint.
- **Guide utilisateur** : installation pas à pas, dépannage Bluetooth
  (casque appairé mais non connecté, multipoint), FAQ.
- **Site de documentation** (GitHub Pages) : captures, guide, FAQ.
- **Rapport de bogue en un clic** : issue GitHub pré-remplie avec le
  diagnostic expurgé, relue par l'utilisateur avant l'envoi.
- **Discussions GitHub** pour les questions ; les issues restent pour les
  bogues et la compatibilité.

**1.0 — version stable**
- **Politique de versions écrite** : sens de la 1.0, compatibilité des
  fichiers de réglages, délai de correction des failles.
- **Avis de nouvelle version**, désactivé par défaut : simple lecture des
  publications GitHub, sans télémétrie ni envoi de données.

**1.1 — fonctions avancées**
- **Autonomie réelle mesurée** d'après `batterie.csv` : durée moyenne par
  charge, consommation selon le mode de réduction de bruit. Des mesures
  seulement, jamais une « santé » de batterie inventée.
- **Égaliseur d'après des mesures publiques** : meilleur réglage des 8
  bandes (±6 dB, fréquences fixes) vers une courbe cible, après
  vérification de la licence des données de mesure.
- **Export de la courbe vers EasyEffects (Linux) et Equalizer APO
  (Windows)**, pour une correction plus fine côté PC, sans toucher au
  casque.
- **Notifications supplémentaires**, chacune désactivable : casque resté
  allumé sans son, charge terminée si le casque la signale, retour de la
  connexion.
- **Journal local des événements** (déconnexions, changements de mode,
  refus) : consultable, effaçable, jamais envoyé.
- **Automatisations des scènes** à la demande de l'utilisateur
  (application au branchement du casque ou à une heure donnée), toujours
  avec les seules commandes validées.

**1.2 — recherche sur le matériel** (nouvelles captures de l'application
et accord pour chaque essai)
- **Réglages aujourd'hui en lecture seule** (priorité du mode audio,
  détection de port…) : écriture seulement après capture dans
  l'application officielle et vérification sur le casque.
- **Autres casques** (ACCENTUM, MOMENTUM 4…) : à partir de rapports
  « Compatibilité » et d'essais sur le matériel ; lecture seule jusque-là.

**Après la signature**
- **MSIX** (signature obligatoire), dépôt **COPR** pour Fedora (mises à
  jour automatiques).
- **Widget Windows 11** et **widget Xbox Game Bar** (venus de 0.15) :
  possibles une fois l'application empaquetée en MSIX.

Les versions sont indicatives : un jalon peut être scindé, mais une
fonction n'en change qu'avec une raison écrite ici.

Restent hors du périmètre de la 1.0 : mise à jour du micrologiciel,
réinitialisation d'usine, nouveau test auditif ou restauration d'un profil
auditif, suppression d'un appairage. D'autres casques ne seront pris en
charge qu'après validation sur le matériel ; jusque-là, ils restent en
lecture seule.

### Étapes terminées (résumé)

1. Fiabilité : session abandonnée après un envoi ambigu (v0.4.0).
2. Scènes nommées, appliquées une par une avec relecture (v0.4.0 ; plan
   historique dans [`PLAN-SCENES-IPC.md`](PLAN-SCENES-IPC.md)).
3. Confort : historique de la batterie, diagnostic expurgé, courbes, A/B,
   raccourcis éditables, panneau rapide (v0.4.0).
4. Recherche séparée : sidetone, suivi de tête, audio USB, paramètres du
   profil (v0.5.0 ; résultats dans [`MOMENTUM5.md`](MOMENTUM5.md)).
5. Distribution : archives Windows et Linux (v0.8.0) ; Flatpak, widget
   Plasma, guide de contribution et tableau de compatibilité (v0.9.0).
   Reste : signature des exécutables Windows (décision en attente),
   paquets natifs (jalon 0.14).
6. Plusieurs interfaces et Linux (v0.7.0).
7. Service unique et interfaces natives, retrait de Slint (v0.8.0).
8. Migration vers zbus 6 : plus tard, quand elle apportera un gain concret
   (elle demande de passer le service et le client en asynchrone).

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

