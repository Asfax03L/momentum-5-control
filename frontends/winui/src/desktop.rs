//! Fenêtre WinUI vue par Win32 : `windows-reactor` ne permet ni de cacher la
//! fenêtre ni d'intercepter sa fermeture. On retrouve la fenêtre principale
//! du fil de l'interface et on la sous-classe : fermer la cache (l'icône de
//! la zone de notification reste), sauf après « Quitter ».

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, IsIconic, PostMessageW, SW_HIDE, SW_RESTORE, SW_SHOW,
    SetForegroundWindow, ShowWindow, WM_CLOSE,
};
use windows::core::BOOL;

/// Classe des fenêtres de premier niveau de WinUI 3.
const WINUI_CLASS: &str = "WinUIDesktopWin32WindowClass";

/// Sélecteur natif de sauvegarde JSON, sur le fil de l'interface.
pub fn settings_file(save: bool) -> windows::core::Result<Option<std::path::PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog,
        IFileSaveDialog, SIGDN_FILESYSPATH,
    };
    use windows::core::{Interface, w};
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    // SAFETY: initialisation équilibrée sur ce fil, objets COM et chaînes
    // conservés pendant l'appel modal ; mémoire du résultat libérée par COM.
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        let _apartment = Apartment;
        let dialog: IFileDialog = if save {
            let d: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?;
            d.cast()?
        } else {
            let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            d.cast()?
        };
        dialog.SetOptions(dialog.GetOptions()? | FOS_FORCEFILESYSTEM)?;
        dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
            pszName: w!("Réglages JSON"),
            pszSpec: w!("*.json"),
        }])?;
        dialog.SetDefaultExtension(w!("json"))?;
        dialog.SetTitle(if save {
            w!("Exporter les réglages vers un nouveau fichier")
        } else {
            w!("Choisir les réglages à examiner")
        })?;
        if save {
            dialog.SetFileName(w!("openheadset-reglages.json"))?;
        }
        if let Err(error) = dialog.Show(window()) {
            if error.code() == windows::core::HRESULT::from_win32(1223) {
                return Ok(None);
            }
            return Err(error);
        }
        let path = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let chosen = std::path::PathBuf::from(std::ffi::OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0.cast()));
        Ok(Some(chosen))
    }
}

/// Fenêtre principale (0 : pas encore trouvée), utilisable depuis tout fil.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// « Quitter » choisi : la fermeture n'est plus interceptée.
static QUITTING: AtomicBool = AtomicBool::new(false);

fn window() -> Option<HWND> {
    match WINDOW.load(Ordering::Acquire) {
        0 => None,
        raw => Some(HWND(raw as *mut _)),
    }
}

unsafe extern "system" fn find_winui(hwnd: HWND, found: LPARAM) -> BOOL {
    let mut name = [0u16; 64];
    // SAFETY: tampon local, longueur rendue par Windows.
    let len = unsafe { GetClassNameW(hwnd, &mut name) };
    let class = String::from_utf16_lossy(&name[..usize::try_from(len).unwrap_or(0)]);
    if class == WINUI_CLASS {
        // SAFETY: `found` pointe vers l'`isize` de `attach`, vivant pendant
        // l'énumération.
        unsafe { *(found.0 as *mut isize) = hwnd.0 as isize };
        return false.into();
    }
    true.into()
}

unsafe extern "system" fn close_hides(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if message == WM_CLOSE && !QUITTING.load(Ordering::Acquire) {
        // SAFETY: fenêtre de ce fil.
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return LRESULT(0);
    }
    // SAFETY: suite normale du traitement du message.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// À appeler depuis le fil de l'interface, une fois la fenêtre créée :
/// retrouve la fenêtre et intercepte sa fermeture. `false` : pas encore là.
pub fn attach() -> bool {
    if window().is_some() {
        return true;
    }
    let mut found: isize = 0;
    // SAFETY: énumération des fenêtres de ce fil ; `found` vit pendant
    // l'appel.
    unsafe {
        let _ = EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_winui),
            LPARAM(&mut found as *mut isize as isize),
        );
    }
    if found == 0 {
        return false;
    }
    let hwnd = HWND(found as *mut _);
    // SAFETY: fenêtre de ce fil ; le sous-classement dure autant qu'elle.
    let hooked = unsafe { SetWindowSubclass(hwnd, Some(close_hides), 1, 0) };
    if !hooked.as_bool() {
        return false;
    }
    WINDOW.store(found, Ordering::Release);
    true
}

