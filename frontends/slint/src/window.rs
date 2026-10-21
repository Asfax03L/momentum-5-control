//! Fenêtres de l'interface Slint sous Windows : placement du panneau
//! rapide près de l'icône, premier plan, boîte « Ouvrir ».

use windows::core::HSTRING;

/// Titre du panneau rapide (sert à retrouver sa fenêtre).
pub const PANEL_TITLE: &str = "MOMENTUM 5 — panneau rapide";

/// Zone de travail (sans la barre des tâches) de l'écran qui contient ce
/// point : `(gauche, haut, droite, bas)` en pixels physiques.
pub fn work_area_at(x: i32, y: i32) -> Option<(i32, i32, i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    // SAFETY: appels Win32 en lecture ; la structure est dimensionnée.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        GetMonitorInfoW(monitor, &mut info).as_bool().then_some((
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right,
            info.rcWork.bottom,
        ))
    }
}

/// Position du pointeur de la souris (pixels physiques).
pub fn cursor_position() -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut point = POINT::default();
    // SAFETY: lecture de la position du pointeur.
    let _ = unsafe { GetCursorPos(&mut point) };
    (point.x, point.y)
}

/// Fenêtre du panneau rapide, si elle existe.
fn panel_window() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    // SAFETY: recherche par titre, sans effet de bord.
    unsafe { FindWindowW(None, &HSTRING::from(PANEL_TITLE)).ok() }
}

/// Met le panneau rapide au premier plan (il reçoit alors le clavier).
pub fn focus_panel() {
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    if let Some(hwnd) = panel_window() {
        // SAFETY: fenêtre de ce processus.
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
}

/// Vrai si le panneau rapide est la fenêtre au premier plan.
pub fn panel_has_focus() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    // SAFETY: lecture de la fenêtre au premier plan.
    panel_window().is_some_and(|hwnd| unsafe { GetForegroundWindow() } == hwnd)
}

/// Boîte « Ouvrir » de Windows pour choisir une courbe d'égaliseur (*.txt).
/// `None` si l'utilisateur annule.
pub fn pick_curve_file() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    use windows::core::w;
    // SAFETY: COM initialisé sur ce thread (sans effet s'il l'est déjà) ;
    // la chaîne rendue par GetDisplayName est libérée par CoTaskMemFree.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let filters = [COMDLG_FILTERSPEC {
            pszName: w!("Courbes d'égaliseur (*.txt)"),
            pszSpec: w!("*.txt"),
        }];
        dialog.SetFileTypes(&filters).ok()?;
        dialog
            .SetTitle(w!("Importer une courbe d'égaliseur"))
            .ok()?;
        // Annulation : erreur, donc `None`.
        dialog.Show(None).ok()?;
        let path = dialog
            .GetResult()
            .ok()?
            .GetDisplayName(SIGDN_FILESYSPATH)
            .ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const _));
        text.map(std::path::PathBuf::from)
    }
}
