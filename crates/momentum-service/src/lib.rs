//! Service applicatif de momentum-ctl-win, sans interface graphique.
//!
//! - `worker` : seul propriétaire de la connexion au casque. Une interface
//!   lui envoie des [`worker::WorkerCommand`] et reçoit des
//!   [`worker::WorkerEvent`].
//! - `ipc_server` : canal local, pour `ctl` (Windows seulement pour
//!   l'instant).
//! - `desktop`, `system`, `audio_output`, `rotating_log` : intégration au
//!   système (notifications, raccourcis, démarrage automatique, instance
//!   unique, sorties audio, journal).
//!
//! Chaque module système a une version Windows et une version Linux qui
//! exposent la même interface : les interfaces graphiques n'ont pas à s'en
//! soucier. Ce que Linux ne fait pas encore (raccourcis globaux, sorties
//! audio) renvoie une erreur ou un refus explicite.

#[cfg(windows)]
pub mod audio_output;
#[cfg(target_os = "linux")]
#[path = "audio_output_linux.rs"]
pub mod audio_output;

#[cfg(windows)]
pub mod desktop;
#[cfg(target_os = "linux")]
#[path = "desktop_linux.rs"]
pub mod desktop;

#[cfg(windows)]
pub mod system;
#[cfg(target_os = "linux")]
#[path = "system_linux.rs"]
pub mod system;

#[cfg(windows)]
pub mod ipc_server;

#[cfg(any(windows, target_os = "linux"))]
pub mod rotating_log;
#[cfg(any(windows, target_os = "linux"))]
pub mod worker;
