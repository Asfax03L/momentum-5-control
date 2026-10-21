//! Intégration au bureau Windows : raccourcis clavier globaux et
//! notifications (toasts). La logique (quelles touches, quand notifier) est
//! dans `momentum-core` (`hotkeys`, `alerts`) ; ici, seulement les appels
//! Windows. Ce qui touche aux fenêtres d'une interface (placement, focus,
//! boîte « Ouvrir ») reste dans l'interface concernée.

use std::sync::mpsc;

use momentum_core::hotkeys::{Hotkey, HotkeyAction};
use tracing::{debug, warn};
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PostThreadMessageW, WM_HOTKEY, WM_QUIT,
};
use windows::core::HSTRING;

use crate::system::set_user_string;

/// Identifiant d'application des notifications (AppUserModelID), déclaré
/// dans `HKCU\Software\Classes\AppUserModelId` : pas de droits
/// administrateur ni de raccourci du menu Démarrer nécessaires.
const AUMID: &str = "momentum-ctl-win.tray";

/// Thread des raccourcis, arrêtable pour réenregistrer d'autres touches.
pub struct HotkeyThread {
    thread_id: u32,
    handle: std::thread::JoinHandle<()>,
}

impl HotkeyThread {
    /// Libère les raccourcis et attend la fin du thread.
    pub fn stop(self) {
        // SAFETY: message posté à la file d'un thread de ce processus.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        let _ = self.handle.join();
    }
}

/// Enregistre les raccourcis dans un thread dédié (les messages `WM_HOTKEY`
/// arrivent dans la file du thread qui les a enregistrés) et appelle
/// `on_hotkey` à chaque appui. Renvoie le thread (pour l'arrêter) et les
/// raccourcis refusés par Windows (déjà pris par une autre application).
pub fn spawn_hotkeys(
    keys: Vec<(HotkeyAction, Hotkey)>,
    on_hotkey: impl Fn(HotkeyAction) + Send + 'static,
) -> (Option<HotkeyThread>, Vec<String>) {
    if keys.is_empty() {
        return (None, Vec::new());
    }
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("raccourcis".into())
        .spawn(move || {
            let mut refused = Vec::new();
            let mut registered = Vec::new();
            for (id, (action, key)) in keys.iter().enumerate() {
                let mut mods = MOD_NOREPEAT;
                for (on, m) in [
                    (key.ctrl, MOD_CONTROL),
                    (key.alt, MOD_ALT),
                    (key.shift, MOD_SHIFT),
                    (key.win, MOD_WIN),
                ] {
                    if on {
                        mods |= m;
                    }
                }
                let id = i32::try_from(id).unwrap_or(i32::MAX);
                // SAFETY: fenêtre nulle = messages postés à ce thread.
                match unsafe { RegisterHotKey(None, id, HOT_KEY_MODIFIERS(mods.0), key.vk) } {
                    Ok(()) => registered.push(id),
                    Err(e) => refused.push(format!("{} : {}", action.label(), e.message())),
                }
            }
            // SAFETY: identifiant du thread courant.
            let thread_id = unsafe { GetCurrentThreadId() };
            let _ = tx.send((thread_id, refused));
            let mut msg = MSG::default();
            // SAFETY: boucle de messages classique de ce thread ; WM_QUIT
            // (posté par `stop`) la termine.
            while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                if msg.message == WM_HOTKEY
                    && let Some((action, _)) = keys.get(msg.wParam.0)
                {
                    debug!(?action, "raccourci clavier");
                    on_hotkey(action.clone());
                }
            }
            for id in registered {
                // SAFETY: raccourcis enregistrés par ce même thread.
                let _ = unsafe { UnregisterHotKey(None, id) };
            }
        });
    match spawned {
        Ok(handle) => match rx.recv() {
            Ok((thread_id, refused)) => (Some(HotkeyThread { thread_id, handle }), refused),
            Err(_) => (None, vec!["thread des raccourcis arrêté".to_string()]),
        },
        Err(e) => (None, vec![format!("thread des raccourcis : {e}")]),
    }
}

/// Échappe un texte pour le XML du toast.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Logo affiché dans l'en-tête des toasts.
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo/png/128.png");

/// Déclare l'identifiant d'application (nom et logo affichés dans les
/// toasts). Le logo est copié dans le dossier des données : Windows lit
/// `IconUri` comme un chemin de fichier.
pub fn register_notifications() {
    let key = format!(r"Software\Classes\AppUserModelId\{AUMID}");
    if let Err(e) = set_user_string(&key, "DisplayName", "MOMENTUM 5 (non officiel)") {
        warn!(%e, "déclaration des notifications");
    }
    let Some(logo) = momentum_core::paths::data_dir().map(|dir| dir.join("logo.png")) else {
        return;
    };
    let written = logo
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&logo, LOGO_PNG));
    match written {
        Ok(()) => {
            if let Err(e) = set_user_string(&key, "IconUri", &logo.display().to_string()) {
                warn!(%e, "logo des notifications");
            }
        }
        Err(e) => warn!(%e, "copie du logo des notifications"),
    }
}

/// Affiche une notification Windows ; un échec est seulement journalisé.
pub fn show_toast(title: &str, body: &str) {
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text>\
         </binding></visual></toast>",
        xml_escape(title),
        xml_escape(body)
    );
    let result = (|| -> windows::core::Result<()> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))?.Show(&toast)
    })();
    if let Err(e) = result {
        warn!(%e, "notification Windows");
    }
}
