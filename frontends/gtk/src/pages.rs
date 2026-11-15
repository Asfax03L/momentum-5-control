//! Pages du client GNOME au-delà de l'accueil : Mon son, Scènes, Paramètres,
//! Batterie, Connexion et audio, À propos. Tout passe par le service ou par les
//! fichiers locaux du dossier des données ; aucune connexion au casque ici.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::glib;
use momentum_client::demo;
use momentum_client::sound_check::{Action as GuidedAction, GuidedView};
use momentum_client::{Command, Snapshot};
use momentum_core::alerts::NotificationPrefs;
use momentum_core::battery_log::{self, BatteryLog};
use momentum_core::bluetooth_codec::{self, CodecCard};
use momentum_core::device::{AUTO_OFF_CHOICES, PromptMode, SIDETONE_LEVELS, check_name};
use momentum_core::diagnostic::{self, DiagnosticInput};
use momentum_core::i18n::{Lang, load_pref, store_pref, tr, trf};
use momentum_core::paths;
use momentum_core::pipewire_audio::{AudioReadError, PipeWireAudio};
use momentum_core::presets::{self, AbCompare, Preset};
use momentum_core::settings_transfer::{self, ImportPlan};
use momentum_core::viewmodel::{self, AUDIO_MODES, AUTO_OFF_LABELS, PROMPT_MODES, SIDETONE_LABELS};

use crate::pipewire;
use crate::scene_panel::ScenePanel;

/// Envoi d'une demande au service, suivi par la fenêtre.
pub type Send = Rc<dyn Fn(Command)>;
/// Message court à l'utilisateur.
pub type Notify = Rc<dyn Fn(String)>;
/// Action de l'assistant EQ, tenu par la fenêtre.
pub type Guided = Rc<dyn Fn(GuidedAction)>;
/// Enregistre le résultat de l'assistant sous ce nom ; la liste à jour, ou non.
pub type GuidedSave = Rc<dyn Fn(&str) -> Option<Vec<Preset>>>;

/// Lancement à l'ouverture de session : entrée XDG, suivie par GNOME.
mod autostart {
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(config.join("autostart").join("momentum-gtk.desktop"))
    }

    pub fn is_enabled() -> bool {
        path().is_some_and(|p| p.exists())
    }

    pub fn set(on: bool) -> std::io::Result<()> {
        let path = path().ok_or_else(|| std::io::Error::other("XDG_CONFIG_HOME"))?;
        if on {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let exe = std::env::current_exe()?;
            std::fs::write(
                path,
                format!(
                    "[Desktop Entry]\nType=Application\nName=OpenHeadset Control\n\
                     Exec=\"{}\" --cache\nIcon=org.openheadset.control\nTerminal=false\n",
                    exe.display()
                ),
            )
        } else {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
}

/// Dessine un tracé `M x y L x y …` du repère 1000 × 100 (`chart_path`).
fn draw_chart(context: &gtk::cairo::Context, path: &str, width: f64, height: f64) {
    // Marge intérieure : un point à 100 % ou au bord droit reste visible.
    const MARGIN: f64 = 12.0;
    let (w, h) = (
        (width - 2.0 * MARGIN).max(1.0),
        (height - 2.0 * MARGIN).max(1.0),
    );
    // Repères discrets à 0, 50 et 100 %.
    context.set_source_rgba(0.5, 0.5, 0.5, 0.25);
    context.set_line_width(1.0);
    for level in [0.0, 50.0, 100.0] {
        let y = MARGIN + level / 100.0 * h;
        context.move_to(MARGIN, y);
        context.line_to(MARGIN + w, y);
    }
    let _ = context.stroke();
    let mut points = Vec::new();
    let mut words = path.split_whitespace();
    while let (Some(command), Some(x), Some(y)) = (words.next(), words.next(), words.next()) {
        let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) else {
            return;
        };
        points.push((
            command == "M",
            MARGIN + x / 1000.0 * w,
            MARGIN + y / 100.0 * h,
        ));
    }
    context.set_source_rgb(0.2, 0.6, 0.9);
    context.set_line_width(2.0);
    for &(start, x, y) in &points {
        if start {
            context.move_to(x, y);
        } else {
            context.line_to(x, y);
        }
    }
    let _ = context.stroke();
    // Chaque mesure est marquée : quelques mesures serrées restent lisibles.
    for &(_, x, y) in &points {
        context.arc(x, y, 3.0, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
    }
}

/// Résultat d'une lecture faite hors du fil de l'interface.
enum AudioRead {
    /// Connexion et empreinte de la lecture, puis son résultat.
    Pipewire(u64, String, Result<PipeWireAudio, AudioReadError>),
    Codecs(Option<CodecCard>),
    CodecSet(Result<CodecCard, String>),
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn index(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(gtk::INVALID_LIST_POSITION)
}

fn choices(title: &str, labels: &[&'static str]) -> adw::ComboRow {
    let row = adw::ComboRow::builder().title(title).build();
    let translated: Vec<&str> = labels.iter().map(|l| tr(l)).collect();
    row.set_model(Some(&gtk::StringList::new(&translated)));
    row
}

fn page(name: &str, title: &str, icon: &str, stack: &adw::ViewStack) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 24);
    column.set_margin_top(12);
    column.set_margin_bottom(24);
    column.set_margin_start(12);
    column.set_margin_end(12);
    let clamp = adw::Clamp::builder()
        .maximum_size(640)
        .child(&column)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    stack.add_titled_with_icon(&scrolled, Some(name), title, icon);
    column
}

fn boxed_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list
}

/// Liste de lignes « titre / détail », refaite seulement si son contenu change.
struct Lines {
    list: gtk::ListBox,
    last: RefCell<Vec<(String, String)>>,
}

impl Lines {
    fn new() -> Self {
        Self {
            list: boxed_list(),
            last: RefCell::new(Vec::new()),
        }
    }

    fn set(&self, lines: Vec<(String, String)>) {
        if *self.last.borrow() == lines {
            return;
        }
        self.list.remove_all();
        for (title, subtitle) in &lines {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(title).as_str())
                .subtitle(glib::markup_escape_text(subtitle).as_str())
                .build();
            row.add_css_class("property");
            self.list.append(&row);
        }
        *self.last.borrow_mut() = lines;
    }
}

/// Contrôles des pages, mis à jour à chaque changement d'état.
pub struct Extras {
    pub stack: adw::ViewStack,
    // Accueil.
    head_tracking: adw::SwitchRow,
    touch: adw::SwitchRow,
    multipoint: adw::SwitchRow,
    devices: gtk::ListBox,
    devices_last: RefCell<Vec<(String, String, String, bool)>>,
    // Mon son.
    audio_mode: adw::ComboRow,
    band_group: adw::PreferencesGroup,
    bands: RefCell<Vec<gtk::Scale>>,
    bass: adw::SwitchRow,
    flat: gtk::Button,
    preset: adw::ComboRow,
    preset_names: RefCell<Vec<String>>,
    preset_apply: gtk::Button,
    preset_delete: gtk::Button,
    preset_name: adw::EntryRow,
    preset_status: adw::ActionRow,
    user_presets: RefCell<Vec<Preset>>,
    ab: RefCell<AbCompare>,
    ab_remember: gtk::Button,
    ab_toggle: gtk::Button,
    curve_export: gtk::Button,
    curve_import: gtk::Button,
    profile_export: gtk::Button,
    bind_legacy: gtk::Button,
    gains_text: adw::EntryRow,
    apo_export: gtk::Button,
    // Assistant EQ.
    guided_status: adw::ActionRow,
    guided_curve: adw::ActionRow,
    guided_start: gtk::Button,
    guided_less: gtk::Button,
    guided_reference: gtk::Button,
    guided_more: gtk::Button,
    guided_next: gtk::Button,
    guided_initial: gtk::Button,
    guided_result: gtk::Button,
    guided_restore: gtk::Button,
    guided_dismiss: gtk::Button,
    guided_name: adw::EntryRow,
    // Scènes.
    scenes: gtk::ListBox,
    scene_panel: Rc<ScenePanel>,
    scene_capture: adw::EntryRow,
    // Paramètres.
    smart_pause: adw::SwitchRow,
    auto_answer: adw::SwitchRow,
    comfort: adw::SwitchRow,
    pub sidetone: adw::ComboRow,
    auto_off: adw::ComboRow,
    protection: adw::SwitchRow,
    prompts: adw::ComboRow,
    prompt_volume: gtk::Scale,
    name: adw::EntryRow,
    info: Lines,
    // Batterie.
    battery_rows: Lines,
    battery_span: adw::ComboRow,
    battery_export: gtk::Button,
    battery_clear: gtk::Button,
    journal: Lines,
    journal_export: gtk::Button,
    journal_clear: gtk::Button,
    battery_chart: gtk::DrawingArea,
    battery_path: Rc<RefCell<String>>,
    // Connexion et audio.
    findings: Lines,
    control: Lines,
    system: Lines,
    stream: Lines,
    codec: adw::ComboRow,
    codec_card: RefCell<Option<CodecCard>>,
    pipewire: RefCell<Option<Result<PipeWireAudio, AudioReadError>>>,
    audio_tx: Sender<AudioRead>,
    audio_rx: Receiver<AudioRead>,
    // À propos.
    about: Lines,
    autostart: adw::SwitchRow,
    copy_diagnostic: gtk::Button,
    report: gtk::Button,
    language: adw::ComboRow,
    /// Ordre de `NotificationPrefs` : batterie faible, connexion, casque
    /// sans son, batterie pleine.
    notifications: [adw::SwitchRow; 4],
    settings_battery: adw::SwitchRow,
    settings_export: adw::ActionRow,
    settings_import: adw::ActionRow,
    settings_status: adw::ActionRow,
    settings_confirm: adw::ActionRow,
    settings_cancel: adw::ActionRow,
    /// Import examiné, en attente de confirmation.
    settings_plan: RefCell<Option<ImportPlan>>,
    incidents: RefCell<Vec<(u64, String)>>,
    snapshot: RefCell<Snapshot>,
    writable: Cell<bool>,
    /// Posés par `connect` : demandes vers le service, recopie en cours.
    send: RefCell<Option<Send>>,
    updating: RefCell<Rc<Cell<bool>>>,
}

