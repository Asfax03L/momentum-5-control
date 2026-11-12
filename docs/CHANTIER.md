# Chantier 1.9 — Parité GNOME et confort

Journal de reprise du chantier en cours. Il suffit à reprendre le travail :
ce qui est fait, ce qui reste, les décisions et leur raison. Ouvert le
12/11/2026 ; à déplacer dans `docs/historique/` quand la 1.9 est publiée.

## Objectif

Le client GNOME (`momentum-gtk`) offre ce que le client KDE offre déjà et
qui lui manque ; les réglages grisés disent pourquoi dans KDE et GNOME ; des
exemples prêts à copier existent pour les barres d'état.

**Fini veut dire** : chaque lot fusionné dans `main` avec la CI verte ; le
client GNOME compilé et son essai sans écran (`--demo --essai-interface`)
réussi ; `docs/PARITE.md`, `docs/ROADMAP.md` et `CHANGELOG.md` à jour.

**Hors périmètre** : toute commande nouvelle vers le casque (aucune dans ce
chantier) ; l'écoute au lecteur d'écran et l'essai Decky en mode jeu, qui
demandent le mainteneur ; la publication de la 1.9, qui demande son accord ;
l'essai de reconnexion sous Windows (dernier point de la 1.8).

## Lots

| Lot | Contenu | Preuve | État |
|---|---|---|---|
| 1 | GNOME, page Mon son : saisie des gains en texte, export pour Equalizer APO | essai sans écran étendu (gains saisis puis relus) | ✅ 12/11/2026 |
| 2 | GNOME, pages À propos et Batterie : sauvegarde et restauration des réglages, langue, notifications au choix, journal local, effacement de l'historique | essai sans écran étendu (notification enregistrée puis remise, sauvegarde écrite puis examinée) ; captures relues | ✅ 12/11/2026 |
| 3 | GNOME : contenu du panneau rapide, raccourcis (liste et bouton de configuration du bureau) | essai sans écran ; panneau en démonstration | à faire |
| 4 | KDE et GNOME : raison affichée sous un réglage grisé (suivi de tête, audio spatial, niveau) | tests du cœur ; essais d'interface | à faire |
| 5 | Barres d'état : exemples à partir de `ctl barre` | sortie de `ctl barre` relue contre chaque exemple | à faire |
| 6 | Documentation : parité, feuille de route, journal des versions | vérificateur de liens | à faire |

## Décisions

- **Le cœur d'abord.** Toute la logique (analyse des gains, sauvegarde,
  préférences du panneau, raisons d'indisponibilité) existe déjà dans
  `momentum-core` et sert au client KDE. Le client GNOME la branche ; il ne
  la réécrit pas.
- **Compilation du client GNOME** : sur le PC fixe, dans la boîte
  `momentum-dev` (GTK 4.22, libadwaita 1.9). Windows ne compile pas GTK.
- **Une PR par lot**, fusionnée quand la CI est verte.

## Constats d'inventaire (12/11/2026)

Absents du client GNOME et présents dans le client KDE, en plus de la liste
de `docs/PARITE.md` : les notifications au choix, le journal local
(consultation, export, effacement), l'effacement de l'historique de
batterie, l'avis de nouvelle version et l'assistant de premier lancement.
Les trois premiers entrent dans le lot 2. Les deux derniers sont à décider
après le lot 3 (l'avis demande un client HTTP que le client GNOME n'a pas).

## Journal

- 12/11/2026 — Inventaire fait, plan écrit, branche `chantier-1.9`.
- 12/11/2026 — Lot 1 : compilé dans la boîte `momentum-dev` (clippy sans avis), essai
  sans écran réussi, avec l'étape nouvelle « gains saisis en texte relus ».
  L'export APO n'a pas d'étape d'essai : il écrit dans le dossier Documents.
- 12/11/2026 — Lot 2 : essai sans écran réussi avec un dossier de données
  temporaire (`XDG_DATA_HOME`). Captures des pages Mon son, Batterie et À propos
  relues : mise en page correcte. Les fenêtres de choix de fichier et les
  confirmations d'effacement ne sont pas essayées automatiquement.
  Vu au passage, non corrigé : la page À propos affiche deux fois Modèle et
  Micrologiciel (`about_rows` puis `device_rows`).
