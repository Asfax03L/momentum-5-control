//! Icône de la zone de notification KDE, écrite directement sur zbus :
//! StatusNotifierItem (`org.kde.StatusNotifierItem`) et son menu DBusMenu
//! (`com.canonical.dbusmenu`), d'après les spécifications freedesktop.
//!
//! Même dessin que WinUI (logo teinté selon le mode), infobulle (nom,
//! batterie, mode) et menu complet (`momentum_core::tray_menu`, le même que
//! WinUI). Les choix du menu sont traités par l'objet `Headset` ; la coche
//! ne change qu'une fois le nouvel état relu sur le casque.

use momentum_core::i18n::{tr, trf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use cxx_qt::CxxQtThread;
use momentum_core::tray_menu::{MenuAction, MenuEntry};
use momentum_core::ui::{IconKind, icon_rgba};
use serde::Serialize;
use zbus::object_server::SignalEmitter;
use zbus::wire::{OwnedObjectPath, OwnedVariant, Structure, Type, Variant};
use zbus::{Connection, block_on};

use crate::bridge::qobject::Headset;

/// Tailles proposées à Plasma (il choisit selon la barre).
const SIZES: [u32; 3] = [22, 32, 64];
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

/// Ce que l'icône affiche, recopié depuis l'objet `Headset`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrayView {
    pub kind: IconKind,
    /// Infobulle : première ligne en titre, le reste en description.
    pub tooltip: String,
    /// Menu du clic droit.
    pub entries: Vec<MenuEntry>,
}

/// État partagé entre l'objet `Headset` et les deux interfaces D-Bus.
struct Shared {
    view: TrayView,
    /// Version du menu, augmentée à chaque changement (`LayoutUpdated`).
    revision: u32,
}

type SharedRef = Arc<Mutex<Shared>>;

fn lock(shared: &SharedRef) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// Icône publiée ; la connexion garde le nom D-Bus tant qu'elle vit.
pub struct Tray {
    connection: Connection,
    shared: SharedRef,
}

impl Tray {
    /// Publie l'icône et l'inscrit auprès de la zone de notification.
    /// `Err` : aucune zone de notification n'écoute (autre bureau).
    pub fn spawn(qt_thread: CxxQtThread<Headset>) -> zbus::Result<Self> {
        let shared = Arc::new(Mutex::new(Shared {
            view: TrayView {
                kind: IconKind::Disconnected,
                tooltip: tr("OpenHeadset Control\nRecherche du casque…").into(),
                entries: Vec::new(),
            },
            revision: 1,
        }));
        let connection = block_on(async {
            let connection = Connection::session().await?;
            let server = connection.object_server();
            server
                .at(
                    ITEM_PATH,
                    Item {
                        qt_thread: qt_thread.clone(),
                        shared: Arc::clone(&shared),
                    },
                )
                .await?;
            server
                .at(
                    MENU_PATH,
                    Menu {
                        qt_thread,
                        shared: Arc::clone(&shared),
                    },
                )
                .await?;
            // Le nom unique de la connexion suffit (spécification SNI) :
            // aucun nom org.kde.* à posséder, donc aucune permission
            // --own-name=org.kde.* dans le Flatpak.
            let name = connection
                .unique_name()
                .map(|name| name.to_string())
                .ok_or_else(|| zbus::Error::Failure("bus sans nom unique".into()))?;
            register(&connection, &name).await?;
            zbus::Result::Ok((connection, name))
        })?;
        let (connection, name) = connection;
        watch_restarts(connection.clone(), name);
        Ok(Self { connection, shared })
    }

    /// Nouvel affichage : republié seulement s'il a changé.
    pub fn update(&self, view: TrayView) {
        let (menu_changed, revision) = {
            let mut shared = lock(&self.shared);
            if shared.view == view {
                return;
            }
            let menu_changed = shared.view.entries != view.entries;
            if menu_changed {
                shared.revision = shared.revision.wrapping_add(1);
            }
            shared.view = view;
            (menu_changed, shared.revision)
        };
        let result = block_on(async {
            let item_emitter = SignalEmitter::new(&self.connection, ITEM_PATH)?;
            Item::new_icon(&item_emitter).await?;
            Item::new_tool_tip(&item_emitter).await?;
            if menu_changed {
                let menu_emitter = SignalEmitter::new(&self.connection, MENU_PATH)?;
                Menu::layout_updated(&menu_emitter, revision, 0).await?;
            }
            zbus::Result::Ok(())
        });
        if let Err(e) = result {
            tracing::warn!(%e, "mise à jour de l'icône de la zone de notification");
        }
    }
}

/// Inscription auprès de la zone de notification de Plasma.
async fn register(connection: &Connection, name: &str) -> zbus::Result<()> {
    connection
        .call_method(
            Some(WATCHER),
            "/StatusNotifierWatcher",
            Some(WATCHER),
            "RegisterStatusNotifierItem",
            &(name,),
        )
        .await?;
    Ok(())
}

