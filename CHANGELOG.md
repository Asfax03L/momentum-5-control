# Journal des versions

Projet non officiel, sans lien avec Sennheiser ni Sonova. « Validé sur le
M5 » signifie : écriture capturée dans Smart Control Plus, puis rejouée
depuis le PC et relue sur un MOMENTUM 5 Wireless (M5AEBT), micrologiciel
6.21.5, Windows 11. La CI (fmt, clippy, tests) ne se connecte à aucun casque.

## Non publié

### Dépendances
- zbus 6 (au lieu de 5) pour le client D-Bus, le service Linux et le
  client KDE : zbus 6 n'a plus d'API bloquante, chaque appel passe par
  `zbus::block_on`.
- Icône de la zone de notification KDE écrite directement sur zbus 6
  (StatusNotifierItem et menu DBusMenu, d'après les spécifications
  freedesktop) au lieu de `ksni` : une seule version de zbus dans le client
  KDE. Elle se réinscrit si Plasma redémarre.

### Service et ligne de commande
- Événement `NotFound(raison)` à chaque recherche ratée du casque (aucun
  casque GAIA connecté, plusieurs sans casque mémorisé, profil Bluetooth
  déjà pris…) ; la raison s'affiche comme incident jusqu'à la connexion.
- `ctl` sans casque s'arrête dès la fin de la recherche en cours (quelques
  secondes au lieu de 15) avec cette raison, sans rien envoyer (code 1).

## v0.9.0 — 22/10/2026

### Consolidation (revue du 21/10/2026)
- Suivi des constats F01 à F14 dans `docs/CONSOLIDATION.md`.
- F02 : un casque connecté mais pas encore identifié refuse toute écriture
  (avant, `Headset::new` les autorisait jusqu'à `identify`). Les tests
  obtiennent un casque validé par un constructeur réservé
  (fonctionnalité `essais`, absente des binaires livrés).
- F01, F03, F06, F07 : **demandes suivies**. Chaque commande reçoit un
  numéro (`Submit` sur D-Bus, `Request::Submit` sur le canal Windows) et
  se termine par un seul `Event::Done` : appliquée et relue, déjà dans cet
  état, acquittée, refusée, non appliquée, annulée sans envoi, ou résultat
  inconnu après une coupure (jamais renvoyée). Chaque connexion au casque a
  un numéro ; une demande faite pour une connexion précédente n'est pas
  envoyée, et le `Snapshot` oublie les données de l'ancienne connexion.
  `ctl` n'attend plus que le résultat de sa demande ; WinUI et le client KDE
  montrent un refus immédiat au lieu d'attendre (KDE ne le prend plus pour
  une panne du service). L'ancienne méthode `Command` reste disponible.
  Essayé sur le M5 le 22/10/2026 sous Bazzite et Windows.
- F10 (en partie) : le nom du casque ne va plus dans les messages `info`.
- F04 : le casque est retrouvé **sans dépendre de son nom** (qui peut être
  changé) : parmi les appareils connectés qui annoncent GAIA, le casque
  mémorisé (`casque.txt`, empreinte sans l'adresse), sinon le seul
  candidat, sinon le seul au nom « MOMENTUM » ou « Sennheiser » ; plusieurs
  candidats indistincts : aucun n'est pris au hasard. Sous Windows, seuls
  les appareils connectés sont interrogés (la recherche ne réveille plus
  les autres). Même règle pour `ctl` sans service ; `--device` reste un
  choix explicite.
- F05 : une action multipoint (connecter, déconnecter) vise l'appareil par
  un **identifiant opaque** (empreinte de son adresse, qui reste dans le
  service), même si la liste change d'ordre ou si deux appareils portent
  le même nom ; la confirmation de déconnexion reste liée à l'appareil
  choisi. L'ancienne forme (position et nom) est refusée si le nom n'est
  pas unique.
- F09 : une publication n'est rendue publique qu'une fois **tous** ses
  paquets construits et testés (Windows, Linux, Flatpak) : brouillon,
  vérification des 6 fichiers, puis publication ; l'étiquette doit
  correspondre à la version de `Cargo.toml`.
- F08 : installateurs préparés et récupérables. Windows : tout est vérifié
  et copié à part avant d'arrêter l'application ; sans Windows App
  Runtime 2 ou avec une archive incomplète, rien n'est changé ; la version
  remplacée est gardée et remise si l'interface ne démarre pas. Linux :
  fichier manquant = arrêt avant tout changement ; binaires remplacés
  gardés et remis en cas d'échec ; `--desinstaller`.
- F10 : journaux ordinaires sans nom de casque ni adresse ; les contenus
  de trame de plus de 3 octets sont masqués en `warn` et `debug` ; les
  trames brutes ne vont qu'au niveau `trace`, à activer volontairement.
- F12 : documentation alignée sur l'état réel (feuille de route datée avec
  jalons et critères, essais Linux datés).
- Outillage : version de Rust figée (`rust-toolchain.toml`, 1.99.0) ;
  versions minimales déclarées et vérifiées par la CI (1.89, WinUI 1.95) ;
  audit des dépendances (`cargo-deny` : failles RustSec, licences,
  provenance) ; Dependabot mensuel, mises à jour regroupées.
