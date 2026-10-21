# Plan : scènes et canal local entre `ctl` et l'icône

Étape 2 de la roadmap. Aucune nouvelle commande du casque : uniquement des
écritures déjà capturées et validées sur le M5.

## 1. Scènes

Une scène est un ensemble nommé de réglages appliqués d'un coup :
« Transport » = ANC maximal et anti-vent, par exemple.

**Fichier** `%LOCALAPPDATA%\momentum-ctl-win\scenes.txt`, créé avec trois
exemples s'il n'existe pas, modifiable à la main :

```ini
# Lignes « réglage = valeur » sous un titre [Nom de la scène].
[Transport]
bruit = personnalise
niveau = 0
anti-vent = on
```

**Réglages acceptés** (liste fermée) :

| Clé | Valeurs | Commande validée |
|---|---|---|
| `son` | `egaliseur`, `voix`, `profil` | `0803` (mode audio) |
| `spatial` | `on`, `off` | `3604` |
| `bruit` | `adaptatif`, `personnalise`, `arret` | séquences de l'app (`1A04`, `1A00 [03, …]`) |
| `niveau` | 0 à 100 (0 = ANC max, 100 = transparence max) | `1A02`, mode Personnalisé |
| `anti-vent` | `on`, `off` | `1A00 [01, état]` |
| `tactile` | `on`, `off` | `1606` |
| `egaliseur` | nom d'un préréglage (de l'app ou personnel) | `1001` bande par bande |
| `basses` | `on`, `off` | `1008` |

**Exclus** : priorité du mode audio (redémarrage), langue des invites (pack
de voix), profil auditif (seule la sélection du profil existant est
permise), connexions, nom, volume d'écoute.

**Cohérence vérifiée à la lecture du fichier** (scène refusée sinon) :
- `niveau` exige `bruit = personnalise` ou pas de `bruit` (il l'impose) ;
- `egaliseur` et `basses` exigent `son = egaliseur` dans la même scène ;
- `spatial = on` est incompatible avec `son = profil` (l'app coupe Dolby
  Atmos pour le profil) ;
- une clé inconnue, une valeur invalide ou une clé en double refusent la
  scène, avec le numéro de ligne.

**Application** : ordre fixe, quel que soit l'ordre des lignes (mode audio,
spatial, bruit, niveau, anti-vent, tactile, égaliseur, basses). Une étape à
la fois ; **arrêt à la première erreur**. Ce n'est pas une transaction : le
rapport dit quelles étapes sont appliquées, laquelle a échoué et lesquelles
n'ont pas été tentées. L'icône relit ensuite l'état du casque. Si une scène
active l'audio spatial sans fixer `son` alors que le profil est sélectionné,
l'étape échoue avec une explication plutôt que d'être envoyée.

**Accès** : sous-menu « Scènes » de l'icône, boutons dans l'Accueil,
`ctl scenes` et `ctl scene <nom>`.

## 2. Canal local `ctl` ↔ icône

Aujourd'hui, `ctl` ouvre sa propre connexion RFCOMM : il faut quitter
l'icône. Avec le canal, le worker de l'icône reste **le seul propriétaire**
de la connexion GAIA, et `ctl` lui transmet une demande.

- **Named pipe** `\\.\pipe\momentum-ctl-win-<SID de l'utilisateur>`,
  créé par l'icône avec :
  - un descripteur de sécurité qui n'autorise **que l'utilisateur courant** ;
  - `PIPE_REJECT_REMOTE_CLIENTS` (pas d'accès réseau) ;
  - `FILE_FLAG_FIRST_PIPE_INSTANCE` (si le nom est déjà pris, l'icône ne sert
    pas le canal plutôt que de le partager).
- **Protocole texte versionné**, une requête par connexion : une ligne
  `v1 <requête>` (1 Kio au plus), réponse `OK` ou `ERREUR <raison>` puis le
  contenu, terminée par un octet nul. Le client lit jusqu'à cet octet puis
  ferme ; le serveur attend cette fermeture (5 s au plus) avant de
  déconnecter, sinon la réponse non lue serait perdue.
- **Liste fermée de requêtes** : `etat` (JSON), `etat-texte`, `scenes`
  (noms), `scene <nom>` (rapport d'application). Aucune commande arbitraire ni trame brute.
- **JSON d'état, version 1** : batterie, bruit, audio spatial, mode audio,
  commandes tactiles, codec, fréquence ; `null` pour une valeur non lue ;
  `"connecte": false` si le casque est absent. Ni nom, ni adresse.
- **`ctl`** : `etat`, `scenes` et `scene` passent par le canal si l'icône
  tourne, sinon par une connexion directe. Les autres sous-commandes
  refusent clairement de s'exécuter quand l'icône tourne, au lieu d'échouer
  sur une connexion RFCOMM occupée. `ctl etat --json` donne le même JSON
  dans les deux cas.
- **Délais** : lecture et écriture du client 5 s ; état 10 s ; scène 60 s.
  Casque absent : `"connecte": false` pour l'état, `ERREUR casque non
  connecté` pour une scène.

## 3. Tests

- Synthétiques (CI) : analyse et validation des scènes, ordre d'application,
  arrêt à la première erreur et rapport partiel (transport simulé), analyse
  des requêtes du canal, JSON avec `null`.
- Sur le PC : canal créé, `ctl etat --json` et `ctl scene` pendant que
  l'icône tourne, refus des autres sous-commandes.
- Sur le M5 : une scène appliquée et relue, puis retour à l'état initial.

## 4. Hors de cette étape

Raccourcis clavier par scène, édition des scènes dans la fenêtre, état
poussé en continu aux clients (abonnement), panneau rapide.
