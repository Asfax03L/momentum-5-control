//! Éditeur de scènes du client GNOME : mêmes règles que les autres interfaces
//! (`momentum_core::scene_editor`). Le brouillon n'est écrit dans `scenes.txt`
//! qu'avec « Enregistrer », et rien n'est envoyé au casque.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use momentum_core::i18n::{tr, trf};
use momentum_core::paths;
use momentum_core::scene_editor::{self, SceneEditor};
use momentum_core::viewmodel::{
    SCENE_AUTO_CHOICES, SCENE_NOISE_CHOICES, SCENE_SOUND_CHOICES, SCENE_SWITCH_CHOICES, SceneForm,
};

fn combo(title: &str, labels: &[&'static str]) -> adw::ComboRow {
    let row = adw::ComboRow::builder().title(title).build();
    let translated: Vec<&str> = labels.iter().map(|l| tr(l)).collect();
    row.set_model(Some(&gtk::StringList::new(&translated)));
    row
}

fn selected(row: &adw::ComboRow) -> i32 {
    i32::try_from(row.selected()).unwrap_or(0)
}

fn select(row: &adw::ComboRow, value: i32) {
    row.set_selected(u32::try_from(value).unwrap_or(gtk::INVALID_LIST_POSITION));
}

pub struct ScenePanel {
    editor: RefCell<SceneEditor>,
    /// La fiche est en cours de recopie : ses signaux ne changent rien.
    loading: Cell<bool>,
    list: adw::ComboRow,
    names: RefCell<Vec<String>>,
    name: adw::EntryRow,
    sound: adw::ComboRow,
    spatial: adw::ComboRow,
    noise: adw::ComboRow,
    level: adw::EntryRow,
    anti_wind: adw::ComboRow,
    touch: adw::ComboRow,
    equalizer: adw::ComboRow,
    equalizers: RefCell<Vec<String>>,
    bass: adw::ComboRow,
    auto: adw::ComboRow,
    auto_time: adw::EntryRow,
    preview: adw::ActionRow,
    notice: adw::ActionRow,
    save: gtk::Button,
    cancel: gtk::Button,
    new_scene: gtk::Button,
    duplicate: gtk::Button,
    delete: gtk::Button,
    up: gtk::Button,
    down: gtk::Button,
}

fn button(label: &str) -> gtk::Button {
    let b = gtk::Button::with_label(label);
    b.set_valign(gtk::Align::Center);
    b
}

fn load_editor() -> SceneEditor {
    // Crée les scènes de départ si le fichier manque encore.
    let _ = paths::load_scenes();
    let text = paths::scenes_file().and_then(|p| std::fs::read_to_string(p).ok());
    SceneEditor::load(text, paths::scene_presets())
}

impl ScenePanel {
    pub fn new(parent: &gtk::Box) -> Rc<Self> {
        let list = adw::ComboRow::builder().title(tr("Scène")).build();
        let new_scene = button(tr("Nouvelle"));
        let duplicate = button(tr("Dupliquer"));
        let delete = button(tr("Supprimer"));
        let up = button("↑");
        let down = button("↓");
        up.set_tooltip_text(Some(tr("Monter")));
        down.set_tooltip_text(Some(tr("Descendre")));
        let actions = adw::ActionRow::new();
        for b in [&new_scene, &duplicate, &delete, &up, &down] {
            actions.add_suffix(b);
        }
        let name = adw::EntryRow::builder().title(tr("Nom")).build();
        let sound = combo(tr("Mode audio"), &SCENE_SOUND_CHOICES);
        let spatial = combo(tr("Audio spatial"), &SCENE_SWITCH_CHOICES);
        let noise = combo(tr("Réduction de bruit"), &SCENE_NOISE_CHOICES);
        let level = adw::EntryRow::builder()
            .title(tr("Niveau personnalisé (0 à 100)"))
            .build();
        let anti_wind = combo("Anti-vent", &SCENE_SWITCH_CHOICES);
        let touch = combo(tr("Commandes tactiles"), &SCENE_SWITCH_CHOICES);
        let equalizer = adw::ComboRow::builder().title(tr("Égaliseur")).build();
        let bass = combo(tr("Basses renforcées"), &SCENE_SWITCH_CHOICES);
        let auto = combo(tr("Automatique"), &SCENE_AUTO_CHOICES);
        let auto_time = adw::EntryRow::builder().title(tr("Heure (HH:MM)")).build();
        let preview = adw::ActionRow::builder().title(tr("Aperçu")).build();
        let notice = adw::ActionRow::new();
        let save = button(tr("Enregistrer"));
        let cancel = button(tr("Annuler"));
        notice.add_suffix(&cancel);
        notice.add_suffix(&save);

        let group = adw::PreferencesGroup::builder()
            .title(tr("Éditeur de scènes"))
            .build();
        group.add(&list);
        group.add(&actions);
        group.add(&name);
        group.add(&sound);
        group.add(&spatial);
        group.add(&noise);
        group.add(&level);
        group.add(&anti_wind);
        group.add(&touch);
        group.add(&equalizer);
        group.add(&bass);
        group.add(&auto);
        group.add(&auto_time);
        group.add(&preview);
        group.add(&notice);
        parent.append(&group);

        let panel = Rc::new(Self {
            editor: RefCell::new(load_editor()),
            loading: Cell::new(false),
            list,
            names: RefCell::new(Vec::new()),
            name,
            sound,
            spatial,
            noise,
            level,
            anti_wind,
            touch,
            equalizer,
            equalizers: RefCell::new(Vec::new()),
            bass,
            auto,
            auto_time,
            preview,
            notice,
            save,
            cancel,
            new_scene,
            duplicate,
            delete,
            up,
            down,
        });
        panel.connect();
        panel.show(None);
        panel
    }

    fn connect(self: &Rc<Self>) {
        for row in [
            &self.sound,
            &self.spatial,
            &self.noise,
            &self.anti_wind,
            &self.touch,
            &self.equalizer,
            &self.bass,
            &self.auto,
        ] {
            let this = self.clone();
            row.connect_selected_notify(move |_| this.edited());
        }
        for row in [&self.name, &self.level, &self.auto_time] {
            let this = self.clone();
            row.connect_changed(move |_| this.edited());
        }
        {
            let this = self.clone();
            self.list.connect_selected_notify(move |row| {
                if this.loading.get() {
                    return;
                }
                if let Ok(index) = usize::try_from(row.selected())
                    && index != this.editor.borrow().selected()
                {
                    this.editor.borrow_mut().select(index);
                    this.show(None);
                }
            });
        }
        let action = |b: &gtk::Button, f: fn(&mut SceneEditor)| {
            let this = self.clone();
            b.connect_clicked(move |_| {
                f(&mut this.editor.borrow_mut());
                this.show(None);
            });
        };
        action(&self.new_scene, SceneEditor::new_scene);
        action(&self.duplicate, SceneEditor::duplicate);
        action(&self.delete, SceneEditor::delete);
        action(&self.up, |e| e.move_by(-1));
        action(&self.down, |e| e.move_by(1));
        {
            let this = self.clone();
            self.cancel.connect_clicked(move |_| {
                this.reload(Some(tr("Modifications abandonnées.").to_string()));
            });
        }
        {
            let this = self.clone();
            self.save.connect_clicked(move |_| {
                let result = scene_editor::save(&this.editor.borrow());
                let notice = match result {
                    Ok((message, _)) => message,
                    Err(reason) => trf("Non enregistré : {}", &[&reason]),
                };
                this.reload(Some(notice));
            });
        }
    }

    /// Relit `scenes.txt` (modifications abandonnées), sur la même position.
    pub fn reload(&self, notice: Option<String>) {
        let at = self.editor.borrow().selected();
        let mut editor = load_editor();
        editor.select(at.min(editor.names().len().saturating_sub(1)));
        *self.editor.borrow_mut() = editor;
        self.show(notice);
    }

    /// Le brouillon en cours, qu'une autre page ne doit pas écraser.
    pub fn dirty(&self) -> bool {
        self.editor.borrow().dirty()
    }

    fn edited(&self) {
        if self.loading.get() {
            return;
        }
        let form = SceneForm {
            name: self.name.text().to_string(),
            sound: selected(&self.sound),
            spatial: selected(&self.spatial),
            noise: selected(&self.noise),
            level: self.level.text().to_string(),
            anti_wind: selected(&self.anti_wind),
            touch: selected(&self.touch),
            equalizer: selected(&self.equalizer),
            bass_boost: selected(&self.bass),
            auto: selected(&self.auto),
            auto_time: self.auto_time.text().to_string(),
        };
        self.editor.borrow_mut().set_form(&form);
        // La fiche reste telle que saisie : seul l'état du brouillon change.
        self.show_status(None);
    }

    fn show_status(&self, notice: Option<String>) {
        let editor = self.editor.borrow();
        self.preview.set_subtitle(&glib_escape(&editor.preview()));
        self.save.set_sensitive(editor.can_save());
        self.cancel.set_sensitive(editor.dirty());
        let text = editor
            .problem()
            .map(|p| format!("⚠ {p}"))
            .or(notice)
            .unwrap_or_else(|| {
                tr("Le brouillon n'est écrit qu'avec « Enregistrer » ; rien n'est envoyé au casque.")
                    .to_string()
            });
        self.notice.set_title(&glib_escape(&text));
    }

    fn show(&self, notice: Option<String>) {
        self.loading.set(true);
        let (names, at, form, choices) = {
            let editor = self.editor.borrow();
            (
                editor.names(),
                editor.selected(),
                editor.form(),
                editor.eq_choices().to_vec(),
            )
        };
        if *self.names.borrow() != names {
            let labels: Vec<&str> = names.iter().map(String::as_str).collect();
            self.list.set_model(Some(&gtk::StringList::new(&labels)));
            *self.names.borrow_mut() = names;
        }
        self.list.set_selected(u32::try_from(at).unwrap_or(0));
        if *self.equalizers.borrow() != choices {
            let labels: Vec<&str> = choices.iter().map(String::as_str).collect();
            self.equalizer
                .set_model(Some(&gtk::StringList::new(&labels)));
            *self.equalizers.borrow_mut() = choices;
        }
        self.name.set_text(&form.name);
        select(&self.sound, form.sound);
        select(&self.spatial, form.spatial);
        select(&self.noise, form.noise);
        self.level.set_text(&form.level);
        select(&self.anti_wind, form.anti_wind);
        select(&self.touch, form.touch);
        select(&self.equalizer, form.equalizer);
        select(&self.bass, form.bass_boost);
        select(&self.auto, form.auto);
        self.auto_time.set_text(&form.auto_time);
        let empty = self.editor.borrow().is_empty();
        for b in [&self.duplicate, &self.delete, &self.up, &self.down] {
            b.set_sensitive(!empty);
        }
        self.loading.set(false);
        self.show_status(notice);
    }
}

fn glib_escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}