- F11 : une ancienne fréquence d'échantillonnage n'est plus présentée
  après un changement de codec ; les fréquences de l'égaliseur sont
  documentées comme constante du profil M5.
- F14 : écritures de fichiers partagés sans nom temporaire commun ;
  préréglages relus juste avant chaque modification (un préréglage ajouté
  par l'autre interface n'est plus perdu).
- F13 (en partie) : la CI lance réellement le client KDE (mode
  démonstration, sans écran) et vérifie que le QML se charge sans erreur.
- `docs/PARITE.md` : fonctions de chaque interface (WinUI, client KDE,
  `ctl`, widget) après le retrait de Slint, avec ce qui reste à faire.
- F13 : `ctl` a des codes de sortie distincts (0 confirmé, 1 refusé ou
  annulé, 2 non appliqué, 3 résultat inconnu) ; son attente et le service
  à deux clients simultanés sont testés sans casque ; la CI lance aussi
  WinUI (runtime installé) en mode démonstration.
- F14 : raccourcis et contenu du panneau rapide ne sont plus écrasés s'ils
  ont été modifiés ailleurs depuis leur lecture (relus et signalés).
- Client KDE : le thème **clair ou sombre** s'applique aussi sous Plasma,
  même si Plasma est dans l'autre (jeu de couleurs Breeze pour
  l'application seule).

### Widget Plasma
- `plasma/org.openheadset.control.plasmoid` : widget Plasma 6 en QML,
  client de l'API D-Bus du service : batterie, mode de réduction de bruit,
  curseur ANC ↔ transparence, anti-vent, audio spatial, ouverture de la
  fenêtre. Ne démarre pas le service de lui-même. Installé par
  `installer-linux.sh`, livré dans l'archive Linux.
- Test de contrat : les commandes JSON écrites par le widget sont lues par
  `momentum_client::Command`.
- Essayé sur Bazzite 44 (Plasma 6.7) avec le M5 : état lu, anti-vent
  inversé puis remis par le même appel `Command` (relu).

### Sécurité du protocole
- Écritures réservées au MOMENTUM 5 **en micrologiciel 6.21.5**, le seul
  sur lequel elles ont été vérifiées : à la connexion, le micrologiciel
  (`1201`) est lu après le modèle ; un autre micrologiciel, ou un
  micrologiciel illisible, met le service et `ctl` en lecture seule, avec
  la raison dans le bandeau (« micrologiciel 6.22.0 non validé »).
  `DeviceError::ModelNotValidated` devient `WritesBlocked`.
- Essayé sur le M5 (6.21.5) le 21/10/2026 : écritures toujours permises
  (anti-vent inversé puis remis, relu).

### Contributions
- `CONTRIBUTING.md` : règles du projet (rien d'inventé, lecture seule par
  défaut, données personnelles), environnement, vérifications avant une PR.
- Modèles d'issue (bogue, compatibilité, idée) et de PR.
- README : tableau des modèles et micrologiciels testés.

### Flatpak (Linux)
- Paquet Flatpak `org.openheadset.Control` (environnement KDE 6.11 de
  Flathub) : client KDE, service lancé par activation D-Bus, `ctl`.
  Construit par la CI (`.github/workflows/flatpak.yml`) et joint aux
  publications. Manifeste : `packaging/flatpak/`.
- Fiche AppStream (`org.openheadset.Control.metainfo.xml`) pour Discover.
- Client KDE dans le bac à sable : fichier .desktop à l'identifiant de
  l'application, lancement à l'ouverture de session par le portail
  `Background`, bouton « Modifier dans KDE… » masqué.

## v0.8.0 — 21/10/2026

### Retrait de l'icône Slint (étape 4 de docs/ARCHITECTURE.md)
- `momentum-tray` (icône et fenêtre Slint) est retiré : WinUI 3 sous
  Windows et le client KDE en reprennent toutes les fonctions, en clients
  du service. Plus de dépendance à Slint ni à GTK.
- `ctl` ne passe plus que par le service (ou se connecte lui-même sans
  service) ; le canal texte de l'icône (`ipc`, `ipc_server`) est retiré,
  seul le JSON d'état de `ctl etat --json` reste (`momentum_core::json`,
  format inchangé).
- `installer.ps1` n'a plus d'option `-Interface` ; il retire l'ancienne
  icône (`momentum-tray.exe`, entrée de démarrage `momentum-ctl-win`).
  `installer-linux.sh` retire aussi `momentum-tray`, son entrée de menu et
  son démarrage automatique.

### Fonctions reprises de l'icône Slint (WinUI 3 et client KDE)
- Volume des invites et nom du casque ; export du profil ; export et
  import de courbes d'égaliseur ; comparaison A/B.
- Menu complet de l'icône (mode, curseur en 5 crans, anti-vent, audio
  spatial, Speech Clarity, profil, réglages du casque, scènes, démarrage),
  construit par `momentum_core::tray_menu` pour les deux interfaces.