/// Plasma relancé : la nouvelle zone de notification ne connaît pas
/// l'icône, qui se réinscrit dès qu'elle apparaît.
fn watch_restarts(connection: Connection, name: String) {
    let spawned = std::thread::Builder::new()
        .name("icone-kde".into())
        .spawn(move || {
            let result = block_on(async {
                let bus = zbus::fdo::DBusProxy::new(&connection).await?;
                let mut changes = bus
                    .receive_name_owner_changed_with_args(&[(0, WATCHER)])
                    .await?;
                while let Some(change) = next(&mut changes).await {
                    let Ok(args) = change.args() else { continue };
                    if args.new_owner().is_some()
                        && let Err(e) = register(&connection, &name).await
                    {
                        tracing::warn!(%e, "réinscription de l'icône");
                    }
                }
                zbus::Result::Ok(())
            });
            if let Err(e) = result {
                tracing::warn!(%e, "suivi de la zone de notification");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(%e, "suivi de la zone de notification");
    }
}

async fn next<S: futures_core::Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    std::future::poll_fn(|cx| std::pin::Pin::new(&mut *stream).poll_next(cx)).await
}

/// Image ARGB32 (ordre réseau) d'une icône SNI.
type Pixmap = (i32, i32, Vec<u8>);

/// RGBA (`icon_rgba`) vers ARGB32 dans l'ordre réseau (SNI).
fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, a]| [a, r, g, b])
        .collect()
}

/// L'icône elle-même (`org.kde.StatusNotifierItem`).
struct Item {
    qt_thread: CxxQtThread<Headset>,
    shared: SharedRef,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "Hardware"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "org.openheadset.control"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        tr("OpenHeadset Control")
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        let kind = lock(&self.shared).view.kind;
        SIZES
            .iter()
            .map(|&size| (size as i32, size as i32, argb(&icon_rgba(kind, size))))
            .collect()
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    /// Infobulle : (nom d'icône, images, titre, description).
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        let tooltip = lock(&self.shared).view.tooltip.clone();
        let (title, description) = tooltip.split_once('\n').unwrap_or((&tooltip, ""));
        (
            "org.openheadset.control".into(),
            Vec::new(),
            title.into(),
            description.into(),
        )
    }

    /// Le clic gauche active l'icône ; le menu est au clic droit.
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("chemin D-Bus valide")
    }

    /// Clic gauche : ouvre ou ferme le panneau rapide (comme sous Windows).
    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.qt_thread.queue(|headset| headset.panel_requested());
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {}

    /// Menu demandé à l'icône elle-même : Plasma passe par `Menu`.
    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: &str) {}

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Élément de menu DBusMenu : (numéro, propriétés, sous-éléments).
#[derive(Debug, Serialize, Type)]
struct Layout(i32, HashMap<String, OwnedVariant>, Vec<OwnedVariant>);

/// Élément de menu à plat : propriétés, action, enfants.
struct Node {
    props: HashMap<String, OwnedVariant>,
    action: Option<MenuAction>,
    children: Vec<i32>,
}

/// Menu numéroté dans l'ordre (0 : racine), recalculé depuis la vue.
fn flatten(entries: &[MenuEntry]) -> Vec<Node> {
    let mut nodes = vec![Node {
        props: HashMap::from([("children-display".into(), owned("submenu"))]),
        action: None,
        children: Vec::new(),
    }];
    add_children(&mut nodes, 0, entries);
    nodes
}

fn add_children(nodes: &mut Vec<Node>, parent: usize, entries: &[MenuEntry]) {
    for entry in entries {
        let id = nodes.len();
        let mut props = HashMap::new();
        let mut action = None;
        let mut submenu: &[MenuEntry] = &[];
        match entry {
            MenuEntry::Separator => {
                props.insert("type".into(), owned("separator"));
            }
            MenuEntry::Info(text) => {
                props.insert("label".into(), owned(label(text)));
                props.insert("enabled".into(), owned(false));
            }
            MenuEntry::Item {
                label: text,
                action: a,
                enabled,
            } => {
                props.insert("label".into(), owned(label(text)));
                props.insert("enabled".into(), owned(*enabled));
                action = Some(a.clone());
            }
            MenuEntry::Check {
                label: text,
                action: a,
                checked,
                enabled,
            } => {
                props.insert("label".into(), owned(label(text)));
                props.insert("enabled".into(), owned(*enabled));
                props.insert("toggle-type".into(), owned("checkmark"));
                props.insert("toggle-state".into(), owned(i32::from(*checked)));
                action = Some(a.clone());
            }
            MenuEntry::Submenu {
                label: text,
                enabled,
                entries,
            } => {
                props.insert("label".into(), owned(label(text)));
                props.insert("enabled".into(), owned(*enabled));
                props.insert("children-display".into(), owned("submenu"));
                submenu = entries;
            }
        }
        nodes.push(Node {
            props,
            action,
            children: Vec::new(),
        });
        nodes[parent].children.push(id as i32);
        add_children(nodes, id, submenu);
    }
}

/// DBusMenu lit `_` comme marque de raccourci clavier : il est doublé.
fn label(text: &str) -> String {
    text.replace('_', "__")
}

