//! Icône de la zone de notification KDE (StatusNotifierItem, crate `ksni`).
//!
//! Même dessin que WinUI (logo teinté selon le mode), infobulle (nom,
//! batterie, mode) et menu complet (`momentum_core::tray_menu`, le même que
//! WinUI). Les choix du menu sont traités par l'objet `Headset` ; la coche
//! ne change qu'une fois le nouvel état relu sur le casque.

use cxx_qt::CxxQtThread;
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem, SubMenu};
use ksni::{Category, Icon, ToolTip, Tray};
use momentum_core::tray_menu::{MenuAction, MenuEntry};
use momentum_core::ui::{IconKind, icon_rgba};

use crate::bridge::qobject::Headset;

/// Tailles proposées à Plasma (il choisit selon la barre).
const SIZES: [u32; 3] = [22, 32, 64];

/// Ce que l'icône affiche, recopié depuis l'objet `Headset`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrayView {
    pub kind: IconKind,
    /// Infobulle : première ligne en titre, le reste en description.
    pub tooltip: String,
    /// Menu du clic droit.
    pub entries: Vec<MenuEntry>,
}

pub struct HeadsetTray {
    view: TrayView,
    qt_thread: CxxQtThread<Headset>,
}

impl HeadsetTray {
    pub fn new(qt_thread: CxxQtThread<Headset>) -> Self {
        Self {
            view: TrayView {
                kind: IconKind::Disconnected,
                tooltip: "OpenHeadset Control\nRecherche du casque…".into(),
                entries: Vec::new(),
            },
            qt_thread,
        }
    }

    /// Nouvel affichage ; `false` s'il n'a pas changé (rien à republier).
    pub fn set_view(&mut self, view: TrayView) -> bool {
        if self.view == view {
            return false;
        }
        self.view = view;
        true
    }

    fn show_window(&self) {
        let _ = self.qt_thread.queue(|headset| headset.show_requested());
    }

    /// Choix du menu, traité sur le fil de Qt.
    fn choose(&self, action: MenuAction) {
        let _ = self
            .qt_thread
            .queue(move |headset| headset.on_menu_action(action));
    }
}

/// Entrée de `tray_menu` pour `ksni`.
fn menu_item(entry: &MenuEntry) -> MenuItem<HeadsetTray> {
    match entry {
        MenuEntry::Separator => MenuItem::Separator,
        MenuEntry::Info(label) => StandardItem {
            label: label.clone(),
            enabled: false,
            ..Default::default()
        }
        .into(),
        MenuEntry::Item {
            label,
            action,
            enabled,
        } => {
            let action = action.clone();
            StandardItem {
                label: label.clone(),
                enabled: *enabled,
                activate: Box::new(move |tray: &mut HeadsetTray| tray.choose(action.clone())),
                ..Default::default()
            }
            .into()
        }
        MenuEntry::Check {
            label,
            action,
            checked,
            enabled,
        } => {
            let action = action.clone();
            CheckmarkItem {
                label: label.clone(),
                enabled: *enabled,
                checked: *checked,
                activate: Box::new(move |tray: &mut HeadsetTray| tray.choose(action.clone())),
                ..Default::default()
            }
            .into()
        }
        MenuEntry::Submenu {
            label,
            enabled,
            entries,
        } => SubMenu {
            label: label.clone(),
            enabled: *enabled,
            submenu: entries.iter().map(menu_item).collect(),
            ..Default::default()
        }
        .into(),
    }
}

/// RGBA (`icon_rgba`) vers ARGB32 dans l'ordre réseau (SNI).
fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect()
}

impl Tray for HeadsetTray {
    fn id(&self) -> String {
        "org.openheadset.control".into()
    }

    fn title(&self) -> String {
        "OpenHeadset Control".into()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        SIZES
            .iter()
            .map(|&size| Icon {
                width: size as i32,
                height: size as i32,
                data: argb(&icon_rgba(self.view.kind, size)),
            })
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        let (title, description) = self
            .view
            .tooltip
            .split_once('\n')
            .unwrap_or((self.view.tooltip.as_str(), ""));
        ToolTip {
            icon_name: "org.openheadset.control".into(),
            title: title.into(),
            description: description.into(),
            ..Default::default()
        }
    }

    /// Clic gauche : ouvre la fenêtre.
    fn activate(&mut self, _x: i32, _y: i32) {
        self.show_window();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view.entries.iter().map(menu_item).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::argb;

    #[test]
    fn rgba_to_network_argb() {
        assert_eq!(argb(&[1, 2, 3, 4, 5, 6, 7, 8]), [4, 1, 2, 3, 8, 5, 6, 7]);
    }
}