- Éditeur des raccourcis globaux (scènes comprises) ; sous KDE, les
  scènes sont aussi enregistrées auprès de KGlobalAccel.
- Batterie : export CSV et effacement confirmé de l'historique
  (`Command::ClearBatteryHistory`, traité par le service).
- Page « Connexion et audio » : heure des dernières lectures et, sous
  Windows, vérification de la sortie audio du système (lecture seule).
- Panneau rapide (Windows) : contenu choisi dans les Paramètres
  (`panneau.txt`), bascule du profil, boutons de scènes.
- Éditeur de scènes dans la fenêtre (`momentum_core::scene_editor` :
  créer, dupliquer, renommer, déplacer, supprimer ; un renommage suit dans
  `raccourcis.txt` et `panneau.txt`).
- Thème clair ou sombre (`apparence.txt`) ; sans choix, celui du système.
  Sous KDE, appliqué avec Qt 6.8 ou plus récent.
- Mode démonstration `--demo` (`momentum_client::demo`) : casque simulé,
  sans service ni Bluetooth, bandeau permanent.
- Essayé le 21/10/2026 : volume des invites (77 → 50 → 77) et A/B sur le
  M5, valeurs remises ; menu de l'icône (anti-vent) sous Windows et KDE ;
  éditeur de raccourcis, export et effacement de la batterie, éditeur de
  scènes (fichiers sauvegardés puis remis) ; captures des autres pages.

### Service et API (étape 1 de docs/ARCHITECTURE.md, Linux)
- `momentum-daemon` : service sans fenêtre, seul propriétaire de la
  connexion ; historique de la batterie et notifications ; API D-Bus
  `org.openheadset.Control1` (commande et état en JSON, signal `Event`,
  propriétés `Connected`, `Battery`, `Name`).
