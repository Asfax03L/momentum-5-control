//! Panneau rapide : petite fenêtre avec la batterie et, au choix de
//! l'utilisateur (`panneau.txt`, comme le client KDE), le mode de bruit, le
//! curseur ANC ↔ transparence, le profil personnalisé et quatre scènes au
//! plus. Ouverte par un raccourci global, par l'action « Panneau rapide » de
//! l'entrée de menu ou par `--panneau`. GNOME n'a pas de zone de
//! notification standard : ce panneau en tient lieu.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use momentum_core::i18n::tr;
use momentum_core::quick_panel::PanelPrefs;
use momentum_core::viewmodel::NOISE_MODES;

#[derive(Clone)]
pub struct Panel {
    pub window: adw::Window,
    pub battery: adw::ActionRow,
    pub battery_bar: gtk::LevelBar,
    pub noise: Vec<gtk::ToggleButton>,
    pub level: gtk::Scale,
    pub profile: adw::SwitchRow,
    /// Parties que l'utilisateur peut retirer du panneau.
    pub modes: gtk::Box,
    pub level_box: gtk::Box,
    pub scenes: gtk::Box,
    pub scene_buttons: Rc<RefCell<Vec<gtk::Button>>>,
    pub open: gtk::Button,
    pub waiting: gtk::Label,
    pub controls: gtk::Box,
}

impl Panel {
    pub fn new(app: &adw::Application) -> Self {
        let battery_bar = gtk::LevelBar::for_interval(0.0, 100.0);
        battery_bar.set_hexpand(true);
        battery_bar.set_valign(gtk::Align::Center);
        battery_bar.set_width_request(120);
        let battery = adw::ActionRow::builder().title(tr("Batterie")).build();
        battery.add_suffix(&battery_bar);

        let modes = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        modes.add_css_class("linked");
        modes.set_halign(gtk::Align::Center);
        let mut noise: Vec<gtk::ToggleButton> = Vec::new();
        for label in NOISE_MODES.map(tr) {
            let button = gtk::ToggleButton::with_label(label);
            if let Some(first) = noise.first() {
                button.set_group(Some(first));
            }
            modes.append(&button);
            noise.push(button);
        }
        let level = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        level.set_hexpand(true);
        level.set_draw_value(true);
        level.update_property(&[gtk::accessible::Property::Label(tr("ANC ↔ transparence"))]);
        let level_label = gtk::Label::new(Some(tr("ANC ↔ transparence")));
        level_label.set_halign(gtk::Align::Start);
        level_label.add_css_class("dim-label");

        let profile = adw::SwitchRow::builder()
            .title(tr("Profil personnalisé"))
            .build();
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.append(&battery);
        list.append(&profile);

        let level_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        level_box.append(&level_label);
        level_box.append(&level);
        let scenes = gtk::Box::new(gtk::Orientation::Vertical, 6);
        scenes.set_visible(false);

        let controls = gtk::Box::new(gtk::Orientation::Vertical, 12);
        controls.append(&list);
        controls.append(&modes);
        controls.append(&level_box);
        controls.append(&scenes);

        let waiting = gtk::Label::new(None);
        waiting.set_wrap(true);
        waiting.add_css_class("dim-label");

        let open = gtk::Button::with_label(tr("Ouvrir OpenHeadset Control"));
        open.add_css_class("pill");
        open.set_halign(gtk::Align::Center);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        column.set_margin_top(12);
        column.set_margin_bottom(18);
        column.set_margin_start(18);
        column.set_margin_end(18);
        column.append(&waiting);
        column.append(&controls);
        column.append(&open);

        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&column));
        let window = adw::Window::builder()
            .application(app)
            .title(tr("Panneau rapide"))
            .icon_name("org.openheadset.control")
            .default_width(360)
            .resizable(false)
            .hide_on_close(true)
            .content(&view)
            .build();
        // Échap ferme le panneau, comme un menu.
        let keys = gtk::EventControllerKey::new();
        let hidden = window.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                hidden.set_visible(false);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        window.add_controller(keys);
        Self {
            window,
            battery,
            battery_bar,
            noise,
            level,
            profile,
            modes,
            level_box,
            scenes,
            scene_buttons: Rc::new(RefCell::new(Vec::new())),
            open,
            waiting,
            controls,
        }
    }

    /// Applique le contenu choisi. `all` : les scènes du fichier, dans leur
    /// ordre ; `on_scene` : demande d'application d'une scène.
    pub fn apply_prefs(
        &self,
        prefs: &PanelPrefs,
        all: &[String],
        writable: bool,
        on_scene: &Rc<dyn Fn(String)>,
    ) {
        self.modes.set_visible(prefs.noise);
        self.level_box.set_visible(prefs.level);
        self.profile.set_visible(prefs.profile);
        let mut buttons = self.scene_buttons.borrow_mut();
        for button in buttons.drain(..) {
            self.scenes.remove(&button);
        }
        for name in prefs.shown_scenes(all) {
            let button = gtk::Button::with_label(&name);
            button.set_sensitive(writable);
            let on_scene = on_scene.clone();
            button.connect_clicked(move |_| on_scene(name.clone()));
            self.scenes.append(&button);
            buttons.push(button);
        }
        self.scenes.set_visible(!buttons.is_empty());
    }
}
