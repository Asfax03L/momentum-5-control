# Interfaces (frontends)

Chaque dossier contient **une** interface utilisateur. Aucune ne parle
directement au casque : elles passent toutes par le service.

```
momentum-core      protocole, casque, modèles d'affichage (viewmodel, ui)
      │
momentum-daemon    service : seul propriétaire de la connexion au casque
      │            (worker de momentum-service)
momentum-client    API : Command  ← interface
      │                 Event, Snapshot → interface
      │            (D-Bus sous Linux, canal nommé sous Windows)
frontends/<nom>    affichage + appels système propres aux fenêtres
```

## Règles pour une interface

1. **Ne jamais ouvrir de connexion au casque.** Envoyer des
   `momentum_client::Command` au service et afficher le `Snapshot` et les
   `Event` reçus (`momentum_client::Client`). Le service regroupe les
   commandes, relit l'état et garde les notifications ; une interface ne
   doit pas refaire ce travail.
2. **Réutiliser les modèles d'affichage** de `momentum_core::viewmodel` et
   `momentum_core::ui` (libellés, listes de choix, pages). Ils sont testés
   et identiques pour toutes les interfaces.
3. **Garder ici seulement ce qui touche aux fenêtres** : placement, focus,
   boîtes de dialogue (exemple : `winui/src/desktop.rs`). Les
   notifications sont envoyées par le service.
4. Mêmes règles de sécurité que le reste du projet (voir `CLAUDE.md`) :
   lecture seule tant qu'une commande n'est pas validée, aucune donnée
   personnelle dans les journaux.

## Interfaces

| Dossier | Bibliothèque | Systèmes | État |
|---|---|---|---|
| `kirigami/` | Qt 6 / Kirigami (KDE), pont cxx-qt | Linux KDE | interface KDE (v0.8.0) |
| `winui/` | WinUI 3 via [`windows-reactor`](https://github.com/microsoft/windows-rs) (Microsoft, 100 % Rust) | Windows | interface Windows (v0.8.0) |
| `gtk/` | GTK 4 / libadwaita (GNOME) | Linux GNOME | à explorer |

Notes pour les pistes « à explorer » (à vérifier au moment de s'y mettre) :

- **WinUI 3 en Rust** (fait en partie) : `windows-reactor` 0.100
  (Microsoft, dans windows-rs, en préversion) décrit l'interface en Rust
  (composant, messages typés, contrôles WinUI natifs). Il demande le
  Windows App Runtime 2.x installé ; la chaîne GNU suffit. `src/app.rs` :
  client du service (`momentum_client::Client`), messages du service
  relayés par une tâche de fond. Une valeur égale à l'état lu n'est jamais
  renvoyée, et les curseurs ont le pas du casque (0,1 dB, 1 %) : WinUI ne
  peut pas déclencher d'écriture en mettant un contrôle à jour.
  `--page son` ouvre une page donnée (vérifications), `--cache` démarre
  caché. `src/desktop.rs` : fenêtre vue par Win32 (fermeture qui cache,
  affichage depuis l'icône ou un raccourci).
- **Kirigami** (fait) : `src/bridge.rs` expose un objet QML `Headset`
  (propriétés et actions) par `cxx-qt` 0.10 ; il est client du service
  (`momentum-client`, D-Bus) et reprend les modèles de vue de
  `momentum-core`. `src/tray.rs` : icône de la zone de notification
  (`ksni`) ; `src/shortcuts.rs` : raccourcis globaux (KGlobalAccel). Hors des membres par
  défaut de l'espace de travail (Qt 6 requis) : `cargo build -p
  momentum-kirigami`.
- **GTK** : `gtk4-rs` et `libadwaita-rs`.

## Ajouter une interface

1. Créer `frontends/<nom>/` avec son `Cargo.toml` (dépendances
   `momentum-core` et `momentum-client` de l'espace de travail).
2. L'ajouter à `members` dans le `Cargo.toml` racine.
3. Limiter ses dépendances système à `cfg(windows)` ou
   `cfg(target_os = "linux")` pour que `cargo build --workspace` passe
   partout (la CI compile Windows et Linux).
