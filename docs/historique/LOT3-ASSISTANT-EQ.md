# Lot 3 — assistant EQ guidé

Version 1.4.0, 29 octobre 2026. Disponible dans **Égaliseur** sous Windows
(WinUI) et Linux KDE, y compris Flatpak et paquets natifs. GTK conserve son
périmètre actuel : sa page Égaliseur n'est pas encore implémentée. Le
diagnostic BTD 700 reste hors de ce lot : aucun dongle n'est utilisé pour
le besoin retenu.

## Parcours

1. Connecter le M5 autorisé en écriture et sélectionner **Égaliseur graphique**.
   Attendre la fin des réglages en cours, puis **Commencer les comparaisons**.
2. Une copie de la courbe initiale est écrite et synchronisée sur disque dans
   `assistant-eq/initiale-*.txt`, sous le dossier de données habituel de
   l'application. Un fichier distinct est créé à chaque démarrage. Si cette
   copie échoue, le parcours ne démarre pas et rien ne part au casque.
3. Comparer les graves (90 et 325 Hz), médiums (1,5, 2,5 et 4,5 kHz), puis
   aigus (8, 13 et 16 kHz). **Moins**, **Référence de l'étape** et **Plus**
   comparent trois variantes autour de la courbe retenue à l'étape précédente.
   Chaque groupe varie de −1 ou +1 dB, dans les limites lues. Cliquer plusieurs
   fois sur Plus n'accumule pas les boosts. La référence initiale n'est pas
   arrondie et les fréquences sont celles déjà documentées pour le M5.
4. **Garder ce choix et continuer** ne produit aucune écriture supplémentaire.
   Le résultat final peut être comparé à la courbe initiale. Pour l'enregistrer,
   réécouter le résultat, attendre sa relecture et choisir un nouveau nom.
   Un nom existant est refusé, y compris s'il vient d'être ajouté par une autre
   interface. Les métadonnées de topologie sont conservées.
5. **Annuler et restaurer la courbe initiale** remet les gains initiaux et attend
   leur confirmation. Si ces gains sont déjà relus sans incident, aucun nouvel
   envoi n'est nécessaire.

Aucun son n'est lancé ou généré. L'utilisateur choisit sa musique et son volume.
Il s'agit d'une aide au choix d'égalisation, sans mesure auditive, sans collecte
de profil auditif et sans reproduction de la personnalisation propriétaire.
Les basses renforcées et le mode audio ne sont pas modifiés par le parcours.

## Confirmation et interruption

Le moteur commun `momentum-client::sound_check` ne demande que
`Command::SetEqGains`, déjà existante. Aucune commande GAIA, aucun UUID et
aucune variante du protocole ne sont ajoutés. Le service conserve son contrôle
du modèle et du micrologiciel, la sérialisation et les relectures.

Un essai attend **son numéro de demande**, un résultat `Applied` ou
`AlreadySatisfied`, et la courbe effectivement relue. Une notification seule,
un accusé seul, un résultat étranger ou une courbe partiellement appliquée
n'autorisent pas la progression. Le cas où le résultat arrive avant le numéro
est traité. Aucun échec ne déclenche de réessai automatique.

Une autre session, un autre casque, une perte de connexion ou du droit d'écrire,
un changement de mode audio, de basses renforcées ou de topologie interrompt
le parcours. Aucune restauration ne part dans le nouveau contexte. La courbe
initiale reste visible et la copie locale reste disponible. Un changement
extérieur des gains interrompt aussi les comparaisons ; dans le même contexte,
une restauration explicite reste possible. Après un échec de restauration,
il faut décider explicitement de restaurer à nouveau ou de fermer sans
restauration ; aucun succès n'est annoncé à tort.

Les autres commandes de la même interface, y compris menus et raccourcis, sont
bloquées pendant le parcours. Cela ne verrouille pas les autres interfaces,
le téléphone ou les scènes automatiques du service. Ne pas les utiliser pour
modifier le son pendant une comparaison. Une scène extérieure qui change
les gains est détectée lors de la relecture.

Fermer normalement la fenêtre conserve l'application en arrière-plan lorsque
son icône est disponible. Quitter normalement est refusé tant que le parcours
n'est pas terminé ou fermé explicitement. Un arrêt forcé du processus ou du PC
ne peut pas restaurer le casque : la dernière courbe appliquée peut rester
active. La copie initiale est importable par **Importer une courbe…**, après
vérification du casque et du mode. Cet import est une action manuelle soumise
aux contrôles habituels ; aucune reprise automatique n'est proposée.

Les copies de secours ne sont ni envoyées ni supprimées automatiquement et ne
sont pas incluses dans l'export JSON des réglages. Les préréglages enregistrés
par l'assistant suivent le format existant et son export habituel.

## Qualification

Les tests synthétiques couvrent les trois groupes, la comparaison finale,
les bornes, l'absence de cumul, les résultats tardifs ou étrangers, les refus,
les changements de contexte, l'enregistrement et la restauration. Les tests
de fichiers vérifient les copies distinctes et le refus d'écraser un nom.
L'essai QML en démonstration parcourt les trois groupes et restaure les huit
gains ; le contrôle natif WinUI utilise également des données isolées.

Ces vérifications logicielles ne qualifient pas une séance d'écoute réelle.
Aucun nouvel essai matériel n'est revendiqué. Le sidetone reste différé selon
le [lot 2](LOT2-REGLAGES.md). Le parcours n'utilise que des écritures EQ déjà
qualifiées ; il ne modifie pas leur qualification matérielle.