- `momentum-client` : messages `Command`, `Event`, `Snapshot` (sérialisés en
  JSON ; types d'état de `momentum-core` sérialisables) et client D-Bus
  (`zbus` 5.19, API bloquante).
- `ctl` (Linux) passe par le service s'il tourne, et le lance au besoin
  (activation D-Bus) ; installé avec l'unité systemd utilisateur par
  `installer-linux.sh`. Essayé sur le M5 le 20/10/2026 : lecture, et
  écriture (anti-vent coupé puis remis par `ctl`, relu par le service).

### Installation et publication (étape 5)
- `installer.ps1` : interface WinUI 3 (WinUI, service et `ctl`, démarrage
  caché à l'ouverture de session) ; avertit si le Windows App Runtime 2
  manque ; `-Desinstaller` retire tout.
- `installer-linux.sh` : fonctionne aussi depuis une archive publiée
  (binaires et `assets` à côté du script).
- Publication : archives `openheadset-control-vX.Y.Z-windows-x86_64.zip`
  (WinUI, service, ctl, probe) et
  `openheadset-control-vX.Y.Z-linux-x86_64.tar.gz` (client KDE, service,
  ctl), avec leur empreinte SHA-256.

### Client WinUI 3 (Windows)
- `momentum-winui` : client du service en WinUI 3 natif, écrit en Rust
  avec `windows-reactor` 0.100 (Microsoft). NavigationView, fond Mica,
  thème du système, contrôles Fluent (RadioButtons, ToggleSwitch, InfoBar,
  ComboBox) ; pages Accueil (batterie, réduction de bruit, fonctions
  rapides, appareils appairés), Égaliseur (mode audio, 8 bandes,
  préréglages), Paramètres, À propos. Confirmation avant de couper le
  Multipoint ou de déconnecter un appareil.
- Logo intégré à l'exécutable (même `assets/logo/embed_logo.rs` que
  l'icône Slint).
- Intégration au bureau : icône de la zone de notification (logo teinté
  selon le mode, infobulle, menu : ouvrir, contrôle du bruit, quitter),
  fermer la fenêtre la cache, une seule instance (un deuxième lancement
  montre la fenêtre), raccourcis globaux de `raccourcis.txt` (partagé avec
  l'icône Slint), lancement à l'ouverture de session (Paramètres,
  démarrage caché `--cache`), `momentum-daemon.exe` lancé s'il ne tourne
  pas. `windows-reactor` ne gère ni la fermeture ni le masquage : la
  fenêtre WinUI est sous-classée en Win32 (`desktop.rs`).
- Pages Scènes (liste de `scenes.txt`, application par le service avec
  compte rendu, « Modifier scenes.txt »), Batterie (courbe tenue par le
  service sur 24 h, 7 ou 30 jours, estimation, détails) et « Copier le
  diagnostic » (À propos), comme le client KDE.
- Panneau rapide au clic gauche sur l'icône : petite fenêtre WinUI
  (batterie, mode de bruit, curseur ANC ↔ transparence, anti-vent, « Ouvrir
  OpenHeadset Control »), placée près de l'icône (`ui::panel_position`),
  hors de la barre des tâches, fermée quand elle perd le focus ; client du
  service avec les mêmes garde-fous. `--panneau` l'ouvre au lancement.
- `battery_log::chart_polylines` : courbe réduite en lignes brisées pour
  les interfaces qui dessinent des segments ; `system::os_version` déplacé
  de l'icône Slint vers `momentum-service`.
- Essayé le 21/10/2026 sur le M5 en lecture : pages affichées avec l'état
  réel, aucune écriture à l'ouverture (état et égaliseur relus).

### Service sous Windows
- `momentum-daemon` tourne aussi sous Windows, sans console : canal nommé
  de l'utilisateur (`openheadset-control-<SID>`), protocole JSON
  `momentum_client::wire` (état, événements numérotés, commandes), journal
  `momentum-daemon.log`.
- `momentum_client::Client` : même client sous Linux (D-Bus) et Windows
  (canal nommé) ; `ctl` passe par le service sous Windows aussi.
- Service restructuré : partie commune (état, commandes, batterie,
  notifications) et un transport par système.
- Essayé le 21/10/2026 sur le M5 (Windows 11) : `ctl etat` et
  `ctl connexions` par le service ; service Linux réinstallé et relu.

### Client KDE : pages et intégration (Linux)
- Accueil : appareils appairés au casque, avec connecter / déconnecter
  (confirmation avant de déconnecter ; aucun bouton pour ce PC ni en
  lecture seule).
- Pages Scènes (application avec relecture et compte rendu, `scenes.txt`
  ouvert dans l'éditeur) et Batterie (courbe sur 24 h, 7 ou 30 jours,
  estimation, tenue par le service).
- À propos : « Copier le diagnostic » (expurgé : adresses, noms d'appareils,
  dossier personnel).
- Lancement à l'ouverture de session (`~/.config/autostart`, démarrage
  caché `--cache`) et instance unique par D-Bus.
- `momentum_core::viewmodel::connection_row` et
  `diagnostic::os_release_name` partagés avec l'icône Slint.
- Essayé sur Plasma 6 le 21/10/2026 : accueil et appareils, deuxième
  lancement, démarrage caché.

### Client KDE, étape 2 de docs/ARCHITECTURE.md (Linux)
- `momentum-kirigami` est client du service `momentum-daemon` : il ne tient
  plus la connexion au casque, démarre le service au besoin et se
  reconnecte s'il redémarre. `ctl` et la fenêtre fonctionnent ensemble.
- Icône de la zone de notification KDE (StatusNotifierItem, `ksni` 0.3.7) :
  logo teinté selon le mode, infobulle, menu (ouvrir, contrôle du bruit,
  quitter). Fermer la fenêtre la cache.
- Raccourcis globaux KDE (KGlobalAccel) : Ctrl+Alt+N, Ctrl+Alt+P,
  Ctrl+Alt+M, modifiables dans Configuration du système > Raccourcis.
- `momentum_core::hotkeys::profile_toggle` : règle du raccourci « profil »
  commune à Slint et Kirigami.
- Essayé sur Plasma 6 (Bazzite) le 21/10/2026 : état lu par le service,
  icône enregistrée, raccourcis déclarés, fenêtre cachée puis rouverte par
  Ctrl+Alt+M.

### Logo sous Windows
- Le logo est intégré à `momentum-tray.exe` (Explorateur, menu Démarrer,
  barre des tâches, Alt+Tab) et affiché dans l'en-tête des notifications.
- L'icône de la zone de notification reprend le logo : casque blanc sur un
  carré arrondi dont la couleur suit le mode (bleu réduction, turquoise
  adaptatif, orange transparence, ardoise arrêt, gris translucide sans
  casque). Même dessin pour l'icône Slint sous Linux.
- `installer.ps1` ajoute « OpenHeadset Control » au menu Démarrer (retiré
  par `-Desinstaller`).

### Client KDE (Qt 6 / Kirigami)
- `momentum-kirigami` : client natif KDE pour Linux (style Breeze, suit le
  thème Plasma, tiroir de navigation). Accueil (casque, batterie,
  micrologiciel, modes de bruit, curseur ANC ↔ transparence, anti-vent,
  Multipoint avec confirmation, audio spatial, suivi de tête, commandes
  tactiles, Smart Pause), Égaliseur (mode audio, 8 bandes lues sur le
  casque, basses, préréglages), Paramètres, À propos. Même worker et mêmes
  modèles de vue que l'icône ; un réglage n'apparaît qu'une fois relu.
  Essayé sur le M5 le 20/10/2026 (Bazzite 44, Wayland).

### Logo et intégration Linux
- Logo d'OpenHeadset Control (`assets/logo/openheadset.svg`), utilisé pour la
  fenêtre Slint (barre de titre, barre des tâches) et le client Kirigami.
- `scripts/installer-linux.sh` : binaires, logo (SVG et PNG de 16 à 256 px)
  et entrées de menu pour l'utilisateur, sans sudo.
- Client Kirigami : nom d'application, fichier .desktop et icône transmis à
  Qt (petite cale C++) ; sous Wayland, Qt envoyait « aucune icône » et la
  barre des tâches de Plasma affichait une icône vide.
- `docs/ARCHITECTURE.md` : direction choisie (service unique, interfaces
  natives clientes, tout en Rust) et étapes.
- CI Linux : compilation du client Kirigami (Qt 6) et archive des binaires.

## v0.7.0 — 20/10/2026

### Interface OpenHeadset Control
- Correction : le premier geste sur un curseur (ANC ↔ transparence,
  bandes de l'égaliseur, volume des invites) ne faisait que donner le focus
  et n'envoyait rien au casque. Le curseur ANC ↔ transparence est toujours
  affiché (le casque passe en Personnalisé dès qu'on le règle). Validé sur
  le M5 sous Windows le 20/10/2026.
- Nouvelle fenêtre (même fonctions, même lien avec le casque) : thème bleu
  nuit et thème clair, cartes, icônes vectorielles originales, illustration
  du casque, animations courtes. Navigation en cinq sections : Accueil,
  Appareil, Égaliseur, Commandes, Paramètres.
- Disposition adaptative selon la largeur réelle : barre latérale, rail
  d'icônes ou navigation en bas ; tableau de bord sur deux colonnes ou une.
- État « envoi au casque » : un réglage n'apparaît appliqué qu'une fois
  relu sur le casque (attente visible sur l'interrupteur ou la tuile).
- Carte du casque : modèle, micrologiciel et révision lus (jamais le numéro
  de série) ; batterie en pourcentage.
- `--demo` (données simulées, clairement signalées) et `--capture <dossier>`
  (rendus PNG à plusieurs tailles).
- Libellés système neutres (« Paramètres de son de Windows/Linux »).


### Journal
- Correction : les lignes du worker (connexion, perte, refus) manquaient au
  journal depuis la réorganisation en espace de travail.
- L'icône note son arrêt : « arrêt demandé : menu Quitter » puis « arrêt
  de momentum-tray », ou un avertissement si la boucle d'événements se
  termine sans « Quitter ». Un plantage est écrit dans le journal avant
  l'arrêt (sinon invisible en release). Un arrêt forcé de l'extérieur ne
  laisse pas de ligne : son absence le signale.

### Organisation du dépôt
- Espace de travail Cargo : `crates/momentum-core` (protocole, casque,
  modèles d'affichage), `crates/momentum-service` (worker, canal local,
  intégration système), `crates/momentum-cli` (`ctl`, `probe`) et
  `frontends/slint` (icône). Aucun changement de comportement ; les
  binaires gardent leur nom. `frontends/README.md` décrit comment ajouter
  une autre interface (WinUI, Kirigami, GTK…).

### Linux (étape 1)
- `ctl` compile et se connecte sous Linux par BlueZ : casque appairé et
  connecté annonçant l'UUID GAIA du M5, profil client BlueZ ou canal imposé
  (`--canal`). Données dans `~/.local/share/momentum-ctl-win`.
- Validé en lecture le 20/10/2026 sur Bazzite 44 (KDE) : `ctl etat` lit
  tout l'état sans canal imposé ; la lecture audio continue pendant et
  après. Aucun réglage écrit sous Linux pour l'instant.
- Étape 2 : l'icône `momentum-tray` tourne sous Linux (KDE Plasma,
  Wayland). Icône StatusNotifierItem par GTK, menu, infobulle et fenêtre
  identiques à Windows ; notifications par D-Bus ; lancement à l'ouverture
  de session (`~/.config/autostart`). Validé en lecture le 20/10/2026 sur
  Bazzite 44 (connexion BlueZ, état affiché, son non interrompu). Pas encore
  sous Linux : panneau rapide (clic gauche), raccourcis globaux, lecture
  des sorties audio, canal local pour `ctl`.
- CI : job Ubuntu (clippy, tests, binaire `ctl` Linux en artefact).
- Guide d'essai : `docs/LINUX.md` (dual boot, lecture seule d'abord).

## v0.6.1 — 12/10/2026

### Corrections
- Sélection du profil personnalisé : le mode audio est relu (`0804`) juste
  avant l'écriture. S'il est déjà sur le profil, rien n'est écrit (le casque
  refusait le `0803` redondant par `0983 [80]`), et le résultat le dit
  (« déjà sélectionné à la lecture ») au lieu d'annoncer une écriture. Une
  notification reçue pendant cette lecture (mode, profil, spatial réactivé)
  l'annule : l'écriture normale part alors. Le cache de l'application n'est
  jamais utilisé pour cette décision.
- Refus de `0803` : erreur dédiée avec le code brut et son sens propre à
  cette commande d'après l'app officielle (80 déjà sélectionné, 81 non
  configuré, 82 non pris en charge, 83 erreur générique ; seul 80 observé).
  Un refus n'est jamais converti en succès.
- Réponses de la sélection validées strictement (`2001 [02]`, `3605 [00|01]`,
  `0804 [00, mode]`).

### Limites
- Garde vérifiée sans casque (trames synthétiques) ; pas encore observée
  sur le MOMENTUM 5.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell ; si Windows bloque les scripts :
`powershell -ExecutionPolicy Bypass -File installer.ps1`). Les fichiers de
l'utilisateur sont conservés.

## v0.6.0 — 12/10/2026

### Ajouts
- Page **Connexion et audio** (lecture seule) : explique les cas « casque
  connecté, mais pas de son ». Elle sépare trois sources, chacune avec son
  heure de lecture :
  - le pilotage du casque (état brut de ce PC dans la liste du casque,
    autres appareils connectés) ;
  - la sortie audio de Windows (sortie par défaut, liaison Bluetooth ou USB,
    stéréo ou mains libres, format de mixage, volume et muet) ;
  - le flux rapporté par le casque (codec, fréquence).
  Constats : son envoyé vers une autre sortie, muet ou volume à 0, profil
  mains libres, aucune sortie audio active vers le casque, pas de lecture,
  Multipoint. Bouton vers les paramètres de son de Windows ; rien n'est
  changé automatiquement, aucune commande nouvelle n'est envoyée au casque.
- Page **Scènes** : créer, renommer, dupliquer, supprimer et réordonner les
  scènes sans modifier `scenes.txt` à la main ; aperçu des étapes et
  vérification des règles avant l'enregistrement. Enregistrer n'envoie rien
  au casque ; « Appliquer au casque » est une action séparée. L'ancien
  fichier est gardé dans `scenes.txt.bak` ; un fichier qui contient des
  erreurs, ou modifié à la main pendant l'édition, n'est pas écrasé. Les
  raccourcis et le panneau suivent un renommage ou une suppression.
- **Panneau rapide** au choix (fichier `panneau.txt`) : contrôle du bruit,
  curseur, profil personnalisé, et jusqu'à 4 scènes choisies.

### Corrections
- Diagnostic : lectures présentées en listes (« lues : … ; non lues : … »)
  au lieu de « langue lu ».

### Essais d'usage (12/10/2026, version de test sur le PC)
- Bouton « Copier » du diagnostic : texte collé dans le Bloc-notes, sans
  nom de casque ni adresse.
- Raccourcis : ouvrir la fenêtre, mode de bruit suivant (trois appuis,
  retour au mode de départ), profil personnalisé / égaliseur (aller-retour),
  modification d'un raccourci puis retour à la valeur d'origine. Casque
  relu ensuite : état de départ retrouvé.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell ; si Windows bloque les scripts :
`powershell -ExecutionPolicy Bypass -File installer.ps1`). Il arrête l'icône
en cours, la remplace et la relance. Les fichiers de l'utilisateur sont
conservés.

