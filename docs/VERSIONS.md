# Politique de versions

À partir de la **1.0.0**, OpenHeadset Control suit le
[versionnage sémantique](https://semver.org/lang/fr/) : `MAJEUR.MINEUR.CORRECTIF`.

- **CORRECTIF** (1.0.1) : corrections seulement, sans nouvelle fonction ni
  changement d'interface publique.
- **MINEUR** (1.1.0) : nouvelles fonctions ; tout ce qui marchait avec une
  1.x continue de marcher.
- **MAJEUR** (2.0.0) : une interface publique ci-dessous change de façon
  incompatible. Ce changement est annoncé dans `CHANGELOG.md` au moins une
  version mineure avant, quand c'est possible.

## Ce que veut dire « 1.0 »

- Toutes les fonctions présentes sont **essayées sur le casque**
  (MOMENTUM 5, micrologiciel listé dans le README) ou affichées en lecture
  seule.
- Les fichiers, la ligne de commande et l'API ci-dessous sont **stables**
  pour toute la série 1.x.
- Le périmètre reste celui de la [ROADMAP](ROADMAP.md) : jamais de mise à
  jour du micrologiciel, de réinitialisation d'usine, de nouveau test
  auditif ni de suppression d'appairage.

## Interfaces publiques (stables en 1.x)

1. **Fichiers du dossier des données** : `scenes.txt`, `prereglages.txt`,
   `raccourcis.txt`, `panneau.txt`, `apparence.txt`, `langue.txt`,
   `mises-a-jour.txt`, `assistant.txt`, `notifications.txt` (1.1),
   `batterie.csv`, `journal.csv` (1.1), `casque.txt`.
   Une version 1.y lit les fichiers écrits par toute version 1.x
   antérieure ou égale. Une version mineure peut ajouter des clés ou des
   réglages ; revenir à une version plus ancienne n'est pas garanti. Les
   fichiers d'une installation 0.16 sont lus tels quels par la 1.0.
2. **Sauvegarde des réglages** (JSON `version: 1`) : une version 1.y
   importe toute sauvegarde `version: 1` faite par une 1.x antérieure ou
   égale (comme pour les fichiers, une sauvegarde plus récente qui utilise
   un réglage ajouté depuis, une scène automatique par exemple, peut être
   refusée par une version plus ancienne). Un format incompatible
   prendrait le numéro 2, refusé clairement par les versions qui ne le
   connaissent pas.
3. **`ctl`** : noms des commandes et des options, sortie `--json` en
   version `1` (une clé existante garde son nom et son sens ; des clés
   peuvent s'ajouter), codes de sortie `0` succès, `1` refus ou
   annulation, `2` non appliqué ou arguments invalides, `3` résultat
   inconnu. Le texte destiné aux humains peut changer.
4. **API D-Bus du service** (Linux) : nom `org.openheadset.Control`,
   interface `org.openheadset.Control1` sur `/org/openheadset/Control`
   (méthodes `Command`, `Submit`, `Snapshot`, `Scenes`, signal `Event`,
   propriétés `Connected`, `Battery`, `Name`). Un changement incompatible
   passerait par une interface `Control2`, offerte à côté de `Control1`
   pendant au moins une version mineure.

## Ce qui n'est pas une interface publique

Disposition et textes des interfaces graphiques, traductions, journaux,
texte du diagnostic, sortie de `probe`, canal nommé Windows entre le
service et ses clients (toujours de la même installation), crates Rust
internes (non publiées sur crates.io). Ils peuvent changer dans n'importe
quelle version.

## Préréglages depuis la 1.3.0

Les lignes historiques `nom = gains` sont conservées ; les topologies
s'ajoutent dans des commentaires. Une version récente lit les anciens
fichiers sans perdre leurs courbes, mais demande une association explicite
aux fréquences documentées du M5 et aux limites courantes avant application.
Les interfaces WinUI et KDE proposent cette action sans écriture au casque.
Une ancienne version peut retirer ces commentaires en réenregistrant le
fichier ; il faudra alors réassocier les préréglages dans la version récente.

## Micrologiciel du casque

Une nouvelle version du micrologiciel peut changer le comportement d'une
commande. Dans ce cas, la commande repasse en lecture seule jusqu'à une
nouvelle validation sur le casque : ce n'est pas une rupture de la
politique de versions, mais une correction (version CORRECTIF).

## Versions maintenues et sécurité

Seule la dernière version mineure 1.x reçoit des corrections ; une faille
corrigée sort en version CORRECTIF. Signalement des failles et délais :
[SECURITY.md](../SECURITY.md).

Pas de calendrier fixe : une version sort quand elle est prête et essayée
sur le casque.
