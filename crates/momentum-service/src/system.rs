//! Intégration Windows de l'application : lancement au démarrage (registre
//! utilisateur), instance unique et signal « ouvre l'égaliseur ».

use std::io;

use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GetLastError, HANDLE, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, INFINITE, SetEvent, WaitForSingleObject,
};
use windows::core::HSTRING;

/// Clé « Run » de l'utilisateur courant : pas besoin de droits administrateur.
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "momentum-ctl-win";

fn win_err(context: &str, code: WIN32_ERROR) -> io::Error {
    io::Error::other(format!("{context} : {:?}", code.to_hresult()))
}

/// Entrée de lancement automatique dans le registre.
pub struct Autostart {
    subkey: HSTRING,
    value: HSTRING,
}

impl Default for Autostart {
    fn default() -> Self {
        Self::new()
    }
}

impl Autostart {
    pub fn new() -> Self {
        Self::at(RUN_KEY, RUN_VALUE)
    }

    /// Entrée de lancement d'une autre interface (nom de valeur propre,
    /// même clé `Run`).
    pub fn named(value: &str) -> Self {
        Self::at(RUN_KEY, value)
    }

    fn at(subkey: &str, value: &str) -> Self {
        Self {
            subkey: HSTRING::from(subkey),
            value: HSTRING::from(value),
        }
    }

    /// Ligne de commande enregistrée : l'exécutable courant, entre guillemets.
    pub fn command_line() -> io::Result<String> {
        Ok(format!("\"{}\"", std::env::current_exe()?.display()))
    }

    /// Valeur actuellement enregistrée, s'il y en a une.
    pub fn registered(&self) -> io::Result<Option<String>> {
        let mut size = 0u32;
        // SAFETY: premier appel pour connaître la taille (aucun tampon fourni).
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(win_err("lecture du registre", status));
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: le tampon fait `size` octets, comme annoncé au registre.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(win_err("lecture du registre", status));
        }
        let text: Vec<u16> = buf.into_iter().take_while(|&c| c != 0).collect();
        Ok(Some(String::from_utf16_lossy(&text)))
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self.registered(), Ok(Some(_)))
    }

    /// Active (avec le chemin de l'exécutable courant) ou désactive.
    pub fn set(&self, on: bool) -> io::Result<()> {
        self.set_with_args(on, "")
    }

    /// Comme [`Autostart::set`], avec des arguments après l'exécutable
    /// (« --cache »).
    pub fn set_with_args(&self, on: bool, args: &str) -> io::Result<()> {
        if on {
            let line = Self::command_line()?;
            self.write(&if args.is_empty() {
                line
            } else {
                format!("{line} {args}")
            })
        } else {
            // SAFETY: chaînes valides, clé de l'utilisateur courant.
            let status =
                unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &self.subkey, &self.value) };
            if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(win_err("suppression dans le registre", status))
            }
        }
    }

    fn write(&self, data: &str) -> io::Result<()> {
        let wide: Vec<u16> = data.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = u32::try_from(wide.len() * 2).map_err(io::Error::other)?;
        // SAFETY: `wide` vit jusqu'à la fin de l'appel et fait `bytes` octets.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &self.subkey,
                &self.value,
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                bytes,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(win_err("écriture dans le registre", status))
        }
    }
}

/// Écrit une chaîne dans le registre de l'utilisateur courant (clé créée
/// au besoin).
pub fn set_user_string(subkey: &str, value: &str, data: &str) -> io::Result<()> {
    Autostart::at(subkey, value).write(data)
}

/// Résultat de la tentative de prise de l'instance unique.
pub enum Instance {
    /// Première instance : écouter `ShowRequest` pour ouvrir l'égaliseur.
    First(ShowRequest),
    /// Une instance tourne déjà ; elle a été priée d'ouvrir l'égaliseur.
    AlreadyRunning,
}

/// Événement nommé qu'une deuxième instance déclenche pour demander à la
/// première d'ouvrir la fenêtre de l'égaliseur.
pub struct ShowRequest(HANDLE);

// SAFETY: un HANDLE d'événement Windows peut être utilisé depuis n'importe
// quel thread ; on ne fait qu'attendre dessus.
unsafe impl Send for ShowRequest {}

impl ShowRequest {
    /// Bloque jusqu'à la prochaine demande ; `false` si l'attente échoue.
    pub fn wait(&self) -> bool {
        // SAFETY: handle valide pendant toute la vie du processus.
        unsafe { WaitForSingleObject(self.0, INFINITE) }.0 == 0
    }
}

/// Prend le verrou d'instance unique. Les handles ne sont jamais fermés :
/// ils doivent vivre aussi longtemps que le processus.
pub fn acquire_instance() -> io::Result<Instance> {
    acquire_named_instance("momentum-ctl-win-show-eq", "momentum-ctl-win-tray")
}

/// Verrou d'instance unique d'une autre interface : `event` (demande
/// d'ouverture) et `mutex` (instance en cours), dans l'espace de la session.
pub fn acquire_named_instance(event: &str, mutex: &str) -> io::Result<Instance> {
    let event = HSTRING::from(format!("Local\\{event}"));
    let mutex = HSTRING::from(format!("Local\\{mutex}"));
    // SAFETY: noms valides, attributs de sécurité par défaut.
    unsafe {
        let event = CreateEventW(None, false, false, &event).map_err(io::Error::other)?;
        let _mutex = CreateMutexW(None, false, &mutex).map_err(io::Error::other)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            SetEvent(event).map_err(io::Error::other)?;
            return Ok(Instance::AlreadyRunning);
        }
        Ok(Instance::First(ShowRequest(event)))
    }
}

/// Système (« Windows 10.0.26300.1 »), pour le diagnostic.
pub fn os_version() -> Option<String> {
    let packed = windows::System::Profile::AnalyticsInfo::VersionInfo()
        .and_then(|v| v.DeviceFamilyVersion())
        .ok()?
        .to_string()
        .parse::<u64>()
        .ok()?;
    Some(format!(
        "Windows {}.{}.{}.{}",
        packed >> 48,
        (packed >> 32) & 0xFFFF,
        (packed >> 16) & 0xFFFF,
        packed & 0xFFFF
    ))
}

/// Heure locale « 14:05:32 », pour dater une lecture.
pub fn clock() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: lecture de l'horloge du système.
    let t = unsafe { GetLocalTime() };
    format!("{:02}:{:02}:{:02}", t.wHour, t.wMinute, t.wSecond)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteTreeW;

    /// Clé de test dédiée : on ne touche jamais à la vraie clé « Run ».
    const TEST_KEY: &str = r"Software\momentum-ctl-win-test";

    #[test]
    fn autostart_round_trip_in_a_test_key() {
        let entry = Autostart::at(TEST_KEY, "test");
        entry.set(false).unwrap();
        assert!(!entry.is_enabled());

        entry.set(true).unwrap();
        assert!(entry.is_enabled());
        assert_eq!(
            entry.registered().unwrap(),
            Some(Autostart::command_line().unwrap())
        );

        entry.set(false).unwrap();
        assert!(!entry.is_enabled());
        // Nettoyage de la clé de test.
        // SAFETY: chaîne valide, clé dédiée aux tests.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(TEST_KEY)) };
    }
}