## v0.5.2 — 11/10/2026

### Corrections (essai d'usage réel)
- Panneau rapide : **Échap** le ferme dès l'ouverture (le clavier n'allait
  pas au panneau tant qu'on n'avait pas cliqué dedans).
- Panneau rapide : un clic sur l'icône le ferme sans le rouvrir aussitôt
  (le clic passe d'abord par la barre des tâches, ce qui fermait le
  panneau ; l'icône le rouvrait ensuite).
- Vérifié sur le PC par un vrai clic sur l'icône : ouverture, Échap, clic
  sur l'icône, clic ailleurs.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell ; si Windows bloque les scripts :
`powershell -ExecutionPolicy Bypass -File installer.ps1`). Il arrête l'icône
en cours, la remplace et la relance. Les fichiers de l'utilisateur sont
conservés.

## v0.5.1 — 10/10/2026

### Corrections (audit indépendant de la v0.5.0, sans casque)
- Égaliseur : dans un même lot de commandes, « remettre à plat » puis
  « restaurer la courbe » pouvait laisser le casque à plat (la restauration
  était comparée à un état périmé). `set_eq_band` et `set_eq_gains` tiennent
  maintenant l'état à jour à chaque bande acquittée, et le worker relit
  l'égaliseur avant une commande d'égaliseur qui suit une scène.