fn group(title: &str) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder().title(title).build()
}

impl Extras {
    /// `home` : page Accueil déjà construite. `extras_home` : son conteneur.
    pub fn new(home: &gtk::ScrolledWindow, extras_home: &gtk::Box) -> Rc<Self> {
        let stack = adw::ViewStack::new();
        stack.add_titled_with_icon(home, Some("accueil"), tr("Accueil"), "go-home-symbolic");

        let head_tracking = adw::SwitchRow::builder().title(tr("Suivi de tête")).build();
        let touch = adw::SwitchRow::builder()
            .title(tr("Commandes tactiles"))
            .build();
        let multipoint = adw::SwitchRow::builder().title(tr("Multipoint")).build();
        let more = group(tr("Commandes"));
        more.add(&head_tracking);
        more.add(&touch);
        more.add(&multipoint);
        extras_home.append(&more);
        let devices = boxed_list();
        let devices_group = group(tr("Appareils appairés au casque"));
        devices_group.add(&devices);
        extras_home.append(&devices_group);

        // Mon son.
        let sound = page("son", tr("Mon son"), "audio-speakers-symbolic", &stack);
        let audio_mode = choices(tr("Mode audio"), &AUDIO_MODES);
        let mode_group = group(tr("Mode audio"));
        mode_group.add(&audio_mode);
        sound.append(&mode_group);
        let band_group = group(tr("Égaliseur"));
        let bass = adw::SwitchRow::builder()
            .title(tr("Basses renforcées"))
            .build();
        let flat = gtk::Button::with_label(tr("Remettre à plat"));
        flat.set_valign(gtk::Align::Center);
        band_group.set_header_suffix(Some(&flat));
        band_group.add(&bass);
        let gains_text = adw::EntryRow::builder()
            .title(tr("Gains en dB : -3,5 ; -1 ; 0 ; …"))
            .show_apply_button(true)
            .build();
        band_group.add(&gains_text);
        sound.append(&band_group);
        let preset = adw::ComboRow::builder().title(tr("Préréglage")).build();
        let preset_apply = gtk::Button::with_label(tr("Appliquer"));
        let preset_delete = gtk::Button::with_label(tr("Supprimer"));
        preset_apply.set_valign(gtk::Align::Center);
        preset_delete.set_valign(gtk::Align::Center);
        let preset_status = adw::ActionRow::builder()
            .title(tr("Courbe du casque"))
            .build();
        preset_status.add_suffix(&preset_delete);
        preset_status.add_suffix(&preset_apply);
        let preset_name = adw::EntryRow::builder()
            .title(tr("Enregistrer la courbe sous ce nom"))
            .show_apply_button(true)
            .build();
        let preset_group = group(tr("Préréglages"));
        preset_group.add(&preset);
        preset_group.add(&preset_status);
        preset_group.add(&preset_name);
        sound.append(&preset_group);
        let ab_remember = gtk::Button::with_label(tr("Mémoriser A"));
        let ab_toggle = gtk::Button::with_label(tr("Passer de A à B"));
        ab_remember.set_valign(gtk::Align::Center);
        ab_toggle.set_valign(gtk::Align::Center);
        let ab_row = adw::ActionRow::builder()
            .title(tr("Comparaison A/B"))
            .build();
        ab_row.add_suffix(&ab_remember);
        ab_row.add_suffix(&ab_toggle);
        let ab_group = group(tr("Comparaison"));
        ab_group.add(&ab_row);
        sound.append(&ab_group);
        let small = |label: &str| {
            let b = gtk::Button::with_label(label);
            b.set_valign(gtk::Align::Center);
            b
        };
        let curve_export = small(tr("Exporter"));
        let curve_import = small(tr("Importer…"));
        let curves = adw::ActionRow::builder()
            .title(tr("Courbe (fichier)"))
            .build();
        curves.add_suffix(&curve_import);
        curves.add_suffix(&curve_export);
        let profile_export = small(tr("Exporter"));
        let profile = adw::ActionRow::builder()
            .title(tr("Profil auditif (fichier privé)"))
            .subtitle(tr("À ne pas joindre à un rapport"))
            .build();
        profile.add_suffix(&profile_export);
        let bind_legacy = small(tr("Associer"));
        let legacy = adw::ActionRow::builder()
            .title(tr("Associer les anciens préréglages à cet égaliseur"))
            .build();
        legacy.add_suffix(&bind_legacy);
        let apo_export = small(tr("Exporter"));
        let apo = adw::ActionRow::builder()
            .title(tr("Exporter pour Equalizer APO"))
            .build();
        apo.add_suffix(&apo_export);
        let files = group(tr("Fichiers"));
        files.add(&curves);
        files.add(&apo);
        files.add(&profile);
        files.add(&legacy);
        sound.append(&files);

        let guided_status = adw::ActionRow::new();
        let guided_curve = adw::ActionRow::new();
        let guided_start = small(tr("Commencer les comparaisons"));
        let guided_less = small(tr("Moins (−1 dB)"));
        let guided_reference = small(tr("Référence de l'étape"));
        let guided_more = small(tr("Plus (+1 dB)"));
        let guided_next = small(tr("Garder ce choix et continuer"));
        let guided_initial = small(tr("Écouter la courbe initiale"));
        let guided_result = small(tr("Écouter le résultat"));
        let guided_restore = small(tr("Restaurer la courbe initiale"));
        let guided_dismiss = small(tr("Fermer"));
        let guided_name = adw::EntryRow::builder()
            .title(tr("Enregistrer le résultat sous ce nom"))
            .show_apply_button(true)
            .build();
        let guided_actions = gtk::FlowBox::new();
        guided_actions.set_selection_mode(gtk::SelectionMode::None);
        guided_actions.set_column_spacing(6);
        guided_actions.set_row_spacing(6);
        for b in [
            &guided_start,
            &guided_less,
            &guided_reference,
            &guided_more,
            &guided_next,
            &guided_initial,
            &guided_result,
            &guided_restore,
            &guided_dismiss,
        ] {
            guided_actions.insert(b, -1);
        }
        let guided_group = group(tr("Assistant EQ guidé"));
        guided_group.add(&guided_status);
        guided_group.add(&guided_curve);
        guided_group.add(&guided_name);
        sound.append(&guided_group);
        sound.append(&guided_actions);

        // Scènes.
        let scene_page = page("scenes", tr("Scènes"), "view-list-symbolic", &stack);
        let scenes = boxed_list();
        let scenes_group = group(tr("Scènes"));
        scenes_group.add(&scenes);
        scene_page.append(&scenes_group);
        let scene_capture = adw::EntryRow::builder()
            .title(tr("Enregistrer l'état actuel comme scène"))
            .show_apply_button(true)
            .build();
        let capture_group = group(tr("Enregistrer l'état actuel"));
        capture_group.add(&scene_capture);
        scene_page.append(&capture_group);
        let scene_panel = ScenePanel::new(&scene_page);

        // Paramètres.
        let settings = page(
            "parametres",
            tr("Paramètres"),
            "emblem-system-symbolic",
            &stack,
        );
        let smart_pause = adw::SwitchRow::builder().title(tr("Smart Pause")).build();
        let auto_answer = adw::SwitchRow::builder()
            .title(tr("Réponse automatique"))
            .build();
        let comfort = adw::SwitchRow::builder()
            .title(tr("Appels confortables"))
            .build();
        let sidetone = choices(tr("Retour de voix (sidetone)"), &SIDETONE_LABELS);
        let calls = group(tr("Appels et port"));
        calls.add(&smart_pause);
        calls.add(&auto_answer);
        calls.add(&comfort);
        calls.add(&sidetone);
        settings.append(&calls);
        let auto_off = choices(tr("Extinction automatique"), &AUTO_OFF_LABELS);
        let protection = adw::SwitchRow::builder()
            .title(tr("Protection de la batterie"))
            .build();
        let power = group(tr("Alimentation"));
        power.add(&auto_off);
        power.add(&protection);
        settings.append(&power);
        let prompts = choices(tr("Invites"), &PROMPT_MODES);
        let prompt_volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        prompt_volume.set_hexpand(true);
        prompt_volume.set_width_request(200);
        prompt_volume.set_draw_value(true);
        let volume_row = adw::ActionRow::builder()
            .title(tr("Volume des invites"))
            .build();
        volume_row.add_suffix(&prompt_volume);
        let voice = group(tr("Invites"));
        voice.add(&prompts);
        voice.add(&volume_row);
        settings.append(&voice);
        let name = adw::EntryRow::builder()
            .title(tr("Nom du casque"))
            .show_apply_button(true)
            .build();
        let name_group = group(tr("Nom du casque"));
        name_group.add(&name);
        settings.append(&name_group);
        let info = Lines::new();
        settings.append(&info.list);

        // Batterie.
        let battery_page = page("batterie", tr("Batterie"), "battery-good-symbolic", &stack);
        let spans: Vec<&'static str> = viewmodel::BATTERY_SPANS.iter().map(|(l, _)| *l).collect();
        let battery_span = choices(tr("Période"), &spans);
        battery_span.set_selected(0);
        let export = gtk::Button::with_label(tr("Exporter (CSV)"));
        export.set_valign(gtk::Align::Center);
        let battery_clear = gtk::Button::with_label(tr("Effacer l'historique…"));
        battery_clear.set_valign(gtk::Align::Center);
        let battery_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        battery_buttons.append(&battery_clear);
        battery_buttons.append(&export);
        let span_group = group(tr("Batterie"));
        span_group.set_header_suffix(Some(&battery_buttons));
        span_group.add(&battery_span);
        battery_page.append(&span_group);
        let battery_path = Rc::new(RefCell::new(String::new()));
        let battery_chart = gtk::DrawingArea::new();
        battery_chart.set_content_height(160);
        battery_chart.set_hexpand(true);
        battery_chart.add_css_class("card");
        {
            let path = battery_path.clone();
            battery_chart.set_draw_func(move |_, context, width, height| {
                draw_chart(context, &path.borrow(), f64::from(width), f64::from(height));
            });
        }
        battery_page.append(&battery_chart);
        let battery_rows = Lines::new();
        battery_page.append(&battery_rows.list);
        let journal = Lines::new();
        let journal_group = group(tr("Journal des événements"));
        journal_group.add(&journal.list);
        battery_page.append(&journal_group);
        let journal_export = gtk::Button::with_label(tr("Exporter le journal (CSV)"));
        let journal_clear = gtk::Button::with_label(tr("Effacer le journal…"));
        let journal_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        journal_buttons.set_halign(gtk::Align::End);
        journal_buttons.append(&journal_clear);
        journal_buttons.append(&journal_export);
        battery_page.append(&journal_buttons);

        // Connexion et audio.
        let audio = page(
            "audio",
            tr("Connexion et audio"),
            "network-bluetooth-symbolic",
            &stack,
        );
        let findings = Lines::new();
        let control = Lines::new();
        let system = Lines::new();
        let stream = Lines::new();
        audio.append(&findings.list);
        let codec = adw::ComboRow::builder()
            .title(tr("Codec Bluetooth (PipeWire)"))
            .subtitle(tr("Réglage du PC : le son peut s'interrompre un instant"))
            .build();
        let codec_group = group(tr("Flux Bluetooth"));
        codec_group.add(&codec);
        audio.append(&codec_group);
        audio.append(&stream.list);
        audio.append(&control.list);
        audio.append(&system.list);

        // À propos.
        let about_page = page("apropos", tr("À propos"), "help-about-symbolic", &stack);
        let about = Lines::new();
        about_page.append(&about.list);
        let autostart_row = adw::SwitchRow::builder()
            .title(tr("Lancement à l'ouverture de session"))
            .active(autostart::is_enabled())
            .build();
        let copy_diagnostic = gtk::Button::with_label(tr("Copier le diagnostic"));
        let report = gtk::Button::with_label(tr("Signaler un bogue…"));
        copy_diagnostic.set_valign(gtk::Align::Center);
        report.set_valign(gtk::Align::Center);
        let diagnostic_row = adw::ActionRow::builder()
            .title(tr("Diagnostic"))
            .subtitle(tr("Sans adresse, numéro de série ni nom d'appareil"))
            .build();
        diagnostic_row.add_suffix(&report);
        diagnostic_row.add_suffix(&copy_diagnostic);
        // Chaque langue dans sa propre langue.
        let language = adw::ComboRow::builder().title(tr("Langue")).build();
        language.set_model(Some(&gtk::StringList::new(&[
            tr("Langue du système"),
            "Français",
            "English",
        ])));
        language.set_selected(match load_pref() {
            None => 0,
            Some(Lang::Fr) => 1,
            Some(Lang::En) => 2,
        });
        let about_group = group(tr("Application"));
        about_group.add(&autostart_row);
        about_group.add(&language);
        about_group.add(&diagnostic_row);
        about_page.append(&about_group);

        let prefs = NotificationPrefs::load();
        let notifications = [
            (tr("Batterie faible"), prefs.low_battery),
            (tr("Connexion perdue et retrouvée"), prefs.connection),
            (tr("Casque allumé sans son"), prefs.idle),
            (tr("Batterie à 100 % câble branché"), prefs.full),
        ]
        .map(|(title, on)| adw::SwitchRow::builder().title(title).active(on).build());
        let notifications_group = group(tr("Notifications"));
        for row in &notifications {
            notifications_group.add(row);
        }
        about_page.append(&notifications_group);

        let backup = adw::PreferencesGroup::builder()
            .title(tr("Sauvegarder et retrouver mes réglages"))
            .description(tr(
                "Scènes, préréglages, raccourcis, panneau et apparence. Aucun profil auditif. Relancer l'interface après import ; un brouillon non enregistré n'est pas inclus.",
            ))
            .build();
        let settings_battery = adw::SwitchRow::builder()
            .title(tr(
                "Inclure aussi l'historique de batterie (export et import)",
            ))
            .build();
        // Ligne qui se presse comme un bouton.
        let action = |title: &str| {
            let row = adw::ActionRow::builder()
                .title(title)
                .activatable(true)
                .build();
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            row
        };
        let settings_export = action(tr("Exporter les réglages…"));
        let settings_import = action(tr("Choisir un fichier à importer…"));
        let settings_status = adw::ActionRow::builder().visible(false).build();
        let settings_confirm = action(tr("Confirmer cet import"));
        let settings_cancel = action(tr("Annuler l'import"));
        settings_confirm.set_visible(false);
        settings_cancel.set_visible(false);
        backup.add(&settings_battery);
        backup.add(&settings_export);
        backup.add(&settings_import);
        backup.add(&settings_status);
        backup.add(&settings_confirm);
        backup.add(&settings_cancel);
        about_page.append(&backup);

        let (audio_tx, audio_rx) = mpsc::channel();
        Rc::new(Self {
            stack,
            head_tracking,
            touch,
            multipoint,
            devices,
            devices_last: RefCell::new(Vec::new()),
            audio_mode,
            band_group,
            bands: RefCell::new(Vec::new()),
            bass,
            flat,
            preset,
            preset_names: RefCell::new(Vec::new()),
            preset_apply,
            preset_delete,
            preset_name,
            preset_status,
            user_presets: RefCell::new(paths::load_user_presets()),
            ab: RefCell::new(AbCompare::default()),
            ab_remember,
            ab_toggle,
            curve_export,
            curve_import,
            profile_export,
            bind_legacy,
            gains_text,
            apo_export,
            guided_status,
            guided_curve,
            guided_start,
            guided_less,
            guided_reference,
            guided_more,
            guided_next,
            guided_initial,
            guided_result,
            guided_restore,
            guided_dismiss,
            guided_name,
            scenes,
            scene_panel,
            scene_capture,
            smart_pause,
            auto_answer,
            comfort,
            sidetone,
            auto_off,
            protection,
            prompts,
            prompt_volume,
            name,
            info,
            battery_rows,
            battery_span,
            battery_export: export,
            battery_clear,
            journal,
            journal_export,
            journal_clear,
            battery_chart,
            battery_path,
            findings,
            control,
            system,
            stream,
            codec,
            codec_card: RefCell::new(None),
            pipewire: RefCell::new(None),
            audio_tx,
            audio_rx,
            about,
            autostart: autostart_row,
            copy_diagnostic,
            report,
            language,
            notifications,
            settings_battery,
            settings_export,
            settings_import,
            settings_status,
            settings_confirm,
            settings_cancel,
            settings_plan: RefCell::new(None),
            incidents: RefCell::new(Vec::new()),
            snapshot: RefCell::new(Snapshot::default()),
            writable: Cell::new(false),
            send: RefCell::new(None),
            updating: RefCell::new(Rc::new(Cell::new(false))),
        })
    }

