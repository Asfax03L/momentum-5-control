# OpenHeadset Control 1.2.0 — Windows uniquement

## En bref

L'accueil Windows reprend la hiérarchie de la maquette SonyBridge avec des
contrôles WinUI natifs : bandeau illustré, indicateurs regroupés, sélecteur
de réduction du bruit, niveau ANC / transparence mis en avant et cartes
adaptées à la largeur de la fenêtre. Les thèmes clair et sombre sont pris
en charge. L'illustration originale représente un casque générique ; ce
n'est pas une photographie officielle du MOMENTUM 5.

- Batterie, charge déclarée, état porté et activité audio sont présentés
  séparément quand le service fournit ces informations. Une donnée absente
  reste indisponible ; 100 % ne signifie pas « charge terminée ».
- Les appareils connectés et les appareils appairés non connectés sont
  regroupés, avec les actions et confirmations existantes.
- Les réglages indisponibles sont expliqués : déconnexion, lecture seule,
  donnée absente, mode Personnalisé ou dépendance à l'audio spatial.
- Smart Pause, commandes tactiles et accès à l'égaliseur / comparaison A/B
  restent accessibles depuis l'accueil.
- La démonstration peut coexister avec l'application installée et ne réserve
  plus ses raccourcis clavier globaux.

Cette publication embarque également le travail préparatoire de lecture
du port et de la charge, son registre de preuves et les outils d'essai
ciblés. Le sidetone préparé dans le cœur n'ajoute aucun bouton à l'accueil.
Les outils d'essai ne sont pas inclus dans les téléchargements.

## À faire après la mise à jour

1. Quitter OpenHeadset Control depuis le menu de son icône de notification.
2. Télécharger puis lancer
   `openheadset-control-v1.2.0-windows-x86_64-installateur.exe`.
3. Relancer l'application. Installer ensemble le service et l'interface
   fournis dans cette version, pour bénéficier des nouveaux champs d'état.

L'installateur est destiné à Windows x64, s'installe pour l'utilisateur
actuel et conserve les réglages. Windows App Runtime 2 est nécessaire ;
le paquet ne contient pas de certificat de signature Authenticode.

Une archive manuelle est également fournie :
`openheadset-control-v1.2.0-windows-x86_64.zip`. Elle contient l'interface,
le service, `ctl`, `probe`, les licences et `installer.ps1`.

## Périmètre et validation

Publication Windows uniquement, sous le tag `windows-v1.2.0`. Aucun paquet
Linux, Flatpak, Plasma ou Decky n'est publié pour cette version. La dernière
publication multiplateforme reste inchangée ; cette version Windows n'est
pas désignée comme « Latest » pour éviter de rediriger les utilisateurs
Linux vers une publication sans paquet Linux.

Le workflow Windows vérifie formatage, clippy, tests Rust avec données
témoin, confidentialité, démarrage WinUI en démonstration, navigation et
accessibilité en français et anglais, puis installation, réinstallation et
désinstallation sur le runner. Les fichiers sont accompagnés d'empreintes
SHA-256 et d'une attestation de provenance GitHub. La publication est
conditionnée au succès de ces contrôles.

Les captures locales de l'accueil ont été contrôlées en thèmes clair et
sombre, à grande largeur et à 422 pixels de large. Ces essais d'interface
ne constituent pas une nouvelle qualification matérielle. La compatibilité
et les garde-fous modèle / micrologiciel du projet restent applicables.