- Lot de commandes : deux actions distinctes (connexion de deux appareils,
  deux exports) ne se remplacent plus. Seules les valeurs absolues sont
  fusionnées, et jamais par-dessus un réglage dont elles dépendent (audio
  spatial et suivi de tête, mode de bruit et niveau).
- Connexions : l'appareil visé est retrouvé par son adresse (gardée en
  mémoire, jamais affichée), et non plus par sa position et son nom ; deux
  appareils du même nom restent distincts. Rien n'est envoyé si l'appareil
  est déjà dans l'état demandé.
- Diagnostic : le masquage des noms ne plante plus sur un texte dont la
  longueur change en minuscules (`İ`, `ẞ`).
- Tests : parcours du worker sur `MockTransport` (lot, scène puis courbe,
  bande refusée, délai dépassé, connexions) et tests de l'audit
  (`tests/audit_v050.rs`).

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell) : il arrête l'icône en cours, la remplace et la relance.
Les fichiers de l'utilisateur sont conservés. Vérifier l'archive avec le
fichier `.sha256` publié (exécutables non signés).

## v0.5.0 — 10/10/2026

### Ajouts (validés sur le M5)
- `ctl suivi-tete on|off` : suivi de tête de Dolby Atmos (`3602`, relu par
  `3603`), refusé quand l'audio spatial est coupé, comme dans l'app.
- `ctl multipoint on|off` : Multipoint (`001D:0E00`, relu par `0495:1409`).
  Le couper ne garde qu'un appareil : depuis le PC, le casque a gardé le PC.
