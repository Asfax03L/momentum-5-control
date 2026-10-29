# Accueil WinUI : charge, port et disponibilité

Lot 1 du 28 octobre 2026. L'interface native Rust / windows-reactor conserve
les commandes typées et le service existants. Aucune commande GAIA n'est
ajoutée ou modifiée.

## Présentation

L'accueil regroupe un bandeau d'identité et de batterie, puis les cartes
Contrôle du bruit, Appareils, Audio spatial et Au quotidien. Un raccourci
ouvre l'égaliseur et l'A/B existants. L'affinage du 29 octobre 2026 reprend
la hiérarchie de la maquette SonyBridge : bandeau horizontal illustré,
indicateurs regroupés, modes de bruit dans un sélecteur et cartes compactes.
Les couleurs et les contrôles restent natifs et suivent le thème Windows.

L'[illustration originale](../frontends/winui/assets/README.md) représente
un casque générique sans marque. Elle est intégrée à l'exécutable et
n'est pas une photographie du matériel. Aucune ressource Sony ou
Sennheiser n'est distribuée.

Les badges batterie, flux audio et port présentent uniquement les données
du service. L'état de charge est conservé séparément. La liste distingue
les appareils connectés et les appareils appairés non connectés, sans
modifier les actions existantes ni déduire une source audio du Bluetooth.

Le mode actif est nommé dans l'accessibilité. Les modes sont des boutons
natifs utilisables au clavier ; les interrupteurs conservent leur nom et
leur état accessibles sans répéter Activé / Désactivé dans chaque carte.

Les cartes passent de deux colonnes à une sous 1000 DIP de largeur cliente.
Le seuil tient compte du DPI de la fenêtre. La navigation conserve les sept
pages existantes et utilise le volet compact natif. Un redimensionnement ne
déclenche aucun accès au casque ni minuterie de lecture supplémentaire.

## Informations réellement connues

- Le pourcentage et l'état de charge sont indépendants. Même à 100 %, une
  lecture « en charge » reste affichée comme telle.
- Une valeur de charge inconnue reste inconnue, notamment le code 02 dont
  la signification physique n'a pas été observée sur ce matériel.
- Porté / non porté désigne l'état physique lu ; il n'est pas déduit de
  Smart Pause ou de l'activation du capteur.
- Les champs absents sont « non disponibles », sans reprendre une ancienne
  session ni introduire des données de démonstration dans une session réelle.
- La page Batterie montre les états courants à part de l'historique. Elle
  ne présente plus l'état de charge comme une donnée jamais lue par le projet.

## Réglages indisponibles

Le texte distingue casque déconnecté, valeur absente, lecture seule,
nécessité du mode Personnalisé, dépendance à l'audio spatial et exclusion
par le profil personnalisé. Les raisons apparaissent à côté des contrôles.
Une valeur absente ne devient pas un interrupteur « désactivé ».

Les snapshots actuels ne distinguent pas toujours une lecture non terminée,
un refus et une valeur rejetée. L'interface ne prétend donc pas connaître le
motif exact et indique « Valeur non disponible pour cette connexion ».
Un registre détaillé des causes par réglage constitue un chantier séparé.

Les gardes existantes de spatial, suivi de tête et commandes tactiles sont
conservées et comparées dans les tests. Le curseur ANC / transparence est
disponible uniquement en mode Personnalisé. Les effets d'un clic passent
toujours par le service et ses relectures.

## Vérification

Huit scénarios synthétiques couvrent notamment charge à 100 %, codes
inconnus, absence de session, indépendance de Smart Pause, disponibilités
partielles et équivalence aux gardes existantes. Ils ne constituent pas
des essais matériels.

La compilation WinUI et la suite de tests par défaut ont été exécutées sous
Windows. Les essais visuels utilisent exclusivement `momentum-winui --demo`
avec un dossier de données séparé, sans service ni connexion Bluetooth.
La démonstration possède sa propre instance et ne réserve aucun raccourci
global, pour coexister avec l'application installée sans la réveiller.
La navigation, le changement de mode, un niveau simulé, l'anti-vent, le suivi
de tête conditionnel et le redimensionnement ont été contrôlés. Les retours
matériels et les autres frontends ne sont pas qualifiés par ces essais.

## Notes de publication proposées

**En bref :** sous Windows, l'accueil rassemble les réglages du quotidien
dans des cartes adaptables à la largeur. Charge et état porté sont visibles
quand le service les fournit ; les contrôles indisponibles sont expliqués.

**À faire après la mise à jour :** relancer l'interface Windows pour charger
le nouvel accueil. Aucune migration de données ni modification des réglages
du casque n'est nécessaire. Un ancien service ne fournissant pas les deux
champs optionnels laissera leurs indicateurs « non disponibles ».