    fn presets(&self, snapshot: &Snapshot) -> Vec<Preset> {
        let bands = snapshot.equalizer.as_ref().map_or(0, |e| e.gains.len());
        let mut all = presets::builtin_presets(bands);
        all.extend(
            self.user_presets
                .borrow()
                .iter()
                .filter(|p| p.gains.len() == bands)
                .cloned(),
        );
        all
    }

    /// Relie les contrôles aux demandes. `updating` : l'état est en cours de
    /// recopie dans les contrôles, leurs signaux n'envoient alors rien.
    pub fn connect(self: &Rc<Self>, send: &Send, notify: &Notify, updating: &Rc<Cell<bool>>) {
        *self.send.borrow_mut() = Some(send.clone());
        *self.updating.borrow_mut() = updating.clone();
        let switch = |row: &adw::SwitchRow, make: fn(bool) -> Command| {
            let send = send.clone();
            let updating = updating.clone();
            row.connect_active_notify(move |row| {
                if !updating.get() {
                    send(make(row.is_active()));
                }
            });
        };
        switch(&self.head_tracking, Command::SetHeadTracking);
        switch(&self.touch, Command::SetTouchControls);
        switch(&self.multipoint, Command::SetMultipoint);
        switch(&self.bass, Command::SetBassBoost);
        switch(&self.smart_pause, Command::SetSmartPause);
        switch(&self.auto_answer, Command::SetAutoAnswer);
        switch(&self.comfort, Command::SetComfortCall);
        switch(&self.protection, Command::SetBatteryProtection);

        let combo = |row: &adw::ComboRow, make: Rc<dyn Fn(usize) -> Option<Command>>| {
            let send = send.clone();
            let updating = updating.clone();
            row.connect_selected_notify(move |row| {
                if updating.get() {
                    return;
                }
                if let Some(command) = usize::try_from(row.selected()).ok().and_then(|i| make(i)) {
                    send(command);
                }
            });
        };
        combo(
            &self.audio_mode,
            Rc::new(|i| {
                Some(match i {
                    1 => Command::SetSpeechClarity(true),
                    2 => Command::SetSoundProfile(true),
                    _ => Command::SetSpeechClarity(false),
                })
            }),
        );
        combo(
            &self.sidetone,
            Rc::new(|i| SIDETONE_LEVELS.get(i).map(|l| Command::SetSidetone(*l))),
        );
        combo(
            &self.auto_off,
            Rc::new(|i| AUTO_OFF_CHOICES.get(i).map(|s| Command::SetAutoOff(*s))),
        );
        combo(
            &self.prompts,
            Rc::new(|i| {
                Some(Command::SetPromptMode(if i == 0 {
                    PromptMode::TonesOnly
                } else {
                    PromptMode::TonesAndVoice
                }))
            }),
        );
        {
            let send = send.clone();
            self.flat.connect_clicked(move |_| send(Command::SetEqFlat));
        }
        {
            let send = send.clone();
            let updating = updating.clone();
            self.prompt_volume.connect_value_changed(move |scale| {
                if updating.get() {
                    return;
                }
                let value = scale.value().round();
                let scale = scale.clone();
                let send = send.clone();
                glib::timeout_add_local_once(crate::app::SETTLE, move || {
                    if scale.value().round() == value {
                        send(Command::SetPromptVolume(
                            momentum_core::device::prompt_volume_from_percent(value as u8),
                        ));
                    }
                });
            });
        }
        {
            let send = send.clone();
            let notify = notify.clone();
            self.name.connect_apply(move |row| {
                let wanted = row.text().trim().to_string();
                match check_name(&wanted) {
                    Ok(_) => send(Command::SetName(wanted)),
                    Err(e) => notify(e.to_string()),
                }
            });
        }
        self.connect_presets(send, notify);
        self.connect_files(send, notify);
        self.connect_scene_capture(notify);
        self.connect_about(notify, updating);
        self.connect_application(notify);
        self.connect_battery(send, notify);
        self.connect_codec(notify, updating);
        // Scènes et lectures locales : au moment où la page s'affiche.
        {
            let this = self.clone();
            let send = send.clone();
            self.stack.connect_visible_child_name_notify(move |stack| {
                match stack.visible_child_name().as_deref() {
                    Some("scenes") => this.reload_scenes(&send),
                    Some("batterie") => this.reload_battery(),
                    Some("audio") => this.reload_audio(),
                    _ => {}
                }
            });
        }
    }