- Fenêtre (Accueil) : interrupteur « Suivi de tête » sous l'audio spatial
  (réglable seulement quand il est actif) et interrupteur « Multipoint »
  dans les appareils appairés, avec confirmation avant de le couper. Validés
  depuis la fenêtre sur le M5. `ctl etat --json` ajoute `suivi_tete`.

### Corrections
- Liste des connexions : un appareil connecté en lecture audio (état 3)
  s'affichait « non connecté ».
- La liste des connexions est relue 3 s après un changement (Multipoint,
  connexion ou déconnexion) : le casque ne notifie pas toujours la
  déconnexion d'un autre appareil.
- Ce PC est toujours affiché « Connecté (ce PC) » : l'état lu sur le casque
  pour lui (lien audio) varie d'une lecture à l'autre alors que la connexion
  de contrôle fonctionne.

### Recherche (étape 4)
- Sidetone : absent de l'app pour le M5, lecture seule.
- Suivi de tête : flux de position `368D` décodé en partie (lacet et
  tangage en degrés, hypothèse).
- Profil personnalisé : calculé par une bibliothèque propriétaire, piste
  arrêtée.
- USB (lecture seule) : composite `3542:1004` (audio USB, HID multimédia et
  téléphonie, HID générique non exploré) ; le contrôle par Bluetooth
  fonctionne câble branché.

### Limites connues
- Exécutables non signés : Windows SmartScreen peut demander une
  confirmation. Vérifier l'archive avec le fichier `.sha256` publié.
- Le suivi de tête n'est réglable qu'avec l'audio spatial actif, et l'audio
  spatial n'est pas disponible tant que le profil personnalisé est
  sélectionné (comme dans l'app).
- Couper le Multipoint ne garde qu'un appareil connecté : dans les deux
  essais (depuis l'app et depuis le PC), le casque a gardé le PC et
  déconnecté le téléphone ; la règle exacte n'est pas établie.
- Langue des invites et priorité du mode audio restent en lecture seule.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell) : il arrête l'icône en cours, la remplace et la relance.
Les fichiers de l'utilisateur sont conservés.

## v0.4.0 — 09/10/2026

### Ajouts
- **Scènes** : fichier `scenes.txt` (trois exemples créés au premier
  lancement), boutons dans l'Accueil, sous-menu « Scènes » de l'icône,
  `ctl scenes` et `ctl scene <nom>`. Uniquement des réglages déjà validés
  (mode audio, audio spatial, bruit, curseur, anti-vent, commandes tactiles,
  préréglage d'égaliseur, basses). Cohérence vérifiée à la lecture du
  fichier ; application une étape à la fois, arrêt à la première erreur et
  rapport de ce qui a été appliqué. Validé sur le M5 : « Travail » puis
  « Transport », depuis `ctl` et depuis la fenêtre, relus et revenus à
  l'état de départ.
- **Canal local `ctl` ↔ icône** : named pipe réservé à l'utilisateur
  courant, sans accès réseau, liste fermée de requêtes. `ctl etat`,
  `ctl etat --json` (JSON versionné, `null` pour une valeur non lue),
  `ctl scenes` et `ctl scene` fonctionnent pendant que l'icône tourne ; les
  autres sous-commandes le signalent au lieu d'échouer sur une connexion
  occupée.

- **Historique de la batterie** (page « Batterie ») : mesures déjà lues par
  l'icône, sans commande supplémentaire, gardées 30 jours sur le PC
  (`batterie.csv`) ; courbe sur 24 h, 7 ou 30 jours, interruptions et silences
  de plus de 30 min laissés vides ; estimation du temps restant seulement
  après une heure de décharge continue et 5 points perdus ; export CSV et
  effacement confirmé.
- **Diagnostic expurgé** (page « À propos ») : versions, modèle,
  micrologiciel, réglages lus ou non, incidents récents ; noms, adresses
  Bluetooth, chemins personnels retirés. Montré avant la copie.

- **Égaliseur** : saisie des 8 gains au clavier (dB, virgule décimale,
  bornes du casque vérifiées) ; export de la courbe dans Documents et import
  par la boîte « Ouvrir » de Windows (format texte versionné avec modèle,
  nombre de bandes, gains et basses ; refus d'un autre modèle) ; comparaison
  A/B des gains.
- **Raccourcis éditables** dans la page Réglages, y compris un raccourci par
  scène (`scene:Nom` dans `raccourcis.txt`) ; enregistrés et réactivés sans
  relancer l'icône.

- **Panneau rapide** au clic gauche sur l'icône (la fenêtre complète reste
  accessible par le panneau et le menu) : batterie, modes de bruit, curseur,
  profil personnalisé, scènes ; placé au-dessus ou au-dessous de l'icône dans
  la zone de travail de l'écran ; fermé par Échap ou dès qu'une autre fenêtre
  passe au premier plan. Option `--panneau` pour l'ouvrir au lancement.

### Fiabilité
- Le journal détaillé (`RUST_LOG=…=debug`) ne contient plus les adresses
  Bluetooth de l'identifiant du service RFCOMM.
- Une erreur pendant l'envoi d'une commande ou pendant l'attente de sa
  réponse abandonne désormais la session, comme un délai dépassé : les octets
  ont pu partir, et une réponse tardive serait prise pour celle de la
  commande suivante. Aucune écriture n'est renvoyée automatiquement ; l'icône
  se reconnecte et relit l'état. Tests synthétiques ajoutés (accusé tardif
  après un envoi en échec, connexion fermée pendant l'attente).

### Documentation
- Roadmap réorganisée : correspondance actuelle avec l'app, prochaines
  étapes proposées, historique daté des phases.
- Commentaires du protocole mis à jour (statuts périmés).

### Pas encore essayé sur le casque
- Gains saisis au clavier, import d'une courbe et comparaison A/B : même
  écriture bande par bande que l'égaliseur (validée), mais pas rejouée en
  mode égaliseur depuis ces nouveaux boutons.
- Clic réel sur l'icône de la barre système (le panneau a été essayé par
  l'option `--panneau`, même code).
- Bouton « Copier » du diagnostic.

### Limites connues
- Exécutables non signés : Windows SmartScreen peut demander une
  confirmation. Vérifier l'archive avec le fichier `.sha256` publié.
- `ctl` hors de `etat`, `scenes` et `scene`, ainsi que `probe`, demandent
  de quitter l'icône (une seule connexion GAIA à la fois).
- Langue des invites et priorité du mode audio restent en lecture seule.
- Dépôt privé : la publication n'est téléchargeable qu'avec un accès au
  dépôt.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell) : il arrête l'icône en cours, la remplace et la relance.
Les fichiers de l'utilisateur (`prereglages.txt`, `raccourcis.txt`,
`scenes.txt`, `batterie.csv`) sont conservés.

