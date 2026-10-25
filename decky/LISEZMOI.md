# OpenHeadset Control pour Decky Loader (mode jeu)

Panneau du menu d'accès rapide (bouton « … ») en mode jeu de Bazzite et
du Steam Deck : batterie, réduction de bruit (ANC maximale, adaptative,
transparence, désactivée), anti-vent et scènes du casque Sennheiser
MOMENTUM 5. Projet non officiel, sans lien avec Sennheiser ni Sonova.

## Prérequis

- [Decky Loader](https://decky.xyz) installé (procédure de Bazzite ou
  du site de Decky).
- OpenHeadset Control installé pour le même utilisateur : archive Linux
  (`~/.local/bin/ctl`), paquet (`/usr/bin/ctl`) ou Flatpak
  `io.github.asfax03l.openheadset-control`.

## Installation

Dans les réglages de Decky, activer le mode développeur (« Developer
mode »), puis, dans l'onglet développeur, « Install Plugin from ZIP » et
choisir `openheadset-control-vX.Y.Z-decky.zip` (libellés de Decky, en
anglais ; à valider en mode jeu). Mise à jour : même chose avec la
nouvelle archive. Désinstallation : menu de l'extension dans Decky.

## Fonctionnement

L'extension n'ouvre aucune connexion au casque : son backend appelle
`ctl --json --service-requis`, client de l'API du service
`momentum-daemon`. Elle reçoit donc les mêmes refus que les autres
interfaces (casque absent, lecture seule, scène inconnue) et n'affiche
qu'un état relu sur le casque. Seules les actions du panneau sont
acceptées par le backend.