    /// Assistant EQ : les boutons repartent vers la fenêtre, qui le tient.
    pub fn connect_guided(self: &Rc<Self>, guided: &Guided, save: &GuidedSave, notify: &Notify) {
        for (button, action) in [
            (&self.guided_start, GuidedAction::Start),
            (&self.guided_less, GuidedAction::Less),
            (&self.guided_reference, GuidedAction::Reference),
            (&self.guided_more, GuidedAction::More),
            (&self.guided_next, GuidedAction::Next),
            (&self.guided_initial, GuidedAction::Initial),
            (&self.guided_result, GuidedAction::Result),
            (&self.guided_restore, GuidedAction::Restore),
            (&self.guided_dismiss, GuidedAction::Dismiss),
        ] {
            let guided = guided.clone();
            button.connect_clicked(move |_| guided(action));
        }
        let this = self.clone();
        let save = save.clone();
        let notify = notify.clone();
        self.guided_name.connect_apply(move |row| {
            let name = row.text().trim().to_string();
            if let Some(list) = save(&name) {
                *this.user_presets.borrow_mut() = list;
                row.set_text("");
                notify(trf("Préréglage « {} » enregistré sur ce PC.", &[&name]));
                let snapshot = this.snapshot.borrow().clone();
                this.refresh_presets(&snapshot);
            }
        });
    }

    /// Essai de l'interface : démarre l'assistant comme par un clic.
    pub fn press_guided_start(&self) {
        self.guided_start.emit_clicked();
    }

    /// Essai de l'interface : restaure la courbe initiale comme par un clic.
    pub fn press_guided_restore(&self) {
        self.guided_restore.emit_clicked();
    }

    /// Essai de l'interface : saisit des gains en texte et valide.
    pub fn enter_gains(&self, text: &str) {
        self.gains_text.set_text(text);
        self.gains_text.emit_by_name::<()>("apply", &[]);
    }

    /// Recopie l'assistant EQ dans ses contrôles.
    pub fn refresh_guided(&self, v: &GuidedView) {
        self.guided_status
            .set_title(&glib::markup_escape_text(&v.status));
        self.guided_curve
            .set_title(&glib::markup_escape_text(&v.initial));
        self.guided_curve
            .set_subtitle(&glib::markup_escape_text(&trf(
                "Courbe relue (dB) : {}",
                &[&v.current],
            )));
        self.guided_curve.set_visible(v.active);
        self.guided_start.set_visible(!v.active);
        self.guided_start.set_sensitive(v.can_start);
        for b in [&self.guided_less, &self.guided_reference, &self.guided_more] {
            b.set_visible(v.active && !v.review);
            b.set_sensitive(v.can_listen);
        }
        self.guided_next.set_visible(v.active && !v.review);
        self.guided_next.set_sensitive(v.can_next);
        for b in [&self.guided_initial, &self.guided_result] {
            b.set_visible(v.active && v.review);
            b.set_sensitive(v.can_listen);
        }
        self.guided_restore.set_visible(v.active);
        self.guided_restore.set_sensitive(v.can_restore);
        self.guided_dismiss.set_visible(v.can_dismiss);
        self.guided_name.set_visible(v.active && v.review);
        self.guided_name.set_sensitive(v.can_save);
    }

