//! Heure locale du système, pour les scènes automatiques à heure fixe.

/// Minute locale courante : (jour, heure, minute). Le jour sert seulement à
/// ne déclencher qu'une fois par jour (valeur croissante, pas une date).
#[cfg(windows)]
pub fn local_minute() -> Option<(u32, u8, u8)> {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: lecture de l'horloge du système.
    let t = unsafe { GetLocalTime() };
    let day = u32::from(t.wYear) * 400 + u32::from(t.wMonth) * 32 + u32::from(t.wDay);
    Some((
        day,
        u8::try_from(t.wHour).ok()?,
        u8::try_from(t.wMinute).ok()?,
    ))
}

/// Minute locale courante : (jour, heure, minute). Le jour sert seulement à
/// ne déclencher qu'une fois par jour (valeur croissante, pas une date).
#[cfg(target_os = "linux")]
pub fn local_minute() -> Option<(u32, u8, u8)> {
    // SAFETY: `time` sans pointeur ; `localtime_r` remplit la structure
    // locale fournie (version réentrante, sans état partagé).
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return None;
        }
        tm
    };
    let day = u32::try_from(tm.tm_year).ok()? * 400 + u32::try_from(tm.tm_yday).ok()?;
    Some((
        day,
        u8::try_from(tm.tm_hour).ok()?,
        u8::try_from(tm.tm_min).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_minute_is_plausible() {
        let (_, hour, minute) = super::local_minute().expect("heure locale");
        assert!(hour < 24 && minute < 60);
    }
}
