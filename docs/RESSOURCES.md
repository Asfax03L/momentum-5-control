# Budget de ressources

Le service tourne en permanence et l'interface reste souvent ouverte dans
la zone de notification : ils doivent rester légers. Chaque version est
mesurée, et la CI échoue si un relevé dépasse son seuil.

## Méthode

- **Version release**, au repos depuis 30 s : rien n'est cliqué, aucun
  réglage n'est envoyé.
- **Service** (`momentum-daemon`) : lancé sur un bus de session privé,
  sans casque (il cherche un casque sans en trouver).
- **Client KDE** : `momentum-kirigami --demo`, sans écran
  (`QT_QPA_PLATFORM=offscreen`), même bus privé.
- **WinUI** : `momentum-winui --demo` (casque simulé, aucun service).
- **Mémoire** : mémoire résidente (`VmRSS` sous Linux, plage de travail
  sous Windows). **Processeur** : temps consommé depuis le lancement
  (utilisateur + système), démarrage compris.

Scripts : `scripts/mesurer-ressources.sh <binaires> [durée]` (Linux) et
`scripts/mesurer-ressources.ps1 -Binaires <binaires>` (Windows). La CI les
lance dans les tâches `ressources` (Fedora 44) et `winui-demarrage`
(Windows), et recopie le tableau dans l'onglet Summary.

## Seuils

Larges, pour repérer une dérive et non une variation normale : environ
trois fois les premiers relevés, arrondis.

| Processus | Mémoire | Processeur (30 s) |
|---|---|---|
| `momentum-daemon` sans casque | 40 Mio | 3 s |
| `momentum-kirigami --demo` | 450 Mio | 15 s |
| `momentum-winui --demo` | 400 Mio | 20 s |

## Relevés

| Date | Version | Système | Processus | Mémoire | Processeur |
|---|---|---|---|---|---|
| 23/10/2026 | 0.10.0+ | Bazzite (Fedora 44, Qt 6.11) | `momentum-daemon` sans casque | 5,7 Mio | 0,00 s |
| 23/10/2026 | 0.10.0+ | Bazzite (Fedora 44, Qt 6.11) | `momentum-kirigami --demo` | 104 Mio | 0,36 s |
| 23/10/2026 | 0.10.0+ | CI Fedora 44 | `momentum-daemon` sans casque | 5,6 Mio | 0,00 s |
| 23/10/2026 | 0.10.0+ | CI Fedora 44 | `momentum-kirigami --demo` | 89 Mio | 0,42 s |
| 23/10/2026 | 0.10.0+ | CI Windows (windows-latest) | `momentum-winui --demo` | 96 Mio | 0,48 s |

Les chiffres de WinUI viennent de la CI (machine virtuelle Windows de
GitHub) ; ils sont ajoutés à chaque version.
