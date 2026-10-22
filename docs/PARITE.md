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
| Audio spatial, suivi de tête | ✅ | ✅ | ✅ | 🟡 audio spatial seul |
| Égaliseur 8 bandes, basses, remise à plat | ✅ | ✅ | ✅ | — |
| Préréglages, courbes (export, import), A/B | ✅ | ✅ | — | — |
| Mode audio : Speech Clarity, profil personnalisé | ✅ | ✅ | ✅ | — |
| Smart Pause, réponse automatique, appels confortables | ✅ | ✅ | ✅ | — |
| Extinction automatique, protection de la batterie | ✅ | ✅ | ✅ | — |
| Invites, volume des invites, nom du casque | ✅ | ✅ | ✅ | — |
| Commandes tactiles | ✅ | ✅ | ✅ | — |
| Multipoint, appareils appairés (connecter, déconnecter) | ✅ | ✅ | ✅ | — |
| Scènes : application | ✅ | ✅ | ✅ | — |
| Export du profil auditif (fichier privé) | ✅ | ✅ | ✅ | — |

## Intégration au bureau

| Fonction | WinUI (Windows) | Client KDE | Remarque |
|---|---|---|---|
| Icône de la zone de notification et menu | ✅ | ✅ | même menu (`momentum_core::tray_menu`) |
| Panneau rapide au clic sur l'icône | ✅ | — | prévu (jalon 0.9.1) ; sous KDE, le clic ouvre la fenêtre ou le widget |
| Éditeur de scènes | ✅ | ✅ | |
| Raccourcis clavier globaux | ✅ éditeur | ✅ KGlobalAccel | sous KDE, modifiables dans Configuration du système |
| Batterie : courbe 24 h / 7 j / 30 j, export, effacement | ✅ | ✅ | historique tenu par le service |
| Connexion et audio : lectures et heures | ✅ | ✅ | |
| Vérification de la sortie audio du système | ✅ | — | Linux : PipeWire prévu (jalon 0.10) |
| Thème clair ou sombre | ✅ | ✅ | sous Plasma : jeu Breeze clair ou sombre pour l'application seule |
| Lancement à l'ouverture de session | ✅ | ✅ | Flatpak : portail du bureau |
| Une seule instance | ✅ | ✅ | |
| Notifications (batterie, connexion) | ✅ | ✅ | envoyées par le service |
| Diagnostic expurgé à copier | ✅ | ✅ | |
| Mode démonstration (`--demo`) | ✅ | ✅ | casque simulé, aucun service |
| Refus et résultats des demandes affichés | ✅ | ✅ | suivi par numéro de demande |

## Distribution

| Paquet | Contenu |
|---|---|
| Archive Windows | WinUI, service, `ctl`, `probe`, `installer.ps1` |
| Archive Linux | client KDE, service, `ctl`, widget Plasma, `installer-linux.sh` |
| Flatpak | client KDE, service, `ctl` (le widget s'installe à part) |