/// Montre la fenêtre au premier plan (depuis tout fil).
pub fn show() {
    if let Some(hwnd) = window() {
        // SAFETY: fenêtre valide pendant toute la vie du processus.
        unsafe {
            let _ = ShowWindow(
                hwnd,
                if IsIconic(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Cache la fenêtre (démarrage avec `--cache`).
pub fn hide() {
    if let Some(hwnd) = window() {
        // SAFETY: fenêtre valide.
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Ferme vraiment la fenêtre, ce qui termine l'application.
pub fn quit() {
    QUITTING.store(true, Ordering::Release);
    match window() {
        // SAFETY: fenêtre valide ; message posté à son fil.
        Some(hwnd) => unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        },
        None => std::process::exit(0),
    }
}

/// Copie un texte dans le presse-papiers (Unicode) ; `false` si Windows le
/// refuse (presse-papiers ouvert par une autre application).
pub fn copy_text(text: &str) -> bool {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: presse-papiers ouvert puis refermé dans ce bloc ; la mémoire
    // allouée appartient au presse-papiers une fois `SetClipboardData` réussi.
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let copied = (|| -> windows::core::Result<()> {
            EmptyClipboard()?;
            let memory: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let target = GlobalLock(memory).cast::<u16>();
            if target.is_null() {
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(memory);
            SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0)))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        copied.is_ok()
    }
}

/// Choisit un fichier de courbe d'égaliseur (`*.txt`) ; `None` si
/// l'utilisateur annule. Boîte de dialogue modale sur la fenêtre.
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
        dialog.Show(window()).ok()?;
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

/// Panneau rapide ouvert (0 : aucun).
static PANEL: AtomicIsize = AtomicIsize::new(0);

fn panel() -> Option<HWND> {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;
    match PANEL.load(Ordering::Acquire) {
        0 => None,
        // SAFETY: simple vérification d'une poignée de fenêtre.
        raw => Some(HWND(raw as *mut _)).filter(|h| unsafe { IsWindow(Some(*h)) }.as_bool()),
    }
}

unsafe extern "system" fn find_titled(hwnd: HWND, found: LPARAM) -> BOOL {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut title = [0u16; 128];
    // SAFETY: tampon local.
    let len = unsafe { GetWindowTextW(hwnd, &mut title) };
    let title = String::from_utf16_lossy(&title[..usize::try_from(len).unwrap_or(0)]);
    if title == PANEL_TITLE {
        // SAFETY: `found` pointe vers l'`isize` de `place_panel`.
        unsafe { *(found.0 as *mut isize) = hwnd.0 as isize };
        return false.into();
    }
    true.into()
}

unsafe extern "system" fn close_when_inactive(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{WA_INACTIVE, WM_ACTIVATE};
    if message == WM_ACTIVATE && (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
        // SAFETY: fenêtre de ce fil ; fermeture différée.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    // SAFETY: suite normale du traitement du message.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// Titre de la fenêtre du panneau rapide (sert à la retrouver).
pub const PANEL_TITLE: &str = "OpenHeadset Control — panneau rapide";

/// Depuis le fil de l'interface, une fois le panneau créé : le place près
/// du pointeur (l'icône), hors de la barre des tâches, toujours au-dessus,
/// et le ferme quand il perd le focus. `false` : pas encore là.
pub fn place_panel() -> bool {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST,
        SWP_FRAMECHANGED, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW,
        WS_EX_TOOLWINDOW,
    };

    let mut found: isize = 0;
    // SAFETY: énumération des fenêtres de ce fil ; `found` vit pendant l'appel.
    unsafe {
        let _ = EnumThreadWindows(
            GetCurrentThreadId(),
            Some(find_titled),
            LPARAM(&mut found as *mut isize as isize),
        );
    }
    if found == 0 {
        return false;
    }
    let hwnd = HWND(found as *mut _);
    // SAFETY: fenêtre de ce fil ; structures locales.
    unsafe {
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let work: RECT = info.rcWork;
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let (x, y) = momentum_core::ui::panel_position(
            (cursor.x, cursor.y),
            (rect.right - rect.left, rect.bottom - rect.top),
            (work.left, work.top, work.right, work.bottom),
        );
        // Fenêtre d'outil : pas de bouton dans la barre des tâches.
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let ex = (ex | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_FRAMECHANGED,
        );
        let _ = SetWindowSubclass(hwnd, Some(close_when_inactive), 2, 0);
        let _ = SetForegroundWindow(hwnd);
    }
    PANEL.store(found, Ordering::Release);
    true
}

/// Panneau rapide ouvert et visible.
pub fn panel_open() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
    // SAFETY: poignée vérifiée par `panel`.
    panel().is_some_and(|h| unsafe { IsWindowVisible(h) }.as_bool())
}

/// Ferme le panneau rapide.
pub fn close_panel() {
    if let Some(hwnd) = panel() {
        // SAFETY: fenêtre valide ; fermeture différée.
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    PANEL.store(0, Ordering::Release);
}
