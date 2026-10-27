# Fiche produit : Sennheiser MOMENTUM 5 (vu par GAIA)

Relevé le 08/10/2026 avec `probe --deep`, en lecture seule, sur un M5 relié à
un PC Windows 11 (build 26300) **et** à un téléphone en même temps (multipoint).

## Identité

| Élément | Valeur | Commande |
|---|---|---|
| Modèle | `M5AEBT Black` (préfixe `M5AEBT`, comme `ACAEBT` pour l'ACCENTUM) | `1206` |
| Firmware | 6.21.5 | `1201` |
| Révision matérielle | 4 (payload `00 04 00 00`) | `1200` |
| Numéro de série | 16 octets (masqué par le probe) | `001D:0003` |
| Codec actif vers le PC | 1 = AAC | `0800` |

## État physique du port — observation complémentaire du 27/10/2026

L'état physique du port est distinct du réglage d'activation de la détection
(`0401`, booléen) et de Smart Pause (`080D`). L'implémentation locale ajoute
uniquement une lecture et le traitement des notifications ; elle n'ajoute
aucune écriture de réglage ni action de lecture/pause multimédia.

| Vendor | Feature / type / numéro | Identifiant et sens | Charge utile |
|---|---|---|---|
| `0495` | 2 / 0 / 2 | `0402`, requête de lecture | vide |
| `0495` | 2 / 2 / 2 | `0502`, réponse | un octet ; `02` observé |
| `0495` | 2 / 1 / 2 | `0482`, notification | un octet ; `02` et `03` observés |
| `0495` | 0 / 0 / 7 | `0007`, abonnement | `02` (feature) |
| `0495` | 0 / 2 / 7 | `0107`, réponse à l'abonnement | vide |

Preuve : captures de Smart Control Plus **1.6.3 / 72844**, modèle **M5AEBT**,
micrologiciel **6.21.5**, corroborées par l'analyse du comportement applicatif,
sans copie de code. Le contexte physique déclaré corrobore `02 = non porté`
et `03 = porté`. Les gestes ne sont pas horodatés individuellement et des
fluctuations restent inexpliquées : aucune conclusion sur une panne du
capteur ou sur l'effet de Smart Pause.

Provenance des observations (empreintes SHA-256 des captures conservées en
privé ; aucun journal brut n'est livré dans ce dépôt) :

- **25/10/2026**, `017cfca9ac1e6d078f698004c8c7b12aff258f140b0dd0653cf7065197fc9efb` :
  requête/réponse `0402`/`0502` aux records **22342/22345** (`02`),
  notifications `0482` aux records **22273** (`02`) et **23003** (`03`).
- **27/10/2026**, `0bc5df5b4400782891f4b396666beda8841dca1432f96a8bb9ebe99ebde41251` :
  abonnement `0007 [02]` / `0107` vide aux records **40357/40360** ;
  lecture `0402` / `0502 [02]` aux records **40436/40439**.

Contrat local : `HeadsetState.on_head_state` vaut `None` si la lecture est
indisponible, `NotWorn` pour `02`, `Worn` pour `03` et `Unknown(octet)` pour
toute autre valeur. `00`, `01` et les valeurs futures ne sont pas assimilés
à un état porté/non porté. Un payload vide ou de plusieurs octets est
rejeté ; une notification malformée conserve le dernier état valide.
Après une réponse valide, les notifications de port déjà reçues avant
cette réponse sont écartées pour éviter de réafficher un état antérieur.
Les notifications suivantes et celles des autres fonctions sont conservées ;
une réponse malformée ne supprime pas une notification valide en attente.

Le service relit cet état dans son instantané et applique les notifications
`0482`. L'abonnement à la feature 2 est renouvelé à chaque connexion ; un
refus GAIA laisse les autres fonctions disponibles, mais une expiration ou
une erreur de transport fait échouer la session. Une déconnexion ou une
nouvelle connexion efface l'ancien instantané du client.

Le champ facultatif est transmis dans les événements JSON du service ; un
ancien service qui l'omet reste lisible. Le JSON historique de `ctl etat`
reste identique. Aucun indicateur graphique ni réglage supplémentaire n'est
ajouté à ce stade.

**Validation : transport simulé uniquement. Chaque échange ajouté reste
« à valider sur le matériel » depuis le PC.** Les tests vérifient le décodage,
le routage, les erreurs et la compatibilité JSON ; ils ne prouvent ni la
réception réelle des notifications sous Windows/Linux ni la pause musicale.

## Transport

- Bluetooth Classic, **RFCOMM**, service SDP nommé « GAIA »,
  UUID `A2129FF3-081B-4C45-8AFE-469D9C4842EC`.
- **Deux** enregistrements GAIA (canaux 11 et 12). Qu'il y en ait un par
  lien multipoint n'est **pas** établi. Le canal 12 a toujours fonctionné.
- Le casque est aussi appairé en **Bluetooth LE** (adresse distincte), mais
  Windows n'a joint aucun service GATT. Cet échec ne prouve pas qu'il n'existe
  aucun service GATT de contrôle : il n'a pas été exploré plus loin.

### Format des trames observé

```text
FF | version | flags | longueur (1 octet) | vendor (2) | commande (2) | payload
```

Smart Control Plus émet en version **3** ; le casque lui répond en version
3 puis **4** (après `001D:000D`), flags toujours **00**. Le projet émet en
version 3 et accepte 3 et 4 en réception ; tout flag non nul (somme de
contrôle, longueur étendue du framing Qualcomm) est refusé, car jamais
observé sur le M5. Payload limité à 254 octets (le plus long connu en fait 24).
Une première version du projet lisait `flags + longueur` comme un u16 : sans
effet sur les petites trames, mais faux en version 4 ou au-delà de 255 octets.
- Le contrôle GAIA marche **pendant que le téléphone est connecté** : pas
  besoin de couper le Bluetooth du téléphone (Smart Control fermé).
- Autres services RFCOMM : Hands-Free (canal 10) et trois UUID propriétaires
  sans nom (`00000000-DECA-FADE-…`, `DF21FE2C-…`) : non explorés, on n'y touche pas.

## Features GAIA annoncées (core Qualcomm `001D:0001`)

Réponse `00 | 0D 01 | 0C 01 | 06 02 | 04 02 | 07 01 | 00 05`, lue comme
« liste complète » puis paires `(feature, version)` : 13 v1, 12 v1, 6 v2,
4 v2, 7 v1, 0 v5. Les features 13 (ANC) et 12 (transparence) coïncident avec
la numérotation Sennheiser ; la signification des autres côté Qualcomm reste
à établir (la 6 pourrait être la mise à jour firmware : **on n'y touche pas**).

Le catalogue `0495:0001` répond aussi depuis le PC (export du 09/10/2026 : feature 16 en version 2).

Features Sennheiser (vendor `0x0495`) qui ont répondu : 2, 3, 4, 8, 9, 10, 12, 13.

Les numéros de feature et leurs versions sont propres à chaque vendor : la
feature 13 du core n'est pas automatiquement l'ANC Sennheiser, et les
versions de feature ne sont pas des versions du transport (le `00 05` final
ne prouve pas un « GAIA v5 »).

## État des réglages relevé

| Groupe | Réglage | Valeur | GET |
|---|---|---|---|
| Bruit | ANC | activé | `1A05` |
| Bruit | Transparence | désactivée, niveau 0 | `1805`, `1A03` |
| Bruit | Paires `[mode, état]` | anti-vent 1 (max), adaptatif 0, transparence 0 | `1A01` |
| Son | Égaliseur | **8 bandes**, ±6 dB, réglé : −3,5 / −1 / +1 / +0,6 / −1 / −0,5 / −0,5 / +3 dB | `1000`, `1002` |
| Son | Bass Boost | désactivé | `1009` |
| Son | Mode audio | `00 01` (1, pas podcast) | `0804` |
| Son | Faible latence | désactivée | `0818` |
| Appels | Décroché auto | activé | `080B` |
| Appels | Comfort Call | activé | `0815` |
| Appels | Sidetone | 0 | `0806` (notification `0886`). Constat initial : réglage absent de l'app pour le M5 ; **mis à jour le 27/10/2026** : page « Sidetone » observée dans Smart Control Plus 1.6.3 et écritures `0805` `02`/`01`/`02`/`00` capturées le 25/10/2026 (voir « Sidetone » plus bas) |
| Général | Smart Pause | activée | `080D` |
| Général | Détection de port | activée | `0401` |
| Général | Invites | tonalités + voix, en français | `0802`, `0807` |
| Général | Extinction auto | jamais (0 min) | `0601 [00]` |
| Connexion | Priorité | compatibilité | `0406` |
| Connexion | Appareils appairés au casque | 3 (2 connectés) | `1400`, `1401 [i]` |

## Écarts avec l'ACCENTUM / le Momentum 4

- Égaliseur à **8 bandes** (5 sur l'ACCENTUM). Fréquences centrales inconnues
  pour l'instant : à trouver avant d'afficher des libellés.
- La réponse `1002` ne répète pas l'index de bande : `[gain]` seul.
- Contrôle du bruit : modèle propre au M5, voir ci-dessous. L'interrupteur
  de transparence `1805` de l'ACCENTUM n'est pas utilisé.
- Batterie : un seul octet (pas de liste par source comme sur des écouteurs).

## Contrôle du bruit : relevé croisé avec l'app

Méthode : appui sur chaque réglage dans Smart Control Plus 1.6.3 (Android,
via ADB), puis lecture GAIA depuis le PC. L'app et le PC restent connectés en
même temps sans conflit. L'écran de l'app propose **Adaptatif / Personnalisé /
ARRÊT**, un curseur **bipolaire** (ANC à gauche, transparence à droite, neutre
au centre) en mode Personnalisé, un interrupteur **Anti-vent** et un réglage
**Audio spatial** (MARCHE/ARRÊT).

| Réglage dans l'app | `1A05` ANC | `1A01` paires | `1A03` niveau | `1805` |
|---|---|---|---|---|
| ARRÊT | 0 | anti-vent 1, adaptatif 0 | inchangé | 0 |
| Adaptatif | 1 | adaptatif **1** | inchangé | 0 |
| Personnalisé, curseur 0 % (ANC max) | 1 | adaptatif 0 | **0** | 0 |
| Personnalisé, 25 / 50 / 75 / 100 % | 1 | adaptatif 0 | **25 / 50 / 75 / 100** | 0 |
| Anti-vent décoché | — | anti-vent **0** | — | — |

Conclusion (codée dans `m5_noise_mode`) : le mode se déduit de `1A05` (marche
ou arrêt), de la paire adaptatif `(3, x)` et du niveau `1A03` (0 à 100, 50 =
neutre). Pour écrire, les candidats sont donc `1A04` (ANC), `1A00 [3, x]`
(adaptatif), `1A00 [1, x]` (anti-vent) et `1A02 [niveau]`. **Aucun n'a encore
été envoyé** : à valider un par un, en commençant par une écriture sans effet.

## Capture des trames de Smart Control Plus (journal HCI)

Méthode : journal HCI Bluetooth du Galaxy A54 activé, appuis minutés dans
l'app via ADB, journal extrait d'un `adb bugreport`, trames GAIA isolées
(btsnoop → ACL → L2CAP → RFCOMM DLCI 24). Le rapport de bug a ensuite été
supprimé ; aucune adresse, nom d'appareil ni numéro de série n'est reporté ici.
Le journal Android est horodaté en heure locale (UTC+2), pas en UTC.

### Écritures envoyées par l'app

Toutes ont ensuite été **validées depuis le PC** (`ctl`, 08/10/2026) : accusé
vide, état relu conforme, et l'app sur le téléphone suit en direct. ARRÊT
(`1A04 [00]`), seule trame déduite, est validée aussi.

| Action dans l'app | Trame envoyée | Réponse | Notification |
|---|---|---|---|
| Audio spatial ARRÊT / MARCHE | `3604 [00/01]` | `3704` vide | `3685 [00/01]`, `3683`, `3681` |
| Curseur à 50 % / 0 % | `1A02 [32/00]` puis GET `1A03` | `1B02` vide | `1A83 [niveau]` |
| Adaptatif | `1A04 [01]` puis `1A00 [03 01]` | `1B04`, `1B00` vides | `1A81` (paires) |
| Personnalisé | `1A00 [03 00]` puis `1A04 [01]` | vides | `1A81` |
| Anti-vent décoché / coché | `1A00 [01 00/01]` | `1B00` vide | `1A81` |
| (ARRÊT, relevé croisé) | `1A04 [00]` attendu | — | — |

Les réponses aux SET sont **vides** : l'accusé de réception est la réponse
`id | 0x0100`, et le nouvel état arrive par notification.

### Séquence de démarrage de l'app

1. `0495:1201` (firmware), `0495:0001` et `001D:0001` (features supportées).
2. `001D:000C` avec les valeurs **01 à 08, chacune deux fois** (16 requêtes),
   puis `001D:000D [07 00 00 00 04]` : négociation de transport probable (sens
   des champs inconnu). Ensuite le casque répond à l'app avec des trames
   **`FF 04`** (même structure). Notre session PC, sans cette négociation,
   reste en `FF 03` ; le projet ne la reproduit pas.
3. `0495:0007 [feature]` ×10 : abonnement aux notifications, **sur le vendor
   Sennheiser** (réponse `0107` vide) ; `001D:0007 [07]` pour le core.
4. Environ 60 GET, dont beaucoup d'inconnus (voir ci-dessous).

### Features Sennheiser annoncées (`0495:0001`)

`00 | 09 01 | 08 06 | 1B 01 | 03 02 | 10 02 | 04 04 | 02 01 | 0A 04 | 14 01 |
0B 02 | 0C 02 | 0D 02 | 00 05` : features 9, 8, 27, 3, 16, 4, 2, 10, 20, 11,
12, 13 et 0, avec leur version.

### Égaliseur : ce que l'app reçoit en plus

| Notification | Contenu M5 | Lecture |
|---|---|---|
| `1082` | 8 octets | gains actuels (= `1002` par bande) |
| `108B` | triplets `(bande, u16)` | **fréquences centrales : 90, 325, 1500, 2500, 4500, 8000, 13000, 16000 Hz** |
| `108D` | triplets `(bande, u16)` | 2048 pour la bande 0, 2908 ailleurs : probablement le facteur Q (hypothèse) |
| `108F` | paires `(bande, 0E)` | type de filtre ? (hypothèse) |
| `1091` | triplets à 0 | inconnu |

### Commandes inconnues lues par l'app (lecture seule, sens à établir)

| Feature | GET → réponse M5 | Piste |
|---|---|---|
| 3 | `060B` → `00` | protection de la batterie ? (désactivée dans l'app) |
| 3 | `0602` → `00 00`, `060E` → `06 FF` | ? |
| 4 | `081A` → `00 00 BB 80` | 48000 : fréquence d'échantillonnage (« 48 kHz » dans l'app) |
| 4 | `080E` → `00 F8 0D C8`, `0808` → `02`, `081E` → `FF 00 00 00 00`, `081D` → `00` | ? |
| 2 | `0402` → `03`, `0408` → `00` | `0402` : état physique porté (`03`) / non porté (`02`), distinct de `0401` ; établi le 27/10/2026, voir « État physique du port ». `0408` : ? |
| 10 | `1409` → `02`, `1414` → `[adresse, 00]` | ? (voir « Gestion des connexions » pour `140D`, `140E`, `1413`) |
| 11 | `1607` → `00` | MMI (gestes ?) |
| 16 | `2001` → `00`, `2003`, `2005`, `2007` → zéros | Sound Personalization ? |
| 20 | `2802` → nom du casque, `2803` → erreur | nom personnalisé (lecture et écriture validées, voir F3) |
| 27 | `3601` → `01 0F`, `3603` → `01`, `3605` → `01` | audio spatial (`3605` = état) |

La valeur `0406` = 1 s'affiche « Qualité audio avancée » dans l'app, alors que
le client d'Oein la traduit par « compatibilité » : à revérifier.

### Égaliseur : écritures (capture du 09/10/2026)

| Action dans l'app | Trame envoyée | Réponse | Notification |
|---|---|---|---|
| Amplification des basses | `1008 [00/01]` | `1108` vide | `1089 [état]` |
| Appliquer un préréglage | 8 × `1001 [bande, gain i8]`, une bande après l'autre | `1101` vide | `1082` (8 gains) après chaque bande |

Le nom du préréglage reste dans l'app. Écritures validées depuis le PC
(`ctl bande`, `ctl plat`, `ctl basses`, puis la fenêtre de l'égaliseur).

### Autres réglages (capture du 09/10/2026)

| Réglage dans l'app | Écriture | Réponse | Notification | Lecture |
|---|---|---|---|---|
| Speech Clarity (My sound) | `0803 [00, 02]` | `0903` vide | `0884 [00, 02]` | `0804` → `[00, mode]` |
| Retour à l'égaliseur graphique | `0803 [00, 01]` | `0903` vide | `0884 [00, 01]` | mode 1 |
| Protection de la batterie | `060A [01/00]` | `070A` vide | — | `060B` |
| Commandes tactiles désactivées / actives | `1606 [01/00]` (1 = verrou) | `1706` vide | `1687 [01/00]` | `1607` |

Speech Clarity est un **mode audio exclusif** avec l'égaliseur graphique
(le « mode podcast » 2 du client d'Oein). L'écran « Commandes » indique que
les gestes du M5 **ne peuvent pas être personnalisés** : seul l'interrupteur
général existe. Les quatre écritures ont été validées depuis le PC (`ctl`)
puis exposées dans le menu de l'icône.

### Réglages de l'appareil (capture du 09/10/2026)

| Réglage | Écriture | Réponse | Notification |
|---|---|---|---|
| Smart Pause | `080C [00/01]` | `090C` vide | — |
| Réponse automatique aux appels | `080A [00/01]` | `090A` vide | — |
| Appels confortables | `0814 [00/01]` | `0914` vide | — |
| Extinction automatique | `0600 [00, secondes u16]` (30 min = `00 07 08`) | `0700` vide | — |
| Invites audio uniquement / audio et vocales | `0801 [01/02]` | `0901` vide | `0882 [mode]` |
| Volume des invites | `081C [i8]` (77 % = `00`, 57 % = `FB`) | `091C` vide | `089D [valeur]` |

L'extinction automatique est en **secondes** : 15 et 60 min (900 et 3600)
ont été validées depuis le PC par relecture, en plus des 30 min capturées.

Non capturés volontairement :
- **Détection de port** : la désactiver coupe aussi Smart Pause, la réponse
  automatique et l'extinction automatique (confirmation dans l'app).
- **Priorité du mode audio** : capturée ensuite, voir la section suivante.

### Priorité du mode audio (capture du 09/10/2026)

Dans l'app : « Qualité audio avancée » est l'état de base ; « Haute
résolution 24 bits/96 kHz » et « Sans perte 16 bits/44,1 kHz » sont deux
**interrupteurs indépendants** ; « Appliquer et redémarrer » applique.

| Action | Trames de l'app | Notification |
|---|---|---|
| Haute résolution activée / coupée | `0405 [00]` / `0405 [01]` | `0486 [valeur]` |
| Sans perte activé / coupé | `0407 [01]` / `0407 [00]` | `0488 [valeur]` |
| Appliquer | `060F` (accusé `070F`), puis **redémarrage du casque** | — |

Lecture : `0406` (1 = Qualité audio avancée, 0 = haute résolution) et
`0408` (1 = sans perte). La lecture de `0406` d'Oein (« compatibilité »)
est donc inversée pour le M5.

Observations : après un redémarrage, le premier appareil qui rouvre GAIA
(ici le PC) peut empêcher l'app de se connecter ; en mode haute résolution
+ sans perte, l'app est restée en erreur jusqu'à ce que le casque soit
éteint et rallumé à la main. Pour cette raison, ces écritures (et le
redémarrage `060F`) ne sont **pas** exposées par le projet.

**Expérience F5 (09/10/2026, deux redémarrages depuis l'app)** : avant de
redémarrer, l'app avertit que la haute résolution « peut nuire à la stabilité
de la connexion Bluetooth », puis demande de retirer le casque.

| Changement | Reconnexion du PC (icône) | Reconnexion de l'app |
|---|---|---|
| Qualité avancée → haute résolution | seule, en 6 s ; `0406` relu = 0 | seule |
| Haute résolution → qualité avancée | seule, en 10 s ; `0406` relu = 1 | **bloquée** sur « Redémarrage en cours… » jusqu'à une extinction manuelle du casque |

Avec l'essai précédent, l'app reste bloquée deux fois sur trois ; rien ne
montre que la connexion du PC en soit la cause. L'écriture reste donc en
lecture seule dans le projet.

Fonctions propres à l'app, sans commande vers le casque : **Retrouver le
casque** (carte de la dernière position du téléphone) et **Zones sonores**
(réglages appliqués selon la position).

### Personnalisation du son (capture du 09/10/2026)

Test auditif fait par l'utilisateur (≈ 3 min 30 : volume, instruments à
cordes, balance basses/cordes, volume le plus bas, percussions). Étapes
horodatées par un suivi passif de l'écran via ADB.

| Moment | Trames | Lecture |
|---|---|---|
| Pendant tout le test | `2000 [01]` toutes les 5 s, accusé `2100` | notification `2081 [01]` = test en cours |
| Curseurs du test | **aucune trame** : l'app traite l'audio de test côté téléphone | — |
| Début du parcours (rapport de bug précédent `.last`) | `080C [00]` (Smart Pause coupée), puis `0803 [00 03]`, accusé `0903` | `0884 [00 03]` : **mode audio 3** |
| Fin du test (« Parfait ! ») | `2002` (24 octets), `2004` (24 octets), `2006` (14 octets), accusés `2102`/`2104`/`2106` | `2081 [02]` = profil reçu |
| Après le profil | `080C [01]` (Smart Pause rétablie), puis `0803 [00 01]`, accusé `0903` | `0884 [00 01]` : retour à l'égaliseur |
| Lecture depuis le PC | `2001` → état ; `2003`, `2005`, `2007` → les trois blocs | **identiques octet pour octet** aux trames envoyées (export `ctl exporter-profil` du 09/10/2026, comparé à la capture) |

Les blocs sont des **flottants 32 bits big-endian** :
- `2002` : −66,8 / −51,8, −76,7 / −61,7, −86,6 / −71,6 ;
- `2004` : 1,137 / 1,0, 1,268 / 1,0, 1,398 / 1,0 ;
- `2006` : −0,9 ×3, puis `05 05`.

Interprétation **probable**, pas établie : traitement dynamique sur 3
groupes (gains, seuils, rapports). L'audit indépendant de l'app 1.6.3 a
trouvé un calcul local (`SurfaceFit.getCompressorSettings()`, « 3 gains,
6 seuils, 6 rapports »), mais ni l'algorithme exact, ni les fréquences des
bandes, ni le sens des deux octets finaux `05 05` (positions X/Y ? presets ?).

Le profil est propre à l'oreille de l'utilisateur. Le projet le **lit**
(`probe --deep`) et peut l'**exporter octet pour octet**
(`ctl exporter-profil fichier.json`) ; il ne l'écrit jamais. Avant le test,
`2001` valait 0 et les blocs étaient nuls.

Trois états à ne pas confondre (aucun ne se déduit des autres) :

| État | Ce qui le montre | Établi ? |
|---|---|---|
| Profil **lu** | `2003`/`2005`/`2007` rendent des blocs non nuls | oui, sur ce casque, identique octet pour octet au profil envoyé |
| Profil **sélectionné** (traitement actif) | mode audio `0804` = 3 | oui : `0803 [00 03]` sélectionne le profil existant (voir ci-dessous) |
| Profil **persistant** après extinction | relecture après redémarrage | oui : identique et toujours sélectionné après extinction (F2) |

`2081 [02]` signifie que le casque a reçu un profil, pas qu'il est
sélectionné : l'app repasse en mode égaliseur juste après. Le mode audio est
donc modélisé à part (`AudioMode` : égaliseur, Speech Clarity, profil
personnalisé, valeur inconnue conservée).

### Sélection du profil existant (capture HCI du 09/10/2026)

Appuis dans « My sound » horodatés par ADB, journal HCI du téléphone, et
relecture en parallèle depuis le PC (multipoint).

| Action dans l'app | Trames de l'app | Réponses du casque |
|---|---|---|
| Tuile « Égaliseur graphique » | `0803 [00 01]` | `0903`, `208A [00]`, `0884 [00 01]` |
| Dolby Atmos MARCHE (en mode égaliseur) | `3604 [01]` | `3704`, `3685 [01]`, `3683`, `3681`, `208A [02]` |
| Tuile « Personnalisation du son », Dolby Atmos actif → « Désactiver Dolby Atmos » | `3604 [00]` puis `0803 [00 03]` | `3685 [00]`, `0903`, `208A [01]`, `0884 [00 03]` |
| Même tuile, Dolby Atmos coupé | `0803 [00 03]` seul | `0903`, `208A [01]`, `0884 [00 03]` |
| Dolby Atmos MARCHE alors que le profil est sélectionné | **aucune** : dialogue « J'ai compris » | — |

Constats :
- le mode 3 sélectionne le profil enregistré **sans le réécrire** : les blocs
  `2003`/`2005`/`2007` relus restent identiques octet pour octet, et `2001`
  reste à 2 ;
- pas de signal régulier nécessaire : le mode 3 tient (relu une minute plus
  tard) ;
- profil personnalisé et Dolby Atmos s'excluent, et c'est l'app qui le gère ;
- `208A` (feature 16, notification) vaut 0 en mode égaliseur, 1 profil
  sélectionné, 2 après activation de Dolby Atmos. Lecture « profil utilisé /
  bloqué par Dolby Atmos » : **hypothèse** (trois cas observés).

Le projet reproduit cette séquence (`ctl profil on|off`, case
« Personnalisation du son » de l'icône) : refus si `2001` ≠ 2, coupure de
l'audio spatial s'il est actif, puis `0803 [00 03]`. Activer l'audio spatial
est refusé tant que le profil est sélectionné, comme dans l'app. Validé
depuis le PC le 09/10/2026 (`profil off` puis `profil on`, relecture du
mode).

Observé pendant le test : `0880` (codec actif : 02 en lecture, FF à
l'arrêt), `089A` (fréquence d'échantillonnage : `AC 44` = 44 100 Hz) et
`1494 [adresse, 0|1]` (lecture audio active par appareil).

## Fonctions encore invisibles côté protocole

Toutes les fonctions de l'app sur le M5 ont été **capturées** ou
identifiées comme propres à l'app. Capturé ne veut pas dire validé : restent
non qualifiées la combinaison de priorité audio `(0406, 0408) = (1, 1)`
(jamais observée), la langue des invites (seul 2 = français est confirmé) et
l'écriture de la priorité depuis le PC (non exposée, voir F5).

### Sidetone (captures du 25/10/2026)

Retour de sa propre voix pendant un appel. Contrat observé dans
Smart Control Plus 1.6.3 / 72844, M5AEBT 6.21.5 (capture
`0b6400dafa003961c4a08bc6f2780c0605f606c0f969a86b513d75174a0a0b60`,
records 22322 à 28852) :

| Échange | Identifiant | Charge utile |
|---|---|---|
| Lecture | `0806` → `0906` | vide → un octet ; `00` observé |
| Écriture | `0805` → `0905` | un octet → vide ; `02`, `01`, `02`, `00` capturés (records 27904, 27936, 27970, 28005) |
| Notification | `0886` | un octet ; `02`, `01`, `02`, `00` observés après chaque écriture |

`00` = arrêt, `01` = minimum actif, `02` = deuxième palier actif.
L'app prévoit cinq pas dans la configuration examinée, donc **0 à 4**
(l'ancienne annotation « 0 à 5 » du code est corrigée) ; seuls **0, 1 et 2**
ont été observés, 3 et 4 restent non essayés. Aucun essai pendant un appel ni
mesure acoustique. Rejeu depuis le PC : **à valider sur le matériel**.

### Volume des invites : échelle (expérience F1, 09/10/2026)

15 positions du curseur de l'app relues depuis le PC : **valeur =
arrondi(26 × % / 100 − 20)**, de −20 (0 %) à +6 (100 %), 0 = 77 %.

| % | 0 | 10 | 20 | 30 | 40 | 50 | 57 | 62 | 67 | 72 | 77 | 82 | 87 | 92 | 99 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| valeur | −20 | −17 | −15 | −12 | −10 | −7 | −5 | −4 | −3 | −1 | 0 | 1 | 3 | 4 | 6 |

Écriture `081C [i8]` validée depuis le PC (`ctl volume-invites 67` → −3
relu, puis 77 → 0 relu). Plusieurs crans de l'app donnent la même valeur :
le pourcentage affiché par le projet est donc approché.

Ce pourcentage est une **échelle d'interface choisie par le projet** à partir
de ces relevés. L'analyse de l'app (27/10/2026) décrit la même valeur
interne sous une forme normalisée, (entier + 20) / 26 : ce n'est ni un
niveau acoustique, ni un gain en dB, ni forcément le pourcentage affiché par
l'app ; les deux présentations ne sont pas le même type de mesure.

### Persistance du profil (expérience F2, 09/10/2026)

Après deux redémarrages logiciels (priorité audio) puis une extinction
manuelle complète, le profil relu est identique octet pour octet (empreinte
`d8d1f1de00b984f9`) et reste **sélectionné** (`0804` = `00 03`). Les autres
réglages (priorité, volume des invites, Smart Pause…) ont aussi tenu.

### Nom du casque (expérience F3, 09/10/2026)

« Donner un nom au casque » dans l'app envoie le nom en octets, sans
terminateur ni longueur : `2801 "M5 Essai"` = `FF 03 00 08 04 95 28 01 4D 35
20 45 73 73 61 69`, réponse `2901` vide, **aucune notification**. La lecture
`2802` renvoie le nom. Validé depuis le PC : `ctl nom` relit « M5 Essai »,
puis `ctl nom "<nom d'origine>"` restaure le nom, relu par le PC puis par
l'app après sa relance (sans relance, l'app garde son cache). Windows garde
l'ancien nom jusqu'à une nouvelle connexion, comme l'app l'annonce.

Non relevés : la longueur maximale acceptée et les caractères accentués. Le
projet se limite donc à 32 caractères ASCII imprimables.

### Langue des invites (expérience F4, 09/10/2026)

L'app propose huit langues (français, allemand, anglais, espagnol, japonais,
coréen, russe, chinois) et « Télécharger et installer d'autres langues » ;
ouvrir la liste n'envoie aucune trame. Choisir « Anglais » affiche
« Télécharger et installer » : le pack anglais remplacerait le pack
français. Le casque ne garde donc qu'un pack de voix, et en changer revient à
transférer un fichier vers le casque. Expérience arrêtée avant « Continuer »
(le projet n'implémente aucune mise à jour) ; la langue reste en lecture
seule (`0807`, 2 = français).

### Gestion des connexions (expérience F6, 09/10/2026)

Écran « Gestion des connexions » de l'app : interrupteur Multipoint, liste
des appareils appairés (« Cet appareil » pour le téléphone), corbeille pour
supprimer des appairages. Un appui sur un appareil le connecte ou le
déconnecte, sans confirmation. Adresses remplacées par `[adresse]` (6 octets).

| Trame | Réponse | Lecture |
|---|---|---|
| `140D` | `[n, n × adresse]` | appareils appairés |
| `140E [adresse]` | `[adresse, état, 3 octets, nom…\0]` | état 1 = non connecté, 2 = connecté, 3 = connecté et vraisemblablement en lecture audio (l'app affiche alors deux fois « connecté » et refuse de le déconnecter) |
| `1413` | `[adresse]` | adresse de l'appareil qui interroge (« Cet appareil ») |
| `1410 [adresse]` | `1510` vide | déconnecter ; puis notifications `1491 [adresse, 01, 00]` et `1484 [00 00]` |
| `140F [adresse]` | `150F` vide | connecter |

Validé depuis le PC : `ctl connexions` reconnaît le PC grâce à `1413`, puis
`ctl deconnecter` et `ctl connecter` sur le téléphone (l'app s'est reconnectée
seule) ; même aller-retour avec les boutons de l'Accueil. Le projet refuse de
déconnecter le PC lui-même. Non touchés : la suppression d'un appairage et
l'interrupteur Multipoint.
**Ne pas deviner** : capturer l'app pendant qu'on change le réglage.

### Suivi de tête (essai E4.2, 09/10/2026)

L'option « Suivi de la tête » n'apparaît dans la page « Spatial audio » que
lorsque l'audio spatial est actif ; elle était **activée par défaut**. Pour
activer l'audio spatial, l'app exige d'abord de quitter le profil
personnalisé (simple avertissement, sans bouton de désactivation) : passage
par « Égaliseur graphique » (`0803 [00 01]`), puis `3604 [01]`.

| Action dans l'app | Trame | Réponse | Notification |
|---|---|---|---|
| Suivi de tête coupé / activé | `3602 [00]` / `[01]` | `3702` vide | `3683 [état]` |
| Ouverture de la page « Spatial audio » | `3608` | `3708` vide | flux `368D` (6 octets, ≈ 2 par seconde) |
| Sortie de la page | `3609` | `3709` vide | fin du flux |
| Audio spatial activé (`3604 [01]`) | — | `3704` vide | `3685 [01]`, `3683 [01]`, `3681 [01 1C]`, `208A [02]` |

Le flux `368D` porte l'orientation de la tête. Sans son et sans mouvement
(premier essai), toutes les valeurs restaient à zéro. **Second essai (E4.2b)** :
musique en cours, casque porté, mouvements de tête pendant 80 s ; 6 octets =
trois entiers signés 16 bits big-endian :

| Octets | Valeurs relevées | Lecture (hypothèse) |
|---|---|---|
| 1-2 | −179 à +179, passage de +175 à −163 sans valeur intermédiaire | lacet (gauche-droite) en degrés, sur ±180 |
| 3-4 | −65 à +40 | tangage (haut-bas) en degrés |
| 5-6 | toujours 0 | roulis non transmis ? |

Le sens positif de chaque axe n'est pas établi : les mouvements n'ont pas
suivi exactement les phases horodatées. L'app affiche en même temps des
textes comme « le son est maintenant plus fort à gauche ». Lecture seule :
le flux ne sert qu'à l'affichage. `3603` (lecture) vaut 1 au relevé
précédent, cohérent avec un suivi activé. `3681 [01 1C]` : sens inconnu.

**Validé depuis le PC (E4.4, 09/10/2026)** : `ctl profil off`, `ctl spatial
on`, puis `ctl suivi-tete off` → `3603` relu 0 et l'app du téléphone suit en
direct ; `ctl suivi-tete on` → relu 1, l'app aussi ; puis `ctl spatial off`
et `ctl profil on`. Le projet refuse le réglage quand l'audio spatial est
coupé (l'app ne le propose pas dans ce cas).

### Multipoint (essai E4.3, 09/10/2026)

Commande du **vendor Qualcomm** (`0x001D`), pas Sennheiser :

| Action dans l'app | Trame | Réponse | Notification |
|---|---|---|---|
| Multipoint coupé | `001D:0E00 [00]` | `0F00` vide | `0E80 [00]` |
| Multipoint rétabli | `001D:0E00 [01]` | `0F00` vide | `0E80 [01]` |

Après chaque changement, l'app relit `0495:1409` : `[01]` sans Multipoint,
`[02]` avec (nombre de connexions simultanées ; `[02]` au relevé F6).

Effets observés : couper Multipoint a **déconnecté le téléphone** (celui qui
envoyait la commande) et gardé le PC ; le téléphone n'a pu revenir que par
les réglages Bluetooth d'Android, ce qui a déconnecté le PC. Après le retour
de Multipoint, le PC s'est reconnecté seul en une dizaine de secondes
(`1491 [adresse, 02 00]`, `1494 [adresse, 01]`, codec `0880 [02]`). Un appui
sur le PC dans l'app pendant une lecture audio est refusé : « Impossible de se
déconnecter : arrêtez la lecture audio ».

**Validé depuis le PC (E4.5, 09/10/2026)** : `ctl multipoint off` → `1409`
relu 1, le casque **garde le PC** (celui qui envoie la commande) et
déconnecte le téléphone ; `ctl multipoint on` → relu 2 ; `ctl connecter`
reconnecte ensuite le téléphone (`140F`). Dans l'app (E4.3), c'était le
téléphone, auteur de la commande, qui avait été déconnecté : le casque ne
garde donc pas toujours l'auteur ; ce qui décide n'est pas établi (lecture
audio en cours ?).

### USB (essai E4.6, 10/10/2026)

Relevé en lecture seule, câble USB-C branché par l'utilisateur, casque allumé.
Périphérique USB composite **VID `3542`, PID `1004`** (pilote `usbccgp`) :

| Interface | Classe Windows | Lecture |
|---|---|---|
| MI_00 | HID, page `0x0001`, usage `0x0000`, rapports d'entrée et de sortie de 33 octets | canal de données générique, propre au fabricant (réglages ou mises à jour ?) : **non exploré, on n'y touche pas** |
| MI_01 | audio USB (`usbaudio`, classe 01/01) | sortie « Casque (MOMENTUM 5) » 2 canaux 48 kHz 24 bits par défaut, micro 1 canal 16 kHz 16 bits |
| MI_04 COL01 | HID contrôle consommateur (page `0x000C`) | touches multimédia |
| MI_04 COL02 | HID « headset » (page `0x000B`, usage `0x0005`), rapport de sortie de 2 octets | téléphonie (voyants muet/sonnerie ?) |

Le Bluetooth reste connecté pendant le branchement, et le contrôle GAIA par
Bluetooth fonctionne toujours (`ctl etat`). Le débranchement laisse le
Bluetooth connecté. Les formats sont les formats **par défaut** de Windows ;
la liste complète des formats acceptés n'a pas été interrogée.

## Sources

- Relevé `probe --deep` (ce dépôt, `tests/m5_captures.rs`).
- [PROTOCOL.md d'Oein](https://github.com/Oein/sennheiser-desktop-client/blob/main/PROTOCOL.md)
  et le code de son client macOS (lu pour les faits, rien de copié : pas de licence).
- [Page produit Sennheiser](https://us.sennheiser-hearing.com/products/momentum-5-wireless),
  [test SoundGuys](https://www.soundguys.com/sennheiser-momentum-5-wireless-review-157679/),
  [test Major HiFi](https://majorhifi.com/38886-2/).
