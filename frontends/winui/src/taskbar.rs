//! Barre des tâches Windows : liste de raccourcis (clic droit sur l'icône)
//! pour appliquer une scène, et pastille de batterie sur l'icône.
//!
//! Une entrée de la liste relance cette interface avec `--scene <nom>` :
//! ce lancement envoie la scène au service par l'API (mêmes refus que
//! l'interface), attend son résultat, montre une notification s'il échoue,
//! puis se ferme sans ouvrir de fenêtre.

use std::sync::mpsc;
use std::time::Duration;

use momentum_client::{Client, Command, Event};
use momentum_core::ui::battery_badge_rgba;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HGDIOBJ,
};
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
    ITaskbarList3, ShellLink, TaskbarList,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};
use windows::core::{HSTRING, Interface};

/// Argument d'une entrée de la liste de raccourcis.
pub const SCENE_FLAG: &str = "--scene";
/// Entrées de scène au plus (la liste de Windows est courte).
const MAX_SCENES: usize = 10;
/// Attente du service puis du résultat de la scène.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RESULT_TIMEOUT: Duration = Duration::from_secs(60);
/// Taille de la pastille (icône superposée, 16 × 16 à 100 %).
const BADGE_SIZE: u32 = 16;

/// Nom de scène passé par `--scene <nom>`, s'il y en a un.
pub fn scene_argument() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == SCENE_FLAG {
            return args.next();
        }
    }
    None
}

/// Applique la scène par le service (lancé au besoin), puis rend le code de
/// sortie : 0 si elle est appliquée.
pub fn apply_scene(name: &str, start_daemon: impl Fn()) -> i32 {
    let result = submit_scene(name, start_daemon);
    match result {
        Ok(()) => 0,
        Err(reason) => {
            momentum_service::desktop::show_toast(&format!("Scène « {name} »"), &reason);
            1
        }
    }
}

fn submit_scene(name: &str, start_daemon: impl Fn()) -> Result<(), String> {
    let client = connect(start_daemon)?;
    // Abonnement avant l'envoi : le résultat ne peut pas être manqué.
    let events = client.events().map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for event in events {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    let id = client
        .submit(&Command::ApplyScene(name.to_string()), 0)
        .map_err(|e| format!("refusée : {e}"))?;
    let deadline = std::time::Instant::now() + RESULT_TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(Event::Done {
                id: done, outcome, ..
            }) if done == id => {
                return if outcome.is_success() {
                    Ok(())
                } else {
                    Err(outcome.describe())
                };
            }
            Ok(_) => {}
            Err(_) => return Err("pas de réponse du service".to_string()),
        }
    }
}

/// Connexion au service, lancé s'il ne répond pas encore.
fn connect(start_daemon: impl Fn()) -> Result<Client, String> {
    if let Ok(client) = Client::connect() {
        return Ok(client);
    }
    start_daemon();
    let deadline = std::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        std::thread::sleep(Duration::from_millis(250));
        match Client::connect() {
            Ok(client) => return Ok(client),
            Err(e) if std::time::Instant::now() >= deadline => {
                return Err(format!("service injoignable : {e}"));
            }
            Err(_) => {}
        }
    }
}

/// Liste de raccourcis : une entrée par scène (les premières), qui relance
/// cette interface avec `--scene`. Liste vide : retirée.
pub fn update_jump_list(scenes: &[String]) -> windows::core::Result<()> {
    let exe = HSTRING::from(
        std::env::current_exe()
            .map_err(|_| windows::core::Error::from_thread())?
            .as_os_str(),
    );
    // SAFETY: objets COM du shell sur le fil de l'interface (COM initialisé
    // par WinUI) ; chaînes HSTRING vivantes pendant les appels.
    unsafe {
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        // Noms sans guillemet seulement : l'argument est entre guillemets.
        let usable: Vec<&String> = scenes
            .iter()
            .filter(|s| !s.contains('"') && !s.trim().is_empty())
            .take(MAX_SCENES)
            .collect();
        if usable.is_empty() {
            return list.DeleteList(None);
        }
        let mut slots = 0u32;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        let tasks: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for name in usable {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&exe)?;
            link.SetArguments(&HSTRING::from(format!("{SCENE_FLAG} \"{name}\"")))?;
            link.SetIconLocation(&exe, 0)?;
            link.SetDescription(&HSTRING::from(format!(
                "Appliquer la scène « {name} » au casque"
            )))?;
            let store: IPropertyStore = link.cast()?;
            store.SetValue(
                &PKEY_Title,
                &PROPVARIANT::from(format!("Scène « {name} »").as_str()),
            )?;
            store.Commit()?;
            tasks.AddObject(&link)?;
        }
        list.AddUserTasks(&tasks.cast::<IObjectArray>()?)?;
        list.CommitList()
    }
}

/// Pastille de batterie sur l'icône de la fenêtre dans la barre des tâches
/// (`None` : retirée, casque absent).
pub fn set_battery_badge(hwnd: HWND, level: Option<u8>) -> windows::core::Result<()> {
    // SAFETY: objet COM du shell sur le fil de l'interface ; l'icône créée
    // est détruite après l'appel (la barre des tâches en garde une copie).
    unsafe {
        let taskbar: ITaskbarList3 = CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)?;
        taskbar.HrInit()?;
        match level {
            None => taskbar.SetOverlayIcon(hwnd, HICON::default(), None),
            Some(level) => {
                let icon = badge_icon(level)?;
                let text = HSTRING::from(format!("Batterie du casque : {level} %"));
                let result = taskbar.SetOverlayIcon(hwnd, icon, &text);
                let _ = DestroyIcon(icon);
                result
            }
        }
    }
}

/// Icône Windows de la pastille (BGRA 32 bits avec alpha).
unsafe fn badge_icon(level: u8) -> windows::core::Result<HICON> {
    let rgba = battery_badge_rgba(level, BADGE_SIZE);
    let size = BADGE_SIZE as i32;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            // Négatif : lignes de haut en bas.
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: section DIB de BADGE_SIZE² pixels 32 bits, remplie ci-dessous
    // dans ses limites ; bitmaps libérés après la création de l'icône.
    unsafe {
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), rgba.len());
        for (dst, src) in pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(rgba.as_chunks::<4>().0)
        {
            *dst = [src[2], src[1], src[0], src[3]];
        }
        let mask = CreateBitmap(size, size, 1, 1, None);
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        });
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        icon
    }
}
