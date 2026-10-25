# Questions fréquentes

*[English version](FAQ.en.md) · [Guide d'utilisation](GUIDE.md)*

### Est-ce une application officielle de Sennheiser ?

Non. OpenHeadset Control est un projet indépendant, sans lien avec
Sennheiser ni Sonova. Le protocole a été reconstitué à partir de
documentation publique et de captures de l'application officielle
([MOMENTUM5.md](MOMENTUM5.md)).

### Quels casques sont pris en charge ?

Le Sennheiser MOMENTUM 5 (et son micrologiciel testé : voir le tableau de
compatibilité du [README](../README.md#compatibilité-momentum-5)). Un autre
modèle n'est pas pris en charge ; le formulaire « Compatibilité » du dépôt
permet de signaler un essai.

### Peut-on mettre à jour le micrologiciel ou réinitialiser le casque ?

Non, et ce n'est pas prévu : ni mise à jour du micrologiciel, ni
réinitialisation d'usine, ni aucune écriture qui n'a pas été validée sur le
casque. Utiliser l'application officielle pour cela.

### Pourquoi un réglage est-il grisé ou « indisponible » ?

L'interface se construit d'après ce que le casque répond. Un réglage qu'il
n'a pas communiqué, ou qui n'est pas encore validé, reste en lecture seule
ou indisponible plutôt que d'être deviné. Le diagnostic (À propos) dit
quels réglages ont été lus.

### Le téléphone peut-il rester connecté en même temps ?

Oui, avec le **Multipoint** : le casque reste connecté à deux appareils, par
exemple le PC et le téléphone. Couper le Multipoint déconnecte l'autre
appareil (une confirmation est demandée).

### Et avec deux PC ?

Avec le Multipoint, le casque peut être connecté à deux PC, et OpenHeadset
Control peut tourner sur chacun.

### Windows ou Linux ne trouve pas le casque.

1. Vérifier que le Bluetooth est activé et le casque allumé.
2. Vérifier qu'il est **appairé** dans les paramètres Bluetooth du système :
   l'application n'appaire rien elle-même.
3. Le bouton « Reconnecter le casque » (Accueil, menu de l'icône) demande
   une reconnexion au système pour un casque déjà connu.
4. En double démarrage Windows + Linux, chaque système a sa propre clé
   d'appairage : voir [LINUX.md](LINUX.md#dual-boot-windows--bazzite--la-clé-dappairage).

### SmartScreen bloque l'installateur.

Les exécutables ne sont pas signés : SmartScreen demande une confirmation
(« Informations complémentaires » > « Exécuter quand même »). L'authenticité
d'un téléchargement se vérifie avec son empreinte et son attestation de
provenance ([VERIFIER.md](VERIFIER.md)).

### « Windows App Runtime 2 introuvable ».

L'interface WinUI en dépend. L'installer depuis
<https://aka.ms/windowsappsdk/2.0/latest/windowsappruntimeinstall-x64.exe>
ou avec `winget install Microsoft.WindowsAppRuntime.2`, puis relancer
l'installateur.

### Comment changer la langue ?

Paramètres > Langue : langue du système, Français ou English, appliquée au
prochain lancement. Pour un essai ponctuel, la variable d'environnement
`OPENHEADSET_LANG=fr` ou `en` a priorité.

### L'application envoie-t-elle des données sur Internet ?

Non. Deux exceptions, toujours à votre initiative : l'**avis de nouvelle
version**, désactivé par défaut, lit la dernière publication sur GitHub au
lancement (GitHub voit votre adresse IP, rien d'autre n'est envoyé) ; le
rapport de bogue ouvre un formulaire GitHub dans le navigateur, prérempli
avec le diagnostic expurgé que vous avez relu, et c'est vous qui
l'envoyez.

### Que contient le diagnostic ?

Versions, système, modèle, micrologiciel, quels réglages ont été lus,
incidents récents. Jamais : nom du casque ou des appareils, adresse
Bluetooth, numéro de série, dossier personnel, profil auditif.

### Où sont mes réglages ? Comment les transférer ?

Dans le dossier des données (voir le [guide](GUIDE.md#10-où-sont-mes-données)).
Paramètres > sauvegarde exporte et importe un fichier JSON, après un aperçu
de ce qui changera.

### Fonctionne-t-il sous GNOME, sur le Steam Deck ?

Sous GNOME, un client GTK existe mais reste partiel (page Accueil). En mode
jeu (Steam Deck, Bazzite), l'extension Decky Loader donne batterie,
réduction de bruit, anti-vent et scènes.

### Est-il sur Flathub, winget, l'AUR ?

Pas encore : les soumissions sont préparées
([SOUMISSIONS.md](SOUMISSIONS.md)). En attendant, les fichiers de la page
Releases (installateur, Flatpak, `.deb`, `.rpm`, `PKGBUILD`).

### Que garantit la version 1.0 ?

Les fichiers de réglages, la sauvegarde JSON, la ligne de commande `ctl`
et l'API D-Bus restent compatibles pour toute la série 1.x : voir la
[politique de versions](VERSIONS.md). Une faille se signale en privé
([SECURITY.md](../SECURITY.md)).

### Comment signaler un problème ou poser une question ?

Un bogue : À propos > « Signaler un problème… ». Une question, une idée :
[Discussions](https://github.com/Asfax03L/openheadset-control/discussions).