fn owned<'a>(value: impl Into<Variant<'a>>) -> OwnedVariant {
    OwnedVariant::try_from(value.into()).expect("valeur D-Bus sans descripteur de fichier")
}

/// Propriétés demandées (toutes si la liste est vide).
fn filtered(
    props: &HashMap<String, OwnedVariant>,
    names: &[String],
) -> HashMap<String, OwnedVariant> {
    props
        .iter()
        .filter(|(k, _)| names.is_empty() || names.contains(k))
        .map(|(k, v)| (k.clone(), v.try_clone().expect("valeur D-Bus copiable")))
        .collect()
}

/// Sous-arbre `id`, sur `depth` niveaux (-1 : tous).
fn layout(nodes: &[Node], id: i32, depth: i32, names: &[String]) -> Layout {
    let node = &nodes[id as usize];
    let children = if depth == 0 {
        Vec::new()
    } else {
        node.children
            .iter()
            .map(|&child| {
                let Layout(id, props, children) = layout(nodes, child, depth - 1, names);
                owned(Structure::from((id, props, children)))
            })
            .collect()
    };
    Layout(id, filtered(&node.props, names), children)
}

/// Menu de l'icône (`com.canonical.dbusmenu`).
struct Menu {
    qt_thread: CxxQtThread<Headset>,
    shared: SharedRef,
}

impl Menu {
    fn nodes(&self) -> (u32, Vec<Node>) {
        let shared = lock(&self.shared);
        (shared.revision, flatten(&shared.view.entries))
    }

    /// Clic sur l'élément `id` ; `false` s'il n'existe pas.
    fn clicked(&self, id: i32) -> bool {
        let (_, nodes) = self.nodes();
        let Some(node) = usize::try_from(id).ok().and_then(|i| nodes.get(i)) else {
            return false;
        };
        if let Some(action) = node.action.clone() {
            let _ = self
                .qt_thread
                .queue(move |headset| headset.on_menu_action(action));
        }
        true
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, Layout)> {
        let (revision, nodes) = self.nodes();
        if usize::try_from(parent_id).map_or(true, |i| i >= nodes.len()) {
            return Err(zbus::fdo::Error::InvalidArgs(trf(
                "élément de menu inconnu : {}",
                &[&parent_id],
            )));
        }
        Ok((
            revision,
            layout(&nodes, parent_id, recursion_depth, &property_names),
        ))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedVariant>)> {
        let (_, nodes) = self.nodes();
        nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (i as i32, node))
            .filter(|(i, _)| ids.is_empty() || ids.contains(i))
            .map(|(i, node)| (i, filtered(&node.props, &property_names)))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedVariant> {
        let (_, nodes) = self.nodes();
        usize::try_from(id)
            .ok()
            .and_then(|i| nodes.get(i))
            .and_then(|node| node.props.get(&name))
            .map(|v| v.try_clone().expect("valeur D-Bus copiable"))
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(trf("propriété inconnue : {}", &[&name])))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedVariant, _timestamp: u32) {
        if event_id == "clicked" {
            self.clicked(id);
        }
    }

    /// Événements groupés ; rend les numéros inconnus.
    fn event_group(&self, events: Vec<(i32, String, OwnedVariant, u32)>) -> Vec<i32> {
        events
            .into_iter()
            .filter(|(id, event_id, _, _)| event_id == "clicked" && !self.clicked(*id))
            .map(|(id, _, _, _)| id)
            .collect()
    }

    /// Le menu est toujours à jour : rien à recharger avant l'ouverture.
    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_to_network_argb() {
        assert_eq!(argb(&[1, 2, 3, 4, 5, 6, 7, 8]), [4, 1, 2, 3, 8, 5, 6, 7]);
    }

    #[test]
    fn menu_is_numbered_in_order() {
        let entries = vec![
            MenuEntry::Info("Casque_1".into()),
            MenuEntry::Separator,
            MenuEntry::Submenu {
                label: "Bruit".into(),
                enabled: true,
                entries: vec![MenuEntry::Check {
                    label: "Anti-vent".into(),
                    action: MenuAction::AntiWind,
                    checked: true,
                    enabled: true,
                }],
            },
            MenuEntry::Item {
                label: "Quitter".into(),
                action: MenuAction::Quit,
                enabled: true,
            },
        ];
        let nodes = flatten(&entries);
        assert_eq!(nodes.len(), 6);
        assert_eq!(nodes[0].children, [1, 2, 3, 5]);
        assert_eq!(nodes[3].children, [4]);
        assert_eq!(nodes[4].action, Some(MenuAction::AntiWind));
        assert_eq!(nodes[5].action, Some(MenuAction::Quit));
        let label = String::try_from(nodes[1].props["label"].try_clone().unwrap()).unwrap();
        assert_eq!(label, "Casque__1");
        let state = i32::try_from(nodes[4].props["toggle-state"].try_clone().unwrap()).unwrap();
        assert_eq!(state, 1);
        // Profondeur 1 : les enfants de la racine, sans leurs sous-menus.
        let Layout(id, _, children) = layout(&nodes, 0, 1, &[]);
        assert_eq!((id, children.len()), (0, 4));
    }
}
