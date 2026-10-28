# Proposition d’interface OpenHeadset pour WinUI 3

Étude du 28 octobre 2026, inspirée de l’organisation visuelle de [SonyBridge](https://github.com/AmitRajput-Dev/SonyBridge). **Il s’agit d’une maquette Web interactive, pas d’une modification de l’interface native Windows.**

Cette proposition montre un accueil avec bandeau MOMENTUM 5, badges d’état, cartes bruit / spatial / appareils, raccourci vers l’égaliseur, navigation latérale et thèmes clair/sombre. Les cinq pages sont navigables. La [note d’adaptation](ADAPTATION-WINUI.md) explique comment reprendre cette présentation dans le frontend Rust / windows-reactor existant.

## Lancer la maquette

Depuis ce dossier, avec Node.js 22 ou plus récent et npm :

```sh
npm ci
npm run assets
npm run dev -- --host 127.0.0.1 --port 4185 --strictPort
```

Ouvrir l’adresse locale indiquée par Vite. Pour construire le bundle :

```sh
npm run build
```

`npm run assets` télécharge uniquement des images et icônes aux adresses publiques listées dans [ressources.json](ressources.json). Cette étape requiert Internet. Elle n’exécute aucun fichier téléchargé. Les ressources tierces restent ignorées par Git ; seules les sources originales de la maquette et le logo du projet sont versionnés. Les icônes viennent de Microsoft Fluent UI System Icons (MIT) ; photo et capture restent des références d’étude locale, sans autorisation de redistribution présumée.

## Limites et comportement

- Aucun accès Bluetooth, ADB, IPC ou service OpenHeadset. Aucun appel à une commande du casque.
- Batterie 92 %, codec AAC, état porté, liste d’appareils et réglages sont fictifs.
- Le sélecteur supérieur simule connexion, lecture seule et informations inconnues ; le bouton de thème reste propre à la maquette.
- L’état change en mémoire et revient à ses valeurs initiales au rechargement. La confirmation et le délai d’application sont simulés.
- La déconnexion d’un appareil est fictive et précédée d’un dialogue. Le PC courant n’offre pas de bouton de déconnexion.
- Le scénario « valeurs inconnues » concerne les badges batterie / codec / port / charge. Il ne couvre pas toutes les capacités indépendantes du vrai modèle de vue.
- Le sens du curseur reste ANC maximal → transparence maximale. Aucune commande Sony n’est transposée dans le protocole M5.
- L’A/B sert à montrer une fonction déjà présente dans OpenHeadset ; il n’est pas présenté comme une nouvelle capacité matérielle.

## Vérifications de la proposition

La maquette source a été comparée visuellement à la référence fournie, en clair et sombre, à 1280 × 1100 CSS px. Les largeurs 800 et 480 CSS px ont aussi été contrôlées sans défilement horizontal. Les dépendances mode / niveau et immersif / suivi de tête, le dialogue d’appareil, l’A/B au clavier, la lecture seule et les valeurs inconnues ont été parcourus dans le navigateur. Aucune erreur ou alerte console n’a été relevée lors du dernier contrôle. La construction Vite a réussi.

Ces contrôles concernent le démonstrateur. L’intégration WinUI, la compilation native, le contraste élevé, la mise à l’échelle Windows et les retours matériels restent à vérifier séparément.

## Premier lot d’intégration proposé

Réorganiser uniquement l’accueil dans `frontends/winui/src/app.rs`, en réutilisant les commandes typées, le modèle de vue, les gardes de disponibilité et le service existants. Afficher charge et port quand leurs champs sont présents. Préserver brouillons, focus et défilement pendant les mises à jour. Ce lot ne demande aucune modification de GAIA, ni de nouveau chemin matériel.
