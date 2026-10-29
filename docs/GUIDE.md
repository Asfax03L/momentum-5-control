# Guide d'utilisation

*[English version](GUIDE.en.md) · [Questions fréquentes](FAQ.md)*

OpenHeadset Control règle le casque Sennheiser **MOMENTUM 5** depuis un PC
Windows 11 ou Linux (KDE Plasma), sans l'application mobile. Projet **non
officiel**, sans lien avec Sennheiser ni Sonova. Seuls les réglages validés
sur le casque sont modifiés ; tout le reste est en lecture seule. Ni mise à
jour du micrologiciel, ni réinitialisation d'usine.

## 1. Installer

### Windows 10/11

1. Télécharger l'installateur
   `openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe` sur la page
   [Releases](https://github.com/Asfax03L/openheadset-control/releases).
2. Le lancer : installation pour l'utilisateur courant, sans droits
   administrateur. Les exécutables ne sont pas signés : SmartScreen peut
   demander une confirmation (« Informations complémentaires » > « Exécuter
   quand même »).
3. L'interface demande **Windows App Runtime 2**. S'il manque,
   l'installateur s'arrête sans rien changer et donne l'adresse du runtime
   (ou `winget install Microsoft.WindowsAppRuntime.2`).

Désinstallation : Paramètres > Applications. Vérifier l'authenticité d'un
téléchargement : [VERIFIER.md](VERIFIER.md).

### Linux (KDE Plasma, Bazzite)

- **Flatpak** (recommandé sur Bazzite et les systèmes en lecture seule) :
  `flatpak install --user openheadset-control-vX.Y.Z-x86_64.flatpak`.
- **Debian, Ubuntu** : `sudo apt install ./openheadset-control_X.Y.Z-1_amd64.deb`.
- **Fedora** : `sudo dnf install ./openheadset-control-X.Y.Z-1.x86_64.rpm`.
- **Arch** : `PKGBUILD` joint aux publications.
- **Script** : `./installer-linux.sh` depuis l'archive `.tar.gz`, sans `sudo`.

Ne pas installer le Flatpak et un paquet à la fois : un seul service peut
tourner. Détails, widget Plasma et mode jeu : [LINUX.md](LINUX.md).

## 2. Premier lancement

Au premier lancement, un **assistant** passe en revue :

1. **Bluetooth** : il doit être activé (bouton « Ouvrir les paramètres
   Bluetooth »).
2. **Appairage** : il se fait dans les paramètres Bluetooth du système
   (mode appairage : voir la notice du casque). L'application n'appaire rien
   elle-même ; elle indique quand le casque est détecté.
3. **Windows App Runtime** (Windows) : présent puisque la fenêtre s'affiche.
4. **Multipoint** : le casque peut rester connecté à deux appareils à la
   fois (deux PC, ou un PC et un téléphone).

L'assistant se rouvre depuis **À propos** > « Assistant de premier
lancement ».

## 3. Les pages

- **Accueil** : batterie, modes de réduction de bruit (ANC, adaptatif,
  transparence, désactivé), curseur ANC ↔ transparence, anti-vent, audio
  spatial et suivi de tête, appareils appairés au casque (Connecter /
  Déconnecter), Multipoint (couper le Multipoint demande une confirmation :
  l'autre appareil est déconnecté).
- **Égaliseur** : mode audio (égaliseur, Speech Clarity, profil
  personnalisé), 8 bandes de −6 à +6 dB, amplification des basses,
  préréglages, export et import de courbes, comparaison A/B.
- **Scènes** : plusieurs réglages d'un coup (« Transport » : ANC maximal et
  anti-vent, par exemple). Éditeur intégré, ou « Enregistrer l'état actuel
  comme scène ».
- **Batterie** : courbe sur 24 h, 7 ou 30 jours, mesurée sur ce PC (export
  CSV, effacement confirmé).
- **Connexion et audio** : état de la liaison et sortie audio du système, en
  lecture seule.
- **Paramètres** : Smart Pause, réponse automatique, appels confortables,
  extinction automatique, protection de la batterie, invites, nom du casque,
  commandes tactiles ; thème clair ou sombre ; **langue** ; contenu du
  panneau rapide ; raccourcis clavier ; sauvegarde et restauration des
  réglages.
- **À propos** : modèle, micrologiciel, diagnostic, rapport de bogue, ce
  guide.

Rien n'est présenté comme appliqué avant la réponse du casque : chaque
réglage est relu, et un refus s'affiche dans un bandeau. Un réglage que le
casque n'a pas communiqué apparaît indisponible.

### Préréglages personnels enregistrés avant la 1.3

Dans Égaliseur (Windows ou KDE), les anciens préréglages restent conservés.
Avant de les appliquer, cliquer sur « Associer les anciens préréglages à
cet égaliseur » pour les lier aux fréquences documentées du M5 et aux
limites courantes. Cette action ne modifie pas le casque et conserve la
sauvegarde habituelle du fichier. Les courbes incompatibles restent présentes.
Faire cette association avant de réutiliser une scène qui en dépend.

Pendant un déplacement de curseur, la valeur demandée et la dernière
lecture sont distinguées. Une reconnexion annule les anciens gestes ;
recommencer le réglage après le retour de la connexion.

## 4. Icône et raccourcis

- **Clic gauche** sur l'icône près de l'horloge : panneau rapide (batterie,
  modes de bruit, curseur, scènes).
- **Clic droit** : menu complet (modes, scènes, égaliseur, Quitter).
- **Raccourcis globaux** : Ctrl+Alt+N mode de bruit suivant, Ctrl+Alt+P
  profil personnalisé ou égaliseur, Ctrl+Alt+M ouvrir la fenêtre ; une
  scène peut avoir le sien. Sous KDE, modifiables dans Configuration du
  système > Raccourcis.
- Fermer la fenêtre la cache seulement ; « Quitter » est dans le menu de
  l'icône.

Sous KDE s'y ajoutent le widget Plasma et la recherche KRunner (« anc off »,
« scène Transport ») ; sous Windows, la liste de raccourcis de la barre des
tâches (scènes) ; en mode jeu, l'extension Decky.

