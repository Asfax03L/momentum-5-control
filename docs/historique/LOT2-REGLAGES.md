# Lot 2 — réglages et interactions fiables

État au 29 octobre 2026. Les changements logiciels utilisent les commandes
déjà présentes. Aucun nouvel essai Bluetooth matériel n'est revendiqué.

## Intentions et relectures

`momentum-client::edits` conserve séparément la valeur demandée et la valeur
relue. Une lecture ancienne ne remplace pas une intention plus récente.
Une intention envoyée disparaît à la fin des demandes de cette interface,
qu'elles réussissent ou échouent ; une intention plus récente non envoyée
reste en attente. Le suivi numéroté des résultats reste en place.

WinUI conserve son délai de regroupement ; KDE envoie à la fin du geste ;
GTK suit les valeurs de son curseur. Les contrôles montrent la demande,
avec un texte distinct donnant la lecture. Leur zone d'état garde une
hauteur réservée pour ne pas déplacer les contrôles pendant un geste.
Les listes de bandes restent indexées de façon stable.

Une autre session, une perte d'écriture autorisée, un changement de mode
audio ou de topologie annule les intentions locales. Les gestes WinUI et
KDE portent un numéro de contexte : un relâchement ancien n'écrit pas dans
une nouvelle connexion. Un changement de mode annule le niveau différé ;
une courbe complète annule les anciennes intentions de bandes.

Le service relit le groupe de bruit avant un niveau précédé, dans le même
lot, d'un changement de mode ou d'une scène. Les lectures de fin de lot
et la confirmation du résultat sur les états relus sont conservées.

## Préréglages et comparaison A/B

Une topologie décrit modèle, fréquences ordonnées et limites de gain. Les
fréquences utilisées sont **celles documentées pour le M5**, déjà affichées
par le projet, et non une nouvelle mesure GAIA. Aucune commande de lecture
de fréquences n'a été inventée. Les bornes et le nombre de bandes viennent
de l'état d'égaliseur lu. Une autre topologie, même avec huit bandes, est
refusée.

Un enregistrement personnel associe la courbe à cette topologie. Le format
historique `nom = gains` reste lisible ; les métadonnées JSON sont écrites
dans des commentaires `# topologie = ...`. Les anciennes versions peuvent
encore lire les lignes de gains. Une ancienne version qui réenregistre le
fichier peut perdre les métadonnées : la version récente redemandera alors
une association explicite.

Les anciens préréglages restent conservés. Dans les pages Égaliseur de
WinUI et KDE, le bouton d'association enregistre les fréquences documentées
et les limites courantes pour les anciennes courbes compatibles, sans
modifier le casque. Les courbes incompatibles restent dans le fichier.
L'écriture conserve la sauvegarde `.bak` habituelle et relit le fichier
pour préserver les ajouts d'une autre interface.

Les scènes vérifient le préréglage avant de modifier un réglage, puis
relisent l'égaliseur à l'étape d'application. Cela ajoute une lecture EQ
de précontrôle aux scènes qui utilisent un préréglage. Les courbes A/B
sont liées à la topologie et abandonnées à une nouvelle session ou à un
échec d'une demande, pour éviter une comparaison périmée.

## Sidetone — prérequis toujours ouvert

Le registre `docs/preuves/registre-m5.json` confirme les captures de
l'application pour `0806 → 0906`, `0805 → 0905` et `0886`, valeurs 0/1/2.
Le compte rendu disponible de l'application précise que le rejeu PC
n'était pas effectué. Aucun compte rendu PC daté avec relecture et
restauration n'a été retrouvé pour cette version ; aucune preuve
supplémentaire n'a été ajoutée.

L'intégration de l'écriture sidetone dans le service, la CLI et les
interfaces reste donc différée. Le lecteur de diagnostic et l'outil
ciblé déjà préparés restent en place ; aucun nouveau contrôle actif n'est
exposé. La qualification exige identité, lecture initiale, niveaux bornés,
relectures et restauration depuis le PC. Les captures de l'application
et les tests synthétiques ne remplacent pas cette étape.

## Vérifications attendues avant publication

Tests synthétiques de dernière intention, résultat tardif ou étranger,
refus, reconnexion, bandes indépendantes, changement de limites et de
fréquences, conservation des anciens préréglages et refus de scène avant
toute trame. Compilation, fmt, clippy, confidentialité, contrôles natifs
en démonstration et CI Windows / Linux restent distincts des essais matériels.
