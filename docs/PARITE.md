# Parité des interfaces (après le retrait de Slint)

État mis à jour le 29/10/2026 pour le lot 3. ✅ disponible · 🟡 partiel ou différent ·
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
| Assistant EQ guidé, trois groupes et restauration | ✅ (1.4) | ✅ (1.4) | — parcours interactif | — |
| Mode audio : Speech Clarity, profil personnalisé | ✅ | ✅ | ✅ | 🟡 profil personnalisé seul |
| Smart Pause, réponse automatique, appels confortables | ✅ | ✅ | ✅ | — |
| Retour de voix (sidetone) : arrêt, niveaux 1 et 2 | ✅ | ✅ | ✅ | — |
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
| Choix du codec Bluetooth | — | ✅ | réglage du PC par PipeWire, aussi `ctl codec` ; pas dans le Flatpak |
| Thème clair ou sombre | ✅ | ✅ | sous Plasma : jeu Breeze clair ou sombre pour l'application seule |
| Lancement à l'ouverture de session | ✅ | ✅ | Flatpak : portail du bureau |
| Une seule instance | ✅ | ✅ | |
| Notifications (batterie, connexion) | ✅ | ✅ | envoyées par le service |
| Diagnostic expurgé à copier | ✅ | ✅ | |
| Mode démonstration (`--demo`) | ✅ | ✅ | casque simulé, aucun service |
| Barre d'état (`ctl barre`, Waybar) | — | — | Linux, hors interface : voir `LINUX.md` |
| Refus et résultats des demandes affichés | ✅ | ✅ | suivi par numéro de demande (aussi dans le widget Plasma) |

## Client GNOME (GTK 4, libadwaita)

`momentum-gtk` (jalon 0.12) : client du même service, comme le client KDE,
sans connexion directe au casque ; mêmes modèles de vue
(`momentum_core::viewmodel`) et même suivi des demandes. Référence de
parité : le **client KDE**. Première étape livrée en 0.12 ; les suivantes
reprennent les pages du client KDE dans cet ordre.

| Fonction (client KDE) | Client GNOME |
|---|---|
| Batterie, mode de bruit, curseur ANC ↔ transparence, anti-vent, audio spatial | ✅ |
| Demandes suivies, refus et échecs affichés (notification) | ✅ |
| Mode démonstration (`--demo`), entrée de menu, une seule instance | ✅ |
| Suivi de tête, commandes tactiles, multipoint | ✅ |
| Appareils appairés (connecter, déconnecter), reconnexion | ✅ |
| Égaliseur 8 bandes, basses, remise à plat, préréglages, A/B, courbes, assistant EQ | ✅ |
| Mode audio : Speech Clarity, profil personnalisé, export du profil | ✅ |
| Paramètres : Smart Pause, réponse automatique, appels confortables, sidetone, extinction, protection de la batterie, invites, volume, nom | ✅ |
| Scènes : application, éditeur, enregistrement de l'état | ✅ |
| Batterie : courbe, valeurs, autonomie, export ; connexion et audio (PipeWire), codec | ✅ |
| À propos, diagnostic, rapport de bogue, lancement à l'ouverture de session (en arrière-plan, `--cache`) | ✅ |
| Panneau rapide (batterie, mode de bruit, curseur ANC) : menu principal, action « Panneau rapide » de l'entrée de menu, `--panneau`, raccourci | ✅ (1.7) |
| Raccourcis globaux (mode suivant, profil, fenêtre, panneau, scènes) par le portail XDG `GlobalShortcuts` | ✅ (1.7 ; accepté puis utilisé sur KDE Plasma le 07/11/2026, entrée de menu renommée en 1.8 ; pas encore sous GNOME) |
| Saisie des gains en texte, export pour Equalizer APO, sauvegarde et restauration des réglages, langue, contenu du panneau, éditeur de raccourcis | — (`ctl reglages` couvre la sauvegarde) |
| Icône de la zone de notification | — (GNOME n'a pas de zone de notification standard : le panneau rapide en tient lieu) |

Toutes ces fonctions sont essayées en démonstration. Avec le casque, le 04/11/2026 (1.6.0, écran virtuel), l'affichage des vraies valeurs est essayé sur Accueil, Paramètres, Batterie et Connexion et audio ; le 07/11/2026, sur le vrai bureau KDE Plasma de Bazzite avec le casque, des réglages changés depuis la fenêtre (anti-vent, mode) ont été relus sur le casque, et le raccourci global du panneau rapide a été accepté puis utilisé.
Le thème clair ou sombre suit GNOME. Essai automatique sans écran :
`momentum-gtk --demo --essai-interface`, avec Broadway (CI, tâche `gnome`).

Distribution : dans l'archive Linux et installé par `installer-linux.sh`
(entrée « OpenHeadset Control (GTK) »), et depuis la 1.7.0 dans les paquets
`.deb` et `.rpm`. Pas dans le Flatpak : son environnement KDE n'a pas
libadwaita.

## Distribution

| Paquet | Contenu |
|---|---|
| Archive Windows | WinUI, service, `ctl`, `probe`, `installer.ps1` |
| Archive Linux | client KDE, client GNOME (`momentum-gtk`), service, `ctl`, widget Plasma, recherche KRunner, `installer-linux.sh` |
| `.deb` et `.rpm` | client KDE, client GNOME (depuis la 1.7.0), service, `ctl`, widget Plasma, recherche KRunner |
| Flatpak | client KDE, service, `ctl` (le widget s'installe à part) |