## 5. Langue

Paramètres > **Langue** : langue du système, Français ou English, appliquée
au prochain lancement de l'interface. Sans choix, l'interface suit la langue
du système (français si c'est la sienne, anglais sinon). La ligne de
commande `ctl` et le texte du diagnostic restent en français.

## 6. Accessibilité

- Tout se fait au **clavier** : Tab et Maj+Tab passent d'une commande à
  l'autre, Espace ou Entrée l'actionne, les flèches règlent les curseurs et
  les listes.
- Chaque commande porte un **nom lu par les lecteurs d'écran** (Narrateur,
  NVDA sous Windows ; Orca sous Linux) ; les titres de page et de section
  sont annoncés comme titres.
- Les couleurs suivent le thème du système (clair, sombre, contraste élevé
  sous Windows) ; les textes secondaires gardent un contraste d'au moins
  4,5:1.

Une gêne d'accessibilité est un bogue : la signaler (section 8).

## 7. Sauvegarder ses réglages

Paramètres > sauvegarde : exporter les scènes, préréglages, raccourcis,
panneau et apparence dans un fichier JSON, puis l'examiner avant de
l'importer sur un autre PC. L'historique de batterie n'est inclus que sur
demande. Aucun profil auditif ni identifiant d'appareil n'est exporté.

## 8. Signaler un problème

**À propos** > « Signaler un problème… » :

1. le **diagnostic expurgé** s'affiche (versions, modèle, micrologiciel,
   réglages lus ou non, incidents récents ; ni nom, ni adresse Bluetooth,
   ni numéro de série, ni dossier personnel, ni profil auditif) ;
2. « Ouvrir le formulaire sur GitHub » ouvre le formulaire « Bogue » dans le
   navigateur, déjà rempli avec ce texte ;
3. relire, compléter les étapes, puis envoyer sur GitHub (compte GitHub
   nécessaire). Rien n'est envoyé avant.

Une question plutôt qu'un bogue :
[Discussions](https://github.com/Asfax03L/openheadset-control/discussions).

## 9. Nouvelles versions

Paramètres > **Avis de nouvelle version** (désactivé par défaut) : au
lancement, l'interface lit la dernière publication sur GitHub et signale
une version plus récente, avec un bouton « Voir la publication ». Seules
votre adresse IP et le nom de l'application parviennent à GitHub ; rien
n'est téléchargé ni installé automatiquement. Le Flatpak se met à jour par
Flatpak. Ce que garantit chaque version :
[politique de versions](VERSIONS.md).

## 10. Où sont mes données

Tout reste sur le PC, dans le dossier des données :

- Windows : `%LOCALAPPDATA%\momentum-ctl-win` ;
- Linux : `~/.local/share/momentum-ctl-win` ;
- Flatpak : `~/.var/app/io.github.asfax03l.openheadset-control/`.

On y trouve `scenes.txt`, `prereglages.txt`, `raccourcis.txt`,
`panneau.txt`, `apparence.txt`, `langue.txt`, `mises-a-jour.txt`,
`notifications.txt`, `batterie.csv`, `journal.csv` et le journal technique
du service.

## 11. Autonomie, journal, notifications, scènes automatiques

- **Autonomie mesurée** (page Batterie) : d'après les mesures de ce PC,
  points de batterie perdus par heure et durée d'une charge complète à ce
  rythme, puis la même chose selon le mode de réduction de bruit. Il faut
  au moins une heure de décharge continue ; ce sont des mesures, jamais
  une « santé » de batterie.
- **Journal des événements** (page Batterie) : connexions, pertes, modes de
  réduction de bruit, refus du casque et scènes appliquées, gardés 30 jours
  sur ce PC (`journal.csv`), sans nom d'appareil ni adresse. Exportable,
  effaçable, jamais envoyé.
- **Notifications au choix** (Paramètres) : batterie faible, connexion
  perdue et retrouvée, et nouveau **casque allumé sans son** (aucune
  lecture signalée depuis 30 minutes, désactivé par défaut).
- **Scènes automatiques** (éditeur de la page Scènes, liste
  « Automatique ») : à la connexion du casque, ou chaque jour à une heure
  donnée (le casque doit alors être connecté). Une seule scène par
  déclenchement ; seules les commandes validées sont envoyées, et chaque
  application est notée au journal. Dans `scenes.txt` :
  `auto = connexion` ou `auto = 08:30`.
- **Export pour Equalizer APO** (page Égaliseur) : la courbe du casque au
  format GraphicEQ d'Equalizer APO (Windows), dans Documents, pour la
  reproduire côté PC. Correspondance approchée : la forme des filtres du
  casque n'est pas documentée ; l'amplification des basses n'est pas
  reproduite.
