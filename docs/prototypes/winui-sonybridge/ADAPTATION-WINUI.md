# SonyBridge → OpenHeadset Control / WinUI 3

Proposition du 28 octobre 2026. Statut : maquette Web interactive vérifiée et versionnée comme étude, intégration native à réaliser. Aucun changement du code applicatif natif, aucune commande matérielle envoyée.

Voir le [mode d’emploi](README.md) pour lancer la maquette sur cette machine.

## Ce qui est utile dans SonyBridge

La référence fournie met en avant le produit, son état, puis des cartes de réglages lisibles. Les badges compacts évitent de chercher la batterie ou le mode dans plusieurs écrans. La sélection du mode et le curseur sont proches, et la liste des appareils distingue les connexions des appairages. Ces principes conviennent à OpenHeadset.

La référence visuelle est issue de l’interface macOS de SonyBridge. Son [ContentView SwiftUI](https://github.com/AmitRajput-Dev/SonyBridge/blob/d0ecd561ebf6f1cb87ffbf29528107b2221724d5/Client/macos/ContentView.swift) a été consulté à cette révision : `d0ecd561ebf6f1cb87ffbf29528107b2221724d5`. Il organise les cartes selon les capacités et propose une disposition élargie. Cela fournit des idées d’organisation ; ce n’est pas du code WinUI réutilisable tel quel.

## Adaptation retenue

| Élément de référence | Proposition Windows | Condition d’intégration |
|---|---|---|
| Grande image et modèle | Bandeau horizontal MOMENTUM 5 | Visuel compact ; ne pas repousser tous les contrôles sous le défilement |
| Badges batterie / codec / mode | Batterie, codec connu, état porté ; charge séparée | Chaque valeur vient de son champ existant ; afficher « inconnu » si absent |
| Cartes son ambiant | Carte Contrôle du bruit : Adaptatif / Personnalisé / Arrêt | Garder exactement les modes déjà exposés par le modèle de vue M5 |
| Curseur de niveau | ANC maximal à gauche, transparence maximale à droite | Échelle de présentation 0–100 ; ne pas la transformer en contrat protocolaire |
| Focus on Voice | Aucun équivalent ajouté | Ne pas inventer une fonction M5 à partir d’une fonction Sony |
| Liste d’appareils | Connectés / Appairés ; « Ce PC » ; actions explicites | Pas d’action de déconnexion de la session de contrôle actuelle |
| Appareil de lecture fixé | Aucun verrouillage ajouté | Une connexion Bluetooth ne prouve pas la source audio active |
| Thèmes très marqués | Clair / sombre, accent système à terme | Utiliser les ressources WinUI ; pas de thème Sony Signature par défaut |
| Bouton marche/arrêt | Aucun bouton ajouté | Pas de nouvelle commande d’extinction pour une raison esthétique |

L’audio spatial, le suivi de tête et le raccourci vers l’égaliseur reprennent les fonctions présentes dans le frontend local. La comparaison A/B existe déjà : la proposition améliore sa visibilité. Le sidetone n’est pas ajouté au parcours tant que son chemin matériel et son intégration au service ne sont pas qualifiés séparément.

## Traduction dans l’application native

Le frontend est écrit en **Rust avec windows-reactor et WinUI 3**. La maquette React sert de référence visuelle ; elle n’ajoute pas de WebView et n’appelle pas le casque.

| Zone | Composants WinUI à utiliser | Point d’ancrage existant |
|---|---|---|
| Navigation latérale | `NavigationView`, `NavigationViewItem` | `frontends/winui/src/app.rs`, navigation existante |
| Bandeau casque | `Grid`, `Image`, `TextBlock`, `Border` | Vue `home`, états du modèle de vue |
| Cartes | `Grid`, `StackPanel`, `Border`, ressources de thème | Réorganiser les contrôles de `home` |
| Modes | `RadioButtons`, présentation compacte accessible | Garder les gestionnaires de mode existants |
| Niveau | `Slider`, valeur textuelle, libellés des deux extrémités | Garder le brouillon et la coalescence existants |
| Options binaires | `ToggleSwitch` | Gardes de disponibilité du modèle de vue |
| Appareils | Lignes avec `Button`, `MenuFlyout`, `ContentDialog` | Actions typées du client et confirmations existantes |
| État indisponible | `InfoBar`, texte d’aide associé | Aucun remplacement par une valeur zéro ou « arrêt » |

Les chemins sont relatifs au dépôt OpenHeadset. Référence locale consultée : branche `essai-etat-port`, commit `c196002435e05d161885d0b3a4b6d48544ec31af`. Relire le HEAD au moment d’implémenter pour préserver les travaux parallèles.

### Premier lot natif conseillé

1. Réorganiser seulement l’accueil : bandeau compact, carte bruit, carte appareils, carte spatial, raccourci Mon son. Réutiliser les commandes et le service existants.
2. Ajouter les informations charge/port déjà disponibles côté cœur et IPC, en conservant leur caractère optionnel et leur invalidation à la fin de session.
3. Employer les couleurs système et les contrôles natifs. Deux colonnes si la largeur disponible le permet, une colonne sinon ; préserver l’ordre clavier et la lisibilité à 125/150/200 % de mise à l’échelle.
4. Vérifier que les notifications et les réponses ne recréent pas les curseurs, ne déplacent pas le focus et ne perdent pas la dernière intention utilisateur.

Le résultat d’une écriture native doit être fondé sur les réponses et relectures du service. La maquette applique localement un changement et montre un délai fictif de 450 ms ; ce délai n’est ni une mesure ni une règle à reprendre dans le produit. Le scénario « valeurs inconnues » rend inconnus batterie, codec, port et charge, et conserve les réglages indépendants simulés. Il ne simule pas tout le registre de capacités.

### Critères de sortie du lot natif

- États connecté, déconnecté, partiellement disponible, écriture en cours et échec distincts.
- Aucune ancienne mesure réaffichée comme actuelle après une nouvelle session.
- Contrôle du bruit : sens 0 ANC → 100 transparence inchangé ; curseur disponible seulement dans le mode approprié.
- Dépendance spatial/suivi de tête et limites de connexion pilotées par le modèle de vue, sans déductions depuis le dessin.
- Nom accessible pour chaque bouton, curseur et interrupteur ; focus visible et stable ; thèmes système et contraste élevé vérifiés sous Windows.
- Tests natifs de focus/défilement et compilation WinUI nécessaires avant de déclarer cette interface intégrée. Ils n’ont pas été exécutés pour la maquette Web.

## Fonctionnement de la maquette

Les pages Accueil, Mon son, Réglages, Batterie et À propos sont navigables. Le sélecteur en haut permet de simuler connexion, lecture seule et valeurs inconnues. Les contrôles changent uniquement l’état local de la page. Batterie 92 %, codec AAC, état porté, appareils et réglages sont **des exemples fictifs**, pas des observations de la session du casque.

Pour la relancer : installer les dépendances avec `npm install`, puis `npm run dev -- --host 127.0.0.1 --port 4185 --strictPort` depuis ce dossier. `npm run build` produit le bundle. Le prototype a été contrôlé à 1280 × 1100 et 480 × 900 CSS px ; ces résultats ne remplacent pas une qualification WinUI avec lecteur d’écran et mise à l’échelle Windows.

## Ressources et provenance

- Capture SonyBridge : référence publique téléchargée à la demande pour comparaison locale ; la capture fournie par l’utilisateur et les planches de contrôle ne sont pas versionnées. Aucun code SwiftUI n’a été copié.
- Photo M5 : [photo officielle Sennheiser](https://cdn.shopify.com/s/files/1/0781/7248/7963/files/M5_Black_Main_IsoFront.jpg?v=1776463846&width=700), issue de la [fiche produit](https://us.sennheiser-hearing.com/products/momentum-5-wireless). Image 700 × 700, fond blanc conservé y compris dans le thème sombre. Cette consultation ne constitue pas une autorisation de redistribution dans une version publiée.
- Icônes : [Microsoft Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons), SVG officiels ; liens détaillés dans `ressources.json`. Préférer les icônes natives du frontend pour l’intégration.
- Logo OpenHeadset : ressource du projet, utilisée uniquement dans cette proposition locale.

Les sources originales de la maquette sont versionnées dans ce dossier. Les photographies, références et icônes tierces sont téléchargées uniquement dans le dossier local ignoré par Git. Leur présence dans le dossier local ne donne aucune autorisation de redistribution. Le logo OpenHeadset provient du projet. Aucune publication ni modification matérielle n’a été effectuée.