## v0.3.0 — 09/10/2026

### Ajouts (validés sur le M5)
- **Volume des invites** : curseur en % dans Réglages et `ctl volume-invites
  <0-100>`. Échelle relevée dans l'app : valeur du casque de −20 (0 %) à +6
  (100 %), 0 = 77 %.
- **Nom du casque** : champ « Nom du casque » dans Réglages et `ctl nom
  [nouveau]`. Le titre de la fenêtre affiche le nom lu sur le casque.
  Limite du projet : 32 caractères ASCII imprimables (longueur maximale et
  accents non relevés). Windows et le téléphone peuvent garder l'ancien nom
  jusqu'à la prochaine connexion.
- **Gestion des connexions** : liste des appareils appairés au casque dans
  l'Accueil, avec Connecter / Déconnecter ; `ctl connexions`, `ctl connecter
  <n>`, `ctl deconnecter <n>`. Le PC qui pilote le casque ne peut pas se
  déconnecter lui-même. Les adresses Bluetooth ne sont ni affichées, ni
  journalisées, ni exportées.

### Vérifié, sans nouvelle commande
- **Persistance du profil personnalisé** : identique octet pour octet et
  toujours sélectionné après une extinction complète.

### Restent en lecture seule, volontairement
- **Langue des invites** : en changer installe un pack de voix sur le casque
  (transfert assimilable à une mise à jour, exclu du projet).
- **Priorité du mode audio** (haute résolution, sans perte) : le changement
  redémarre le casque, et l'app du téléphone est restée bloquée deux fois sur
  trois jusqu'à une extinction manuelle.
- Non exposés : suppression d'un appairage, interrupteur Multipoint, mise à
  jour du micrologiciel, réinitialisation d'usine, nouveau test auditif.

### Limites connues
- Exécutables non signés : Windows SmartScreen peut demander une
  confirmation. Vérifier l'archive avec le fichier `.sha256` publié.
- Une seule connexion GAIA à la fois sur le PC : quitter l'icône avant
  d'utiliser `ctl` ou `probe`.
- Seul le MOMENTUM 5 (préfixe de modèle `M5AEBT`) est autorisé en écriture ;
  un autre modèle s'ouvre en lecture seule.
- Dépôt privé : la publication n'est téléchargeable qu'avec un accès au
  dépôt.

### Installation
Décompresser l'archive, puis lancer `installer.ps1` (clic droit > Exécuter
avec PowerShell). Il installe l'icône pour l'utilisateur courant, sans droits
administrateur, et l'inscrit au démarrage de Windows (décochable dans le menu
de l'icône). Désinstallation : `installer.ps1 -Desinstaller`.

## v0.2.0 — 09/10/2026

- Fenêtre principale complète (Slint, thème Fluent) : Accueil (bruit, ANC ↔
  transparence, anti-vent, audio spatial, commandes tactiles), Mon son
  (égaliseur 8 bandes, basses, préréglages, Speech Clarity, sélection et
  export du profil personnalisé), Réglages (Smart Pause, réponse automatique,
  appels confortables, extinction automatique, invites, protection de la
  batterie), À propos.
- Raccourcis clavier globaux (Ctrl+Alt+N bruit, Ctrl+Alt+P profil,
  Ctrl+Alt+M fenêtre), notifications Windows (batterie faible, connexion
  perdue ou retrouvée).
- Archive publiée par la CI avec empreinte SHA-256 et installateur par
  utilisateur.
