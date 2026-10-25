# Parité des interfaces (après le retrait de Slint)

État au 22/10/2026 (`main`). ✅ disponible · 🟡 partiel ou différent ·
— absent (avec la raison). Toutes les interfaces passent par le service :
aucune ne parle au casque elle-même.

## Réglages du casque

Les deux interfaces envoient la même liste de commandes de l'API
(`momentum_client::Command`) ; `ctl` en couvre l'essentiel en ligne de
commande ; le widget Plasma se limite aux réglages rapides.

| Fonction | WinUI (Windows) | Client KDE | `ctl` | Widget Plasma |
|---|---|---|---|---|
| Mode de bruit, curseur ANC ↔ transparence, anti-vent | ✅ | ✅ | ✅ | ✅ |
| Audio spatial, suivi de tête | ✅ | ✅ | ✅ | ✅ |
| Égaliseur 8 bandes, basses, remise à plat | ✅ | ✅ | ✅ | — |
| Préréglages, courbes (export, import), A/B | ✅ | ✅ | — | — |
| Mode audio : Speech Clarity, profil personnalisé | ✅ | ✅ | ✅ | 🟡 profil personnalisé seul |
| Smart Pause, réponse automatique, appels confortables | ✅ | ✅ | ✅ | — |
| Extinction automatique, protection de la batterie | ✅ | ✅ | ✅ | — |
| Invites, volume des invites, nom du casque | ✅ | ✅ | ✅ | — |
| Commandes tactiles | ✅ | ✅ | ✅ | — |
| Multipoint, appareils appairés (connecter, déconnecter) | ✅ | ✅ | ✅ | — |
| Scènes : application | ✅ | ✅ | ✅ | 🟡 celles du panneau rapide |
| Export du profil auditif (fichier privé) | ✅ | ✅ | ✅ | — |

## Intégration au bureau

| Fonction | WinUI (Windows) | Client KDE | Remarque |
|---|---|---|---|
| Icône de la zone de notification et menu | ✅ | ✅ | même menu (`momentum_core::tray_menu`) |
| Panneau rapide au clic sur l'icône | ✅ | ✅ | même contenu (`panneau.txt`) ; sous Wayland, KWin place le panneau |
| Éditeur de scènes | ✅ | ✅ | |
| Raccourcis clavier globaux | ✅ éditeur | ✅ KGlobalAccel | sous KDE, modifiables dans Configuration du système |
| Batterie : courbe 24 h / 7 j / 30 j, export, effacement | ✅ | ✅ | historique tenu par le service |
| Connexion et audio : lectures et heures | ✅ | ✅ | |
| Vérification de la sortie audio du système | ✅ | ✅ | Linux : PipeWire, lecture seule (depuis 0.10) |
| Thème clair ou sombre | ✅ | ✅ | sous Plasma : jeu Breeze clair ou sombre pour l'application seule |
| Lancement à l'ouverture de session | ✅ | ✅ | Flatpak : portail du bureau |
| Une seule instance | ✅ | ✅ | |
| Notifications (batterie, connexion) | ✅ | ✅ | envoyées par le service |
| Diagnostic expurgé à copier | ✅ | ✅ | |
| Mode démonstration (`--demo`) | ✅ | ✅ | casque simulé, aucun service |
| Refus et résultats des demandes affichés | ✅ | ✅ | suivi par numéro de demande (aussi dans le widget Plasma) |

## Client GNOME (GTK 4, libadwaita)

`momentum-gtk` (jalon 0.12) : client du même service, comme le client KDE,
sans connexion directe au casque ; mêmes modèles de vue
(`momentum_core::viewmodel`) et même suivi des demandes. Référence de
parité : le **client KDE**. Première étape livrée en 0.12 ; les suivantes
reprennent les pages du client KDE dans cet ordre.

| Fonction (client KDE) | Client GNOME | Étape |
|---|---|---|
| Batterie, mode de bruit, curseur ANC ↔ transparence, anti-vent | ✅ | 1 (0.12) |
| Audio spatial | ✅ | 1 (0.12) |
| Demandes suivies, refus et échecs affichés (notification) | ✅ | 1 (0.12) |
| Mode démonstration (`--demo`), entrée de menu, une seule instance | ✅ | 1 (0.12) |
| Suivi de tête, commandes tactiles, multipoint | — | 2 |
| Appareils appairés (connecter, déconnecter), reconnexion | — | 2 |
| Égaliseur 8 bandes, basses, préréglages, courbes, A/B | — | 3 |
| Mode audio : Speech Clarity, profil personnalisé | — | 3 |
| Paramètres : Smart Pause, réponse automatique, appels confortables, extinction, protection de la batterie, invites, nom | — | 4 |
| Scènes : application, éditeur, enregistrement de l'état | — | 5 |
| Batterie (courbe, export), connexion et audio (PipeWire), à propos et diagnostic | — | 6 |
| Thème clair ou sombre, lancement à l'ouverture de session | — | 6 (thème : suit déjà GNOME) |
| Icône de la zone de notification, panneau rapide, raccourcis globaux | — | à décider : GNOME n'a pas de zone de notification standard |

Distribution : dans l'archive Linux et installé par `installer-linux.sh`
(entrée « OpenHeadset Control (GTK) ») ; pas encore dans les paquets
`.deb` et `.rpm` ni dans un Flatpak GNOME.

## Distribution

| Paquet | Contenu |
|---|---|
| Archive Windows | WinUI, service, `ctl`, `probe`, `installer.ps1` |
| Archive Linux | client KDE, client GNOME (`momentum-gtk`), service, `ctl`, widget Plasma, recherche KRunner, `installer-linux.sh` |
| Flatpak | client KDE, service, `ctl` (le widget s'installe à part) |
