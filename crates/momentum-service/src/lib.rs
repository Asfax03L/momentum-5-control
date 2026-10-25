//! Service applicatif de momentum-ctl-win, sans interface graphique.
//!
//! - `worker` : seul propriétaire de la connexion au casque. Une interface
//!   lui envoie des [`worker::WorkerCommand`] et reçoit des
//!   [`worker::WorkerEvent`].
//! - `desktop`, `system`, `audio_output`, `rotating_log` : intégration au
//!   système (notifications, raccourcis, démarrage automatique, instance
//!   unique, sorties audio, journal).
//!
//! `system` et `audio_output` servent l'interface WinUI : Windows
//! seulement. Sous Linux, `desktop` se limite aux notifications du service ;
//! le client KDE gère lui-même raccourcis, démarrage et instance unique.

#[cfg(windows)]
pub mod audio_output;

#[cfg(windows)]
pub mod desktop;
#[cfg(target_os = "linux")]
#[path = "desktop_linux.rs"]
pub mod desktop;

#[cfg(windows)]
pub mod system;

#[cfg(any(windows, target_os = "linux"))]
pub mod clock;
#[cfg(any(windows, target_os = "linux"))]
pub mod reconnect;
#[cfg(any(windows, target_os = "linux"))]
pub mod rotating_log;
#[cfg(any(windows, target_os = "linux"))]
pub mod worker;