    fn connect_files(self: &Rc<Self>, send: &Send, notify: &Notify) {
        {
            let this = self.clone();
            let notify = notify.clone();
            self.curve_export.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow();
                let Some(eq) = snapshot.equalizer.as_ref() else {
                    return;
                };
                let model = snapshot.info.as_ref().and_then(|i| i.model.as_deref());
                let text = presets::format_curve(model, &eq.gains, eq.bass_boost);
                notify(
                    match paths::export_file("momentum-egaliseur", "txt", unix_now()) {
                        Some(path) => match std::fs::write(&path, text) {
                            Ok(()) => trf("Courbe exportée dans {}.", &[&path.display()]),
                            Err(e) => trf("Export de la courbe impossible : {}.", &[&e]),
                        },
                        None => {
                            tr("Export impossible : dossier personnel introuvable.").to_string()
                        }
                    },
                );
            });
        }
        {
            let this = self.clone();
            let send = send.clone();
            let notify = notify.clone();
            self.curve_import.connect_clicked(move |button| {
                let this = this.clone();
                let send = send.clone();
                let notify = notify.clone();
                let window = button.root().and_downcast::<gtk::Window>();
                gtk::FileDialog::new().open(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        let Some(path) = result.ok().and_then(|f| f.path()) else {
                            return;
                        };
                        let snapshot = this.snapshot.borrow().clone();
                        let Some(eq) = snapshot.equalizer else {
                            return;
                        };
                        let model = snapshot.info.as_ref().and_then(|i| i.model.clone());
                        let parsed = std::fs::read_to_string(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|text| {
                                presets::parse_curve(
                                    &text,
                                    model.as_deref(),
                                    eq.gains.len(),
                                    eq.min_gain,
                                    eq.max_gain,
                                )
                            });
                        notify(match parsed {
                            Ok(curve) => {
                                if curve.gains != eq.gains {
                                    send(Command::SetEqGains(curve.gains));
                                }
                                if let Some(on) = curve.bass_boost.filter(|&on| on != eq.bass_boost)
                                {
                                    send(Command::SetBassBoost(on));
                                }
                                trf("Courbe importée depuis {}.", &[&path.display()])
                            }
                            Err(reason) => trf("Courbe non importée : {}.", &[&reason]),
                        });
                    },
                );
            });
        }
        {
            let this = self.clone();
            let send = send.clone();
            let notify = notify.clone();
            self.gains_text.connect_apply(move |row| {
                let Some(eq) = this.snapshot.borrow().equalizer.clone() else {
                    return;
                };
                let parsed =
                    presets::parse_gains_db(&row.text(), eq.gains.len(), eq.min_gain, eq.max_gain);
                match parsed {
                    Ok(gains) if gains == eq.gains => {
                        notify(tr("Ce sont déjà les gains du casque.").to_string());
                    }
                    Ok(gains) => {
                        send(Command::SetEqGains(gains));
                        row.set_text("");
                    }
                    Err(reason) => notify(trf("Gains non appliqués : {}.", &[&reason])),
                }
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.apo_export.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow();
                let Some(eq) = snapshot.equalizer.as_ref() else {
                    return;
                };
                notify(match presets::format_graphic_eq(&eq.gains) {
                    Err(reason) => reason,
                    Ok(text) => {
                        match paths::export_file("momentum-egaliseur-apo", "txt", unix_now()) {
                            Some(path) => match std::fs::write(&path, text) {
                                Ok(()) => trf(
                                    "Courbe exportée pour Equalizer APO dans {}.",
                                    &[&path.display()],
                                ),
                                Err(e) => trf("Export de la courbe impossible : {}.", &[&e]),
                            },
                            None => {
                                tr("Export impossible : dossier personnel introuvable.").to_string()
                            }
                        }
                    }
                });
            });
        }
        {
            let send = send.clone();
            let notify = notify.clone();
            self.profile_export.connect_clicked(move |_| {
                match paths::export_file("momentum-profil", "json", unix_now()) {
                    Some(path) => send(Command::ExportProfile(path)),
                    None => {
                        notify(tr("Export impossible : dossier personnel introuvable.").to_string())
                    }
                }
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.bind_legacy.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow().clone();
                let Some(topology) = snapshot.eq_topology() else {
                    return;
                };
                match paths::bind_legacy_presets(&topology) {
                    Ok(list) => {
                        *this.user_presets.borrow_mut() = list;
                        this.refresh_presets(&snapshot);
                    }
                    Err(reason) => notify(reason),
                }
            });
        }
    }

    fn connect_scene_capture(self: &Rc<Self>, notify: &Notify) {
        let this = self.clone();
        let notify = notify.clone();
        self.scene_capture.connect_apply(move |row| {
            let snapshot = this.snapshot.borrow().clone();
            let name = row.text().to_string();
            let result = snapshot
                .state
                .as_ref()
                .ok_or_else(|| tr("Aucun état connu à enregistrer.").to_string())
                .and_then(|state| {
                    momentum_core::scene_capture::save_current(
                        &name,
                        state,
                        snapshot.equalizer.as_ref(),
                        false,
                    )
                    .map_err(|e| e.to_string())
                });
            match result {
                Ok(scene) => {
                    row.set_text("");
                    *this.user_presets.borrow_mut() = paths::load_user_presets();
                    if !this.scene_panel.dirty() {
                        this.scene_panel.reload(None);
                    }
                    notify(trf(
                        "Scène « {} » enregistrée (rien n'a été envoyé au casque).",
                        &[&scene.name],
                    ));
                }
                Err(reason) => notify(reason),
            }
        });
    }

    fn diagnostic_text(&self) -> String {
        let s = self.snapshot.borrow();
        let now = unix_now();
        let incidents: Vec<(u64, String)> = self
            .incidents
            .borrow()
            .iter()
            .map(|(time, message)| (now.saturating_sub(*time), message.clone()))
            .collect();
        let home = std::env::var("HOME").unwrap_or_default();
        let user = std::env::var("USER").unwrap_or_default();
        let system = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|text| diagnostic::os_release_name(&text));
        let battery_entries = paths::battery_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or(0, |text| BatteryLog::parse(&text, now).entries().len());
        let mut secrets: Vec<&str> = vec![home.as_str(), user.as_str()];
        secrets.extend(s.device_name.as_deref());
        secrets.extend(s.info.as_ref().and_then(|i| i.name.as_deref()));
        secrets.extend(s.connections.iter().map(|d| d.name.as_str()));
        diagnostic::build(&DiagnosticInput {
            app_version: env!("CARGO_PKG_VERSION"),
            os_version: system.as_deref(),
            connected: s.state.is_some(),
            read_only: s.read_only.as_deref(),
            info: s.info.as_ref(),
            state: s.state.as_ref(),
            eq_bands: s.equalizer.as_ref().map(|e| e.gains.len()),
            paired_devices: s.state.is_some().then_some(s.connections.len()),
            battery_entries,
            incidents: &incidents,
            secrets: &secrets,
        })
    }

    fn connect_about(self: &Rc<Self>, notify: &Notify, updating: &Rc<Cell<bool>>) {
        {
            let notify = notify.clone();
            let updating = updating.clone();
            self.autostart.connect_active_notify(move |row| {
                if updating.get() {
                    return;
                }
                if let Err(e) = autostart::set(row.is_active()) {
                    notify(e.to_string());
                }
                row.set_active(autostart::is_enabled());
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.copy_diagnostic.connect_clicked(move |button| {
                button.clipboard().set_text(&this.diagnostic_text());
                notify(tr("Diagnostic copié.").to_string());
            });
        }
        {
            let this = self.clone();
            self.report.connect_clicked(move |button| {
                let system = std::fs::read_to_string("/etc/os-release")
                    .ok()
                    .and_then(|text| diagnostic::os_release_name(&text));
                let url = diagnostic::issue_url(
                    env!("CARGO_PKG_VERSION"),
                    diagnostic::ISSUE_INTERFACE_GNOME,
                    system.as_deref(),
                    &this.diagnostic_text(),
                );
                let window = button.root().and_downcast::<gtk::Window>();
                gtk::UriLauncher::new(&url).launch(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    |_| {},
                );
            });
        }
    }

    /// Langue, notifications au choix, sauvegarde et restauration.
    fn connect_application(self: &Rc<Self>, notify: &Notify) {
        {
            let notify = notify.clone();
            self.language.connect_selected_notify(move |row| {
                let lang = match row.selected() {
                    1 => Some(Lang::Fr),
                    2 => Some(Lang::En),
                    _ => None,
                };
                notify(match store_pref(lang) {
                    Ok(()) => {
                        tr("Langue enregistrée : appliquée au prochain lancement de l'interface.")
                            .to_string()
                    }
                    Err(e) => trf("Langue non enregistrée : {}.", &[&e]),
                });
            });
        }
        for (kind, row) in self.notifications.iter().enumerate() {
            let notify = notify.clone();
            row.connect_active_notify(move |row| {
                let mut prefs = NotificationPrefs::load();
                let on = row.is_active();
                match kind {
                    0 => prefs.low_battery = on,
                    1 => prefs.connection = on,
                    2 => prefs.idle = on,
                    _ => prefs.full = on,
                }
                if let Err(e) = prefs.store() {
                    tracing::warn!(%e, "notifications.txt");
                    notify(e.to_string());
                }
            });
        }
        {
            // L'aperçu dépend de ce choix : un import en attente est annulé.
            let this = self.clone();
            self.settings_battery.connect_active_notify(move |_| {
                if this.import_ready() {
                    this.cancel_settings();
                }
            });
        }
        {
            let this = self.clone();
            self.settings_export.connect_activated(move |row| {
                let this = this.clone();
                let window = row.root().and_downcast::<gtk::Window>();
                gtk::FileDialog::builder()
                    .title(tr("Exporter vers un nouveau fichier"))
                    .initial_name("openheadset-reglages.json")
                    .build()
                    .save(
                        window.as_ref(),
                        gtk::gio::Cancellable::NONE,
                        move |result| {
                            if let Some(path) = result.ok().and_then(|f| f.path()) {
                                this.export_settings(&path);
                            }
                        },
                    );
            });
        }
        {
            let this = self.clone();
            self.settings_import.connect_activated(move |row| {
                let this = this.clone();
                let window = row.root().and_downcast::<gtk::Window>();
                gtk::FileDialog::builder()
                    .title(tr("Choisir les réglages à examiner"))
                    .build()
                    .open(
                        window.as_ref(),
                        gtk::gio::Cancellable::NONE,
                        move |result| {
                            if let Some(path) = result.ok().and_then(|f| f.path()) {
                                this.preview_settings(&path);
                            }
                        },
                    );
            });
        }
        {
            let this = self.clone();
            self.settings_confirm
                .connect_activated(move |_| this.confirm_settings());
        }
        {
            let this = self.clone();
            self.settings_cancel
                .connect_activated(move |_| this.cancel_settings());
        }
    }

    /// Message de la sauvegarde ; les lignes de confirmation suivent
    /// l'import en attente.
    fn settings_notice(&self, text: &str) {
        self.settings_status
            .set_title(&glib::markup_escape_text(text));
        self.settings_status.set_visible(!text.is_empty());
        let ready = self.import_ready();
        self.settings_confirm.set_visible(ready);
        self.settings_cancel.set_visible(ready);
    }

    /// Un import examiné attend sa confirmation.
    pub fn import_ready(&self) -> bool {
        self.settings_plan.borrow().is_some()
    }

    /// Écrit la sauvegarde dans un fichier nouveau.
    pub fn export_settings(&self, path: &std::path::Path) {
        *self.settings_plan.borrow_mut() = None;
        let battery = self.settings_battery.is_active();
        let result = paths::data_dir()
            .ok_or_else(|| tr("Dossier des données introuvable.").to_string())
            .and_then(|dir| {
                settings_transfer::export(&dir, path, battery).map_err(|e| e.to_string())
            });
        self.settings_notice(&result.unwrap_or_else(|e| trf("Export refusé : {}", &[&e])));
    }

    /// Examine une sauvegarde sans rien modifier ; le résumé s'affiche.
    pub fn preview_settings(&self, path: &std::path::Path) {
        *self.settings_plan.borrow_mut() = None;
        let battery = self.settings_battery.is_active();
        let result = paths::data_dir()
            .ok_or_else(|| tr("Dossier des données introuvable.").to_string())
            .and_then(|dir| {
                settings_transfer::prepare_import(&dir, path, battery).map_err(|e| e.to_string())
            });
        let message = match result {
            Ok(plan) => {
                let summary = plan.summary().to_string();
                *self.settings_plan.borrow_mut() = Some(plan);
                summary
            }
            Err(e) => trf("Import refusé : {}", &[&e]),
        };
        self.settings_notice(&message);
    }

    fn confirm_settings(&self) {
        let Some(plan) = self.settings_plan.borrow_mut().take() else {
            return;
        };
        let message = plan
            .apply()
            .unwrap_or_else(|e| trf("Import refusé : {}", &[&e]));
        self.settings_notice(&message);
    }

    pub fn cancel_settings(&self) {
        *self.settings_plan.borrow_mut() = None;
        self.settings_notice(tr("Import annulé."));
    }

    /// Essai de l'interface : bascule l'avis « casque allumé sans son »
    /// comme par un clic ; renvoie le nouvel état de la ligne.
    pub fn toggle_idle_notification(&self) -> bool {
        let row = &self.notifications[2];
        row.set_active(!row.is_active());
        row.is_active()
    }

    fn connect_presets(self: &Rc<Self>, send: &Send, notify: &Notify) {
        {
            let this = self.clone();
            let send = send.clone();
            let notify = notify.clone();
            self.preset_apply.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow().clone();
                let chosen = usize::try_from(this.preset.selected())
                    .ok()
                    .and_then(|i| this.presets(&snapshot).get(i).cloned());
                let Some(preset) = chosen else {
                    return;
                };
                match snapshot
                    .eq_topology()
                    .ok_or(tr("Topologie de l'égaliseur non disponible."))
                    .and_then(|t| preset.check_topology(&t))
                {
                    Ok(()) => send(Command::SetEqGains(preset.gains)),
                    Err(reason) => notify(reason.to_string()),
                }
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.preset_delete.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow().clone();
                let chosen = usize::try_from(this.preset.selected())
                    .ok()
                    .and_then(|i| this.presets(&snapshot).get(i).cloned())
                    .filter(|p| !p.builtin);
                let Some(preset) = chosen else {
                    return;
                };
                match paths::remove_user_preset(&preset.name) {
                    Ok(list) => {
                        *this.user_presets.borrow_mut() = list;
                        this.refresh_presets(&snapshot);
                    }
                    Err(reason) => notify(reason),
                }
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.preset_name.connect_apply(move |row| {
                let snapshot = this.snapshot.borrow().clone();
                let name = row.text().trim().to_string();
                let (Some(eq), Some(topology)) =
                    (snapshot.equalizer.as_ref(), snapshot.eq_topology())
                else {
                    return;
                };
                match paths::upsert_user_preset_for(&name, &eq.gains, &topology) {
                    Ok(list) => {
                        *this.user_presets.borrow_mut() = list;
                        row.set_text("");
                        this.refresh_presets(&snapshot);
                        notify(trf("Préréglage « {} » enregistré sur ce PC.", &[&name]));
                    }
                    Err(reason) => notify(trf("Préréglage non enregistré : {}.", &[&reason])),
                }
            });
        }
        {
            let this = self.clone();
            let notify = notify.clone();
            self.ab_remember.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow().clone();
                let (Some(eq), Some(topology)) =
                    (snapshot.equalizer.as_ref(), snapshot.eq_topology())
                else {
                    return;
                };
                if let Err(reason) = this.ab.borrow_mut().remember_for(&topology, &eq.gains) {
                    notify(reason.to_string());
                }
                this.refresh_ab();
            });
        }
        {
            let this = self.clone();
            let send = send.clone();
            let notify = notify.clone();
            self.ab_toggle.connect_clicked(move |_| {
                let snapshot = this.snapshot.borrow().clone();
                let (Some(eq), Some(topology)) =
                    (snapshot.equalizer.as_ref(), snapshot.eq_topology())
                else {
                    return;
                };
                let toggled = this.ab.borrow_mut().toggle_for(&topology, &eq.gains);
                match toggled {
                    Ok(Some(gains)) => send(Command::SetEqGains(gains)),
                    Ok(None) => {}
                    Err(reason) => notify(reason.to_string()),
                }
                this.refresh_ab();
            });
        }
    }

    fn connect_battery(self: &Rc<Self>, send: &Send, notify: &Notify) {
        {
            let this = self.clone();
            self.battery_span
                .connect_selected_notify(move |_| this.reload_battery());
        }
        // Effacements : confirmés, puis faits par le service, qui réécrit le
        // fichier ; relecture un peu plus tard.
        for (button, heading, body, command, done) in [
            (
                &self.battery_clear,
                tr("Effacer l'historique de la batterie ?"),
                tr(
                    "Toutes les mesures enregistrées sur ce PC sont supprimées. Exporter d'abord pour les garder.",
                ),
                Command::ClearBatteryHistory,
                tr("Effacement de l'historique demandé au service."),
            ),
            (
                &self.journal_clear,
                tr("Effacer le journal des événements ?"),
                tr(
                    "Tous les événements notés sur ce PC sont supprimés. Exporter d'abord pour les garder.",
                ),
                Command::ClearJournal,
                tr("Effacement du journal demandé au service."),
            ),
        ] {
            let this = self.clone();
            let send = send.clone();
            let notify = notify.clone();
            button.connect_clicked(move |button| {
                let dialog = adw::AlertDialog::new(Some(heading), Some(body));
                dialog.add_responses(&[("annuler", tr("Annuler")), ("effacer", tr("Effacer"))]);
                dialog.set_response_appearance("effacer", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("annuler"));
                dialog.set_close_response("annuler");
                let this = this.clone();
                let send = send.clone();
                let notify = notify.clone();
                let command = command.clone();
                dialog.connect_response(Some("effacer"), move |_, _| {
                    send(command.clone());
                    notify(done.to_string());
                    let this = this.clone();
                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(600),
                        move || {
                            this.reload_battery();
                        },
                    );
                });
                dialog.present(Some(button));
            });
        }
        {
            let notify = notify.clone();
            self.journal_export
                .connect_clicked(move |_| notify(momentum_core::journal::export(unix_now())));
        }
        let notify = notify.clone();
        self.battery_export.connect_clicked(move |_| {
            let message = match (
                paths::battery_file(),
                paths::export_file("momentum-batterie", "csv", unix_now()),
            ) {
                (Some(source), Some(target)) => match std::fs::copy(&source, &target) {
                    Ok(_) => trf("Historique exporté dans {}.", &[&target.display()]),
                    Err(e) => trf("Export de l'historique impossible : {}.", &[&e]),
                },
                _ => tr("Export impossible : dossier Documents introuvable.").to_string(),
            };
            notify(message);
        });
    }

    fn connect_codec(self: &Rc<Self>, notify: &Notify, updating: &Rc<Cell<bool>>) {
        let this = self.clone();
        let notify = notify.clone();
        let updating = updating.clone();
        self.codec.connect_selected_notify(move |row| {
            if updating.get() {
                return;
            }
            let wanted = usize::try_from(row.selected()).ok().and_then(|i| {
                this.codec_card
                    .borrow()
                    .as_ref()
                    .and_then(|c| c.choices.get(i).cloned())
            });
            let Some(wanted) = wanted.filter(|c| !c.active) else {
                return;
            };
            let Some(id) = this.snapshot.borrow().headset_id.clone() else {
                notify(tr("Casque non connecté").to_string());
                return;
            };
            let tx = this.audio_tx.clone();
            let spawned = std::thread::Builder::new()
                .name("codec".into())
                .spawn(move || {
                    let _ = tx.send(AudioRead::CodecSet(bluetooth_codec::select(
                        &id,
                        &wanted.profile,
                    )));
                });
            if spawned.is_err() {
                notify(tr("codec : changement refusé par PipeWire").to_string());
            }
        });
    }

    fn reload_scenes(&self, send: &Send) {
        let (scenes, errors) = paths::load_scenes();
        self.scenes.remove_all();
        for scene in scenes {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&scene.name).as_str())
                .subtitle(glib::markup_escape_text(&viewmodel::scene_preview(&scene)).as_str())
                .build();
            let apply = gtk::Button::with_label(tr("Appliquer"));
            apply.set_valign(gtk::Align::Center);
            apply.set_sensitive(self.writable.get());
            let send = send.clone();
            apply.connect_clicked(move |_| send(Command::ApplyScene(scene.name.clone())));
            row.add_suffix(&apply);
            self.scenes.append(&row);
        }
        for error in errors {
            let row = adw::ActionRow::builder()
                .title(tr("Ligne ignorée"))
                .subtitle(glib::markup_escape_text(&error).as_str())
                .build();
            self.scenes.append(&row);
        }
    }

    fn reload_battery(&self) {
        let now = unix_now();
        let log = paths::battery_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| BatteryLog::parse(&text, now))
            .unwrap_or_default();
        let current = self.snapshot.borrow().state.map(|s| s.battery);
        let span = usize::try_from(self.battery_span.selected()).unwrap_or(0);
        let view = viewmodel::battery_view(&log, now, span, current);
        let mut lines = vec![
            (tr("Batterie").to_string(), view.current),
            (tr("Estimation").to_string(), view.estimate),
        ];
        lines.extend(view.details.into_iter().map(|r| (r.label, r.value)));
        let events = momentum_core::journal::load(now);
        lines.extend(
            battery_log::autonomy_lines(&log, &events)
                .into_iter()
                .map(|line| (tr("Autonomie").to_string(), line)),
        );
        self.battery_rows.set(lines);
        let mut journal: Vec<(String, String)> = events
            .recent(30)
            .into_iter()
            .map(|e| (momentum_core::journal::ago(now, e.time), e.describe()))
            .collect();
        if journal.is_empty() {
            journal.push((
                tr("Journal des événements").to_string(),
                tr("Aucun événement noté pour l'instant.").to_string(),
            ));
        }
        self.journal.set(journal);
        *self.battery_path.borrow_mut() = view.path;
        self.battery_chart.queue_draw();
    }

    fn reload_audio(&self) {
        let session = self.snapshot.borrow().session;
        let Some(id) = self.snapshot.borrow().headset_id.clone() else {
            *self.pipewire.borrow_mut() = Some(Err(AudioReadError::NoIdentity));
            self.refresh_audio();
            return;
        };
        if demo::requested() {
            let audio = PipeWireAudio {
                outputs: vec![momentum_core::pipewire_audio::Output {
                    profile: Some(momentum_core::pipewire_audio::Profile::A2dp),
                    codec: Some("AAC"),
                    default: Some(true),
                }],
                default_headset: Some(true),
            };
            *self.pipewire.borrow_mut() = Some(Ok(audio));
            let choice = |label: &str, active| bluetooth_codec::CodecChoice {
                profile: format!("a2dp-sink-{}", label.to_lowercase()),
                label: label.to_string(),
                active,
            };
            *self.codec_card.borrow_mut() = Some(CodecCard {
                card: "bluez_card.demo".to_string(),
                choices: vec![choice("AAC", true), choice("SBC", false)],
            });
            let updating = self.updating.borrow().clone();
            self.refresh_codec(&updating);
            self.refresh_audio();
            return;
        }
        let tx = self.audio_tx.clone();
        let _ = std::thread::Builder::new()
            .name("lecture-audio".into())
            .spawn(move || {
                let result = pipewire::read(&id);
                let _ = tx.send(AudioRead::Pipewire(session, id.clone(), result));
                let _ = tx.send(AudioRead::Codecs(bluetooth_codec::read(&id)));
            });
    }

    /// Résultats des lectures faites hors du fil de l'interface.
    pub fn poll(&self, notify: &Notify, updating: &Cell<bool>) {
        while let Ok(result) = self.audio_rx.try_recv() {
            match result {
                AudioRead::Pipewire(session, id, audio) => {
                    // Un résultat de l'ancienne connexion ne devient jamais actuel.
                    if pipewire::is_current(&self.snapshot.borrow(), session, &id) {
                        *self.pipewire.borrow_mut() = Some(audio);
                        self.refresh_audio();
                    }
                }
                AudioRead::Codecs(card) => {
                    *self.codec_card.borrow_mut() = card;
                }
                AudioRead::CodecSet(Ok(card)) => {
                    *self.codec_card.borrow_mut() = Some(card);
                }
                AudioRead::CodecSet(Err(reason)) => notify(reason),
            }
            self.refresh_codec(updating);
        }
    }

    fn refresh_codec(&self, updating: &Cell<bool>) {
        let was = updating.replace(true);
        let card = self.codec_card.borrow();
        let labels: Vec<&str> = card
            .iter()
            .flat_map(|c| c.choices.iter().map(|x| x.label.as_str()))
            .collect();
        self.codec.set_model(Some(&gtk::StringList::new(&labels)));
        self.codec.set_sensitive(!labels.is_empty());
        let active = card
            .as_ref()
            .and_then(|c| c.choices.iter().position(|x| x.active))
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or(gtk::INVALID_LIST_POSITION);
        self.codec.set_selected(active);
        updating.set(was);
    }

    fn refresh_audio(&self) {
        let s = self.snapshot.borrow();
        let connections: Vec<_> = s
            .connections
            .iter()
            .enumerate()
            .map(|(i, c)| c.to_connection(i))
            .collect();
        let v = viewmodel::pipewire_audio_view(
            &viewmodel::AudioInputs {
                device_name: s.device_name.as_deref(),
                state: s.state.as_ref(),
                connections: &connections,
                ..viewmodel::AudioInputs::default()
            },
            self.pipewire.borrow().as_ref(),
            None,
        );
        let rows = |rows: Vec<viewmodel::InfoRow>| -> Vec<(String, String)> {
            rows.into_iter()
                .map(|r| (r.label, trf("{} ({})", &[&r.value, &r.source])))
                .collect()
        };
        self.findings.set(
            v.findings
                .into_iter()
                .map(|(_, text)| (tr("Constat").to_string(), text))
                .collect(),
        );
        self.control.set(rows(v.control));
        self.system.set(rows(v.windows));
        self.stream.set(rows(v.stream));
    }

    fn refresh_presets(&self, s: &Snapshot) {
        let gains = s.equalizer.as_ref().map(|e| e.gains.as_slice());
        let all = self.presets(s);
        let v = viewmodel::sound_view(s.state.as_ref(), gains, &all, self.writable.get());
        if *self.preset_names.borrow() != v.preset_names {
            let names: Vec<&str> = v.preset_names.iter().map(String::as_str).collect();
            self.preset.set_model(Some(&gtk::StringList::new(&names)));
            *self.preset_names.borrow_mut() = v.preset_names.clone();
        }
        self.preset.set_selected(index(v.current_preset));
        self.preset_status.set_subtitle(&v.preset_status);
        self.preset_delete.set_sensitive(
            usize::try_from(self.preset.selected())
                .ok()
                .and_then(|i| v.removable.get(i).copied())
                .unwrap_or(false),
        );
        self.preset_apply.set_sensitive(self.writable.get());
        self.preset_name.set_sensitive(self.writable.get());
    }

    fn refresh_ab(&self) {
        let writable = self.writable.get();
        let ab = self.ab.borrow();
        self.ab_remember.set_sensitive(writable);
        self.ab_toggle.set_sensitive(writable && ab.ready());
        self.ab_toggle.set_label(ab.label());
    }

    /// Curseurs de l'égaliseur : créés quand le nombre de bandes est connu.
    fn ensure_bands(&self, count: usize, min: f64, max: f64) {
        if self.bands.borrow().len() == count {
            return;
        }
        let Some(send) = self.send.borrow().clone() else {
            return;
        };
        let updating = self.updating.borrow().clone();
        let mut bands = self.bands.borrow_mut();
        bands.clear();
        for band in 0..count {
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 0.5);
            scale.set_hexpand(true);
            scale.set_width_request(220);
            scale.set_draw_value(true);
            let row = adw::ActionRow::builder()
                .title(trf("Bande {}", &[&(band + 1)]))
                .build();
            row.add_suffix(&scale);
            self.band_group.add(&row);
            let send = send.clone();
            let updating = updating.clone();
            scale.connect_value_changed(move |scale| {
                if updating.get() {
                    return;
                }
                let value = scale.value();
                let scale = scale.clone();
                let send = send.clone();
                glib::timeout_add_local_once(crate::app::SETTLE, move || {
                    if (scale.value() - value).abs() < f64::EPSILON
                        && let Ok(index) = u8::try_from(band)
                    {
                        let tenths = (value * 10.0).round().clamp(-128.0, 127.0) as i8;
                        send(Command::SetEqBand(index, tenths));
                    }
                });
            });
            bands.push(scale);
        }
        self.band_group.remove(&self.gains_text);
        self.band_group.add(&self.gains_text);
    }

    fn refresh_devices(&self, s: &Snapshot, writable: bool) {
        let Some(send) = self.send.borrow().clone() else {
            return;
        };
        let rows: Vec<_> = s
            .connections
            .iter()
            .map(|c| {
                let row = viewmodel::connection_row(&c.name, c.connected, c.this_device, writable);
                (row.name, row.status, row.action, row.can_act)
            })
            .collect();
        if *self.devices_last.borrow() == rows {
            return;
        }
        self.devices.remove_all();
        for (index, (name, status, action, can_act)) in rows.iter().enumerate() {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(name).as_str())
                .subtitle(status.as_str())
                .build();
            let button = gtk::Button::with_label(&momentum_core::i18n::translate(action));
            button.set_valign(gtk::Align::Center);
            button.set_sensitive(*can_act);
            let send = send.clone();
            let name = name.clone();
            let connect = action == "Connecter";
            let device = s
                .connections
                .get(index)
                .map(|c| c.id.clone())
                .filter(|id| !id.is_empty());
            button.connect_clicked(move |_| {
                send(Command::ToggleConnection {
                    index,
                    name: name.clone(),
                    connect,
                    device: device.clone(),
                });
            });
            row.add_suffix(&button);
            self.devices.append(&row);
        }
        *self.devices_last.borrow_mut() = rows;
    }

    /// Recopie l'état dans les contrôles ; `updating` est déjà vrai.
    pub fn refresh(&self, s: &Snapshot, controls: &viewmodel::HomeControls, writable: bool) {
        if let Some(problem) = &s.problem
            && self.snapshot.borrow().problem.as_ref() != Some(problem)
        {
            let mut incidents = self.incidents.borrow_mut();
            incidents.push((unix_now(), problem.clone()));
            let extra = incidents.len().saturating_sub(20);
            incidents.drain(..extra);
        }
        let identity_changed = self.snapshot.borrow().headset_id != s.headset_id;
        *self.snapshot.borrow_mut() = s.clone();
        // Page ouverte avant l'arrivée de l'état : ses lectures sont à refaire.
        if identity_changed {
            match self.stack.visible_child_name().as_deref() {
                Some("audio") => self.reload_audio(),
                Some("batterie") => self.reload_battery(),
                _ => {}
            }
        }
        let changed = self.writable.replace(writable) != writable;
        if changed
            && self.stack.visible_child_name().as_deref() == Some("scenes")
            && let Some(send) = self.send.borrow().clone()
        {
            self.reload_scenes(&send);
        }
        let can_files = s.equalizer.is_some();
        self.curve_export.set_sensitive(can_files);
        self.apo_export.set_sensitive(can_files);
        self.gains_text.set_sensitive(writable && can_files);
        self.curve_import.set_sensitive(writable && can_files);
        self.profile_export.set_sensitive(writable);
        self.bind_legacy
            .set_sensitive(writable && s.eq_topology().is_some());
        self.scene_capture.set_sensitive(s.state.is_some());
        self.head_tracking.set_active(controls.head_tracking_on);
        self.head_tracking
            .set_sensitive(controls.head_tracking_enabled);
        self.touch.set_active(controls.touch_on);
        self.touch.set_sensitive(controls.touch_enabled);
        let multipoint = s.info.as_ref().and_then(|i| i.multipoint);
        self.multipoint.set_active(multipoint == Some(true));
        self.multipoint
            .set_sensitive(writable && multipoint.is_some());
        self.refresh_devices(s, writable);

        let st = viewmodel::settings_view(s.state.as_ref(), s.info.as_ref(), writable);
        self.smart_pause.set_active(st.smart_pause.on);
        self.smart_pause.set_sensitive(st.smart_pause.enabled);
        self.auto_answer.set_active(st.auto_answer.on);
        self.auto_answer.set_sensitive(st.auto_answer.enabled);
        self.comfort.set_active(st.comfort_call.on);
        self.comfort.set_sensitive(st.comfort_call.enabled);
        self.protection.set_active(st.battery_protection.on);
        self.protection.set_sensitive(st.battery_protection.enabled);
        self.sidetone.set_selected(index(st.sidetone));
        self.sidetone.set_sensitive(st.sidetone_enabled);
        self.auto_off.set_selected(index(st.auto_off));
        self.auto_off.set_sensitive(st.auto_off_enabled);
        self.prompts.set_selected(index(st.prompt_mode));
        self.prompts.set_sensitive(st.prompts_enabled);
        if st.prompt_volume >= 0 {
            self.prompt_volume.set_value(f64::from(st.prompt_volume));
        }
        self.prompt_volume.set_sensitive(st.prompt_volume_enabled);
        if self.name.text().is_empty() {
            self.name.set_text(&st.name);
        }
        self.name.set_sensitive(st.rename_enabled);
        self.info
            .set(st.info.into_iter().map(|r| (r.label, r.value)).collect());

        let gains = s.equalizer.as_ref().map(|e| e.gains.as_slice());
        let all = self.presets(s);
        let sound = viewmodel::sound_view(s.state.as_ref(), gains, &all, writable);
        self.audio_mode.set_selected(index(sound.audio_mode));
        self.audio_mode.set_sensitive(sound.modes_enabled);
        if let Some(eq) = &s.equalizer {
            self.ensure_bands(
                eq.gains.len(),
                f64::from(eq.min_gain) / 10.0,
                f64::from(eq.max_gain) / 10.0,
            );
            for (scale, gain) in self.bands.borrow().iter().zip(&eq.gains) {
                scale.set_value(f64::from(*gain) / 10.0);
                scale.set_sensitive(writable);
            }
            self.bass.set_active(eq.bass_boost);
        }
        self.bass.set_sensitive(writable && s.equalizer.is_some());
        self.flat.set_sensitive(writable && s.equalizer.is_some());
        self.refresh_presets(s);
        self.refresh_ab();
        self.refresh_audio();
        let version = env!("CARGO_PKG_VERSION");
        let mut about: Vec<(String, String)> = viewmodel::about_rows(version, s.info.as_ref())
            .into_iter()
            .map(|r| (r.label, r.value))
            .collect();
        about.extend(
            viewmodel::device_rows(s.info.as_ref())
                .into_iter()
                .map(|r| (r.label, r.value)),
        );
        self.about.set(about);
    }
}
