//! Icône de la barre système et fenêtre d'égaliseur pour le MOMENTUM 5.
//!
//! Une seule boucle d'événements, celle de Slint (backend winit) : sous
//! Windows, elle pompe les messages du thread principal, ce qui suffit aussi
//! à `tray-icon` ; sous Linux, `tray-icon` passe par GTK (icône
//! StatusNotifierItem, native dans KDE), dont les événements sont pompés par
//! un minuteur Slint. Les éléments de menu et la fenêtre ne sont pas `Send` : ils
//! vivent dans un `thread_local`, et les autres threads (menu, casque) leur
//! parlent via `slint::invoke_from_event_loop`. Le thread `worker` possède
//! seul la connexion au casque.

// En release, pas de fenêtre console derrière l'icône.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod window;
#[cfg(target_os = "linux")]
#[path = "window_linux.rs"]
mod window;

#[cfg(any(windows, target_os = "linux"))]
mod capture;
#[cfg(any(windows, target_os = "linux"))]
mod demo;

fn main() {
    #[cfg(any(windows, target_os = "linux"))]
    if let Err(e) = app::main() {
        tracing::error!("arrêt sur erreur : {e:#}");
        eprintln!("erreur : {e:#}");
        std::process::exit(1);
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    eprintln!("momentum-tray ne fonctionne que sous Windows et Linux");
}

#[cfg(any(windows, target_os = "linux"))]
mod app {
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::sync::mpsc::{self, Sender};

    use anyhow::Context;
    use momentum_core::alerts::{Alert, AlertTracker};
    use momentum_core::battery_log::BatteryLog;
    use momentum_core::device::{
        AUTO_OFF_CHOICES, AudioMode, EqState, HeadsetState, NoiseModeRequest, PromptMode,
        SoundProfileStatus,
    };
    use momentum_core::diagnostic::{self, DiagnosticInput};
    use momentum_core::hotkeys::{self, HotkeyAction};

    use momentum_core::paths;
    use momentum_core::presets::{self, Preset};
    use momentum_core::protocol::messages::M5NoiseMode;
    use momentum_core::scenes::{self, Scene};
    use momentum_core::ui::{
        ICON_SIZE, IconKind, LEVEL_PRESETS, audio_summary, band_label, icon_rgba, tooltip,
    };
    use momentum_core::viewmodel::{self, Page};
    use momentum_service::desktop::HotkeyThread;
    use slint::{ComponentHandle, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tray_icon::menu::{
        CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    };
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    use momentum_service::rotating_log::RotatingFile;
    use momentum_service::system::{Autostart, Instance, acquire_instance};
    use momentum_service::worker::{self, WorkerCommand, WorkerEvent};

    slint::include_modules!();

    /// Éléments du menu dont on change l'état.
    struct MenuView {
        menu: Menu,
        status: MenuItem,
        /// Dernier incident, affiché sous le statut seulement s'il y en a un.
        problem: MenuItem,
        problem_shown: std::cell::Cell<bool>,
        adaptive: CheckMenuItem,
        custom: CheckMenuItem,
        off: CheckMenuItem,
        level_menu: Submenu,
        levels: Vec<(u8, CheckMenuItem)>,
        anti_wind: CheckMenuItem,
        spatial: CheckMenuItem,
        speech_clarity: CheckMenuItem,
        sound_profile: CheckMenuItem,
        audio_info: MenuItem,
        settings_menu: Submenu,
        touch_controls: CheckMenuItem,
        battery_protection: CheckMenuItem,
        smart_pause: CheckMenuItem,
        auto_answer: CheckMenuItem,
        comfort_call: CheckMenuItem,
        auto_off_menu: Submenu,
        auto_off: Vec<(u16, CheckMenuItem)>,
        prompts_menu: Submenu,
        prompts_tones: CheckMenuItem,
        prompts_voice: CheckMenuItem,
        open: MenuItem,
        equalizer: MenuItem,
        scenes_menu: Submenu,
        reload_scenes: MenuItem,
        autostart: CheckMenuItem,
        quit: MenuItem,
    }

    impl MenuView {
        fn build() -> anyhow::Result<Self> {
            let check = |label: &str| CheckMenuItem::new(label, false, false, None);
            let view = MenuView {
                menu: Menu::new(),
                status: MenuItem::new("Recherche du casque…", false, None),
                problem: MenuItem::new("", false, None),
                problem_shown: std::cell::Cell::new(false),
                adaptive: check("ANC adaptatif"),
                custom: check("Personnalisé"),
                off: check("Désactivé"),
                level_menu: Submenu::new("Curseur ANC ↔ transparence", false),
                levels: LEVEL_PRESETS
                    .iter()
                    .map(|(level, name)| (*level, check(&format!("{name} ({level} %)"))))
                    .collect(),
                anti_wind: check("Anti-vent"),
                spatial: check("Audio spatial"),
                speech_clarity: check("Speech Clarity (voix)"),
                sound_profile: check("Personnalisation du son"),
                audio_info: MenuItem::new("Mode audio : —", false, None),
                settings_menu: Submenu::new("Réglages du casque", false),
                touch_controls: check("Commandes tactiles"),
                battery_protection: check("Protection de la batterie"),
                smart_pause: check("Smart Pause"),
                auto_answer: check("Réponse automatique aux appels"),
                comfort_call: check("Appels confortables"),
                auto_off_menu: Submenu::new("Extinction automatique", true),
                auto_off: AUTO_OFF_CHOICES
                    .iter()
                    .map(|&s| {
                        let label = if s == 0 {
                            "Jamais".to_string()
                        } else {
                            format!("{} min", s / 60)
                        };
                        (s, check(&label))
                    })
                    .collect(),
                prompts_menu: Submenu::new("Invites", true),
                prompts_tones: check("Tonalités seules"),
                prompts_voice: check("Tonalités et voix"),
                open: MenuItem::new("Ouvrir la fenêtre…", true, None),
                equalizer: MenuItem::new("Égaliseur…", true, None),
                scenes_menu: Submenu::new("Scènes", false),
                reload_scenes: MenuItem::new("Recharger le fichier des scènes", true, None),
                autostart: CheckMenuItem::new(
                    AUTOSTART_LABEL,
                    true,
                    Autostart::new().is_enabled(),
                    None,
                ),
                quit: MenuItem::new("Quitter", true, None),
            };
            for (_, item) in &view.levels {
                view.level_menu.append(item)?;
            }
            for (_, item) in &view.auto_off {
                view.auto_off_menu.append(item)?;
            }
            view.prompts_menu
                .append_items(&[&view.prompts_tones, &view.prompts_voice])?;
            view.settings_menu.append_items(&[
                &view.smart_pause,
                &view.auto_answer,
                &view.comfort_call,
                &view.auto_off_menu,
                &view.prompts_menu,
                &PredefinedMenuItem::separator(),
                &view.touch_controls,
                &view.battery_protection,
            ])?;
            view.menu.append_items(&[
                &view.open,
                &view.status,
                &PredefinedMenuItem::separator(),
                &view.adaptive,
                &view.custom,
                &view.off,
                &view.level_menu,
                &PredefinedMenuItem::separator(),
                &view.anti_wind,
                &view.spatial,
                &view.speech_clarity,
                &view.sound_profile,
                &view.audio_info,
                &view.equalizer,
                &view.settings_menu,
                &view.scenes_menu,
                &PredefinedMenuItem::separator(),
                &view.autostart,
                &view.quit,
            ])?;
            Ok(view)
        }

        /// Recopie l'état du casque dans le menu (ou grise tout si absent).
        fn show(
            &self,
            name: Option<&str>,
            state: Option<&HeadsetState>,
            problem: Option<&str>,
            writable: bool,
        ) {
            match problem {
                Some(text) => {
                    self.problem.set_text(format!("⚠ {text}"));
                    if !self.problem_shown.get() && self.menu.insert(&self.problem, 1).is_ok() {
                        self.problem_shown.set(true);
                    }
                }
                None => {
                    if self.problem_shown.get() && self.menu.remove(&self.problem).is_ok() {
                        self.problem_shown.set(false);
                    }
                }
            }
            self.audio_info.set_text(match state {
                Some(s) => audio_summary(s),
                None => "Mode audio : —".to_string(),
            });
            // Écritures possibles : casque connecté et modèle validé.
            let writable = state.is_some() && writable;
            for item in [&self.adaptive, &self.custom, &self.off, &self.anti_wind] {
                item.set_enabled(writable);
            }
            // Un réglage non lu (refusé par le casque) reste grisé.
            let toggle = |item: &CheckMenuItem, value: Option<bool>| {
                item.set_enabled(writable && value.is_some());
                item.set_checked(value == Some(true));
            };
            let s = state.copied().unwrap_or_default();
            toggle(&self.spatial, s.spatial_audio);
            // Comme l'app : Dolby Atmos et profil personnalisé s'excluent ;
            // on ne propose d'activer l'audio spatial qu'hors du profil.
            let profile_selected = s.audio_mode == Some(AudioMode::Personalization);
            if profile_selected && s.spatial_audio == Some(false) {
                self.spatial.set_enabled(false);
            }
            // Profil sélectionnable seulement s'il existe sur le casque.
            toggle(
                &self.sound_profile,
                s.audio_mode
                    .filter(|_| s.sound_profile == Some(SoundProfileStatus::Received))
                    .map(|m| m == AudioMode::Personalization),
            );
            toggle(
                &self.speech_clarity,
                s.audio_mode.map(|m| m == AudioMode::SpeechClarity),
            );
            toggle(&self.touch_controls, s.touch_controls);
            toggle(&self.battery_protection, s.battery_protection);
            toggle(&self.smart_pause, s.smart_pause);
            toggle(&self.auto_answer, s.auto_answer);
            toggle(&self.comfort_call, s.comfort_call);
            toggle(
                &self.prompts_tones,
                s.prompt_mode.map(|m| m == PromptMode::TonesOnly),
            );
            toggle(
                &self.prompts_voice,
                s.prompt_mode.map(|m| m == PromptMode::TonesAndVoice),
            );
            for (seconds, item) in &self.auto_off {
                toggle(item, s.auto_off_seconds.map(|v| v == *seconds));
            }
            self.level_menu.set_enabled(writable);
            self.settings_menu.set_enabled(state.is_some());
            self.scenes_menu.set_enabled(writable);
            self.status.set_text(match (name, state) {
                (Some(n), Some(s)) => format!("{n} — batterie {} %", s.battery),
                (Some(n), None) => format!("{n} — connexion…"),
                _ => "Casque déconnecté — recherche…".to_string(),
            });

            let mode = state.map(|s| s.noise.mode());
            self.adaptive
                .set_checked(matches!(mode, Some(M5NoiseMode::Adaptive)));
            self.custom
                .set_checked(matches!(mode, Some(M5NoiseMode::Custom { .. })));
            self.off.set_checked(matches!(mode, Some(M5NoiseMode::Off)));
            for (level, item) in &self.levels {
                item.set_checked(
                    matches!(mode, Some(M5NoiseMode::Custom { level: l }) if l == *level),
                );
            }
            self.anti_wind
                .set_checked(state.is_some_and(|s| s.noise.anti_wind));
        }

        /// Traduit un clic en commande pour le casque (rien si le réglage
        /// n'a pas été lu).
        fn command_for(&self, id: &MenuId, state: Option<&HeadsetState>) -> Option<WorkerCommand> {
            let state = state?;
            if id == self.adaptive.id() {
                Some(WorkerCommand::SetMode(NoiseModeRequest::Adaptive))
            } else if id == self.custom.id() {
                Some(WorkerCommand::SetMode(NoiseModeRequest::Custom))
            } else if id == self.off.id() {
                Some(WorkerCommand::SetMode(NoiseModeRequest::Off))
            } else if id == self.anti_wind.id() {
                Some(WorkerCommand::SetAntiWind(!state.noise.anti_wind))
            } else if id == self.spatial.id() {
                state.spatial_audio.map(|v| WorkerCommand::SetSpatial(!v))
            } else if id == self.sound_profile.id() {
                state
                    .audio_mode
                    .map(|m| WorkerCommand::SetSoundProfile(m != AudioMode::Personalization))
            } else if id == self.speech_clarity.id() {
                state
                    .audio_mode
                    .map(|m| WorkerCommand::SetSpeechClarity(m != AudioMode::SpeechClarity))
            } else if id == self.touch_controls.id() {
                state
                    .touch_controls
                    .map(|v| WorkerCommand::SetTouchControls(!v))
            } else if id == self.smart_pause.id() {
                state.smart_pause.map(|v| WorkerCommand::SetSmartPause(!v))
            } else if id == self.auto_answer.id() {
                state.auto_answer.map(|v| WorkerCommand::SetAutoAnswer(!v))
            } else if id == self.comfort_call.id() {
                state
                    .comfort_call
                    .map(|v| WorkerCommand::SetComfortCall(!v))
            } else if id == self.prompts_tones.id() {
                Some(WorkerCommand::SetPromptMode(PromptMode::TonesOnly))
            } else if id == self.prompts_voice.id() {
                Some(WorkerCommand::SetPromptMode(PromptMode::TonesAndVoice))
            } else if let Some((seconds, _)) =
                self.auto_off.iter().find(|(_, item)| id == item.id())
            {
                Some(WorkerCommand::SetAutoOff(*seconds))
            } else if id == self.battery_protection.id() {
                state
                    .battery_protection
                    .map(|v| WorkerCommand::SetBatteryProtection(!v))
            } else {
                self.levels
                    .iter()
                    .find(|(_, item)| id == item.id())
                    .map(|(level, _)| WorkerCommand::SetLevel(*level))
            }
        }
    }

    /// Tout l'état de l'interface, propre au thread principal.
    struct App {
        view: MenuView,
        tray: TrayIcon,
        window: MainWindow,
        bands: Rc<VecModel<Band>>,
        commands: Commands,
        /// Mode démonstration : faux worker, aucun fichier de l'utilisateur
        /// modifié par les données simulées.
        demo: bool,
        /// Délai maximal de l'indicateur « envoi au casque ».
        busy_timer: slint::Timer,
        device_name: Option<String>,
        state: Option<HeadsetState>,
        paired: Vec<momentum_core::device::Connection>,
        eq: Option<EqState>,
        info: Option<momentum_core::device::DeviceInfo>,
        /// Préréglages personnels (fichier `prereglages.txt`).
        user_presets: Vec<Preset>,
        /// Information ponctuelle (export, préréglage enregistré…).
        notice: Option<String>,
        /// Décide des notifications Windows (batterie, connexion).
        alerts: AlertTracker,
        /// Dernier incident à montrer (effacé à la reconnexion ou à la
        /// commande suivante).
        problem: Option<String>,
        /// Modèle non validé : lecture seule (message permanent).
        read_only: Option<String>,
        /// Scènes du fichier `scenes.txt` et leurs entrées de menu.
        scenes: Vec<Scene>,
        scene_items: Vec<(String, MenuItem)>,
        /// Historique local de la batterie (fichier `batterie.csv`).
        battery_log: BatteryLog,
        /// Période affichée (indice dans `viewmodel::BATTERY_SPANS`).
        battery_span: usize,
        /// Derniers incidents (horodatage Unix, message), pour le diagnostic.
        incidents: std::collections::VecDeque<(u64, String)>,
        /// Thread des raccourcis clavier (remplacé à l'enregistrement).
        hotkey_thread: Option<HotkeyThread>,
        /// Éditeur des raccourcis : action et touches saisies.
        hotkey_edits: Vec<(HotkeyAction, String)>,
        /// Comparaison A/B de l'égaliseur : courbe A, courbe B, A à l'écoute.
        ab_a: Option<Vec<i8>>,
        ab_b: Option<Vec<i8>>,
        ab_on_a: bool,
        /// Connexion et audio : heures des lectures (heure locale).
        control_since: Option<String>,
        connections_at: Option<String>,
        codec_since: Option<String>,
        rate_since: Option<String>,
        /// Dernière lecture des sorties de Windows et son heure.
        windows_audio: Option<Result<momentum_core::audio_check::WindowsAudio, String>>,
        windows_at: Option<String>,
        /// Éditeur de scènes : brouillon (nom d'origine dans le fichier,
        /// scène), scène choisie, modifications non enregistrées.
        scene_draft: Vec<(Option<String>, Scene)>,
        scene_selected: usize,
        scene_dirty: bool,
        /// Texte de `scenes.txt` à l'ouverture de l'éditeur : une
        /// modification à la main entre-temps bloque l'enregistrement.
        scene_loaded_text: Option<String>,
        /// Erreurs du fichier : l'enregistrement est refusé (rien à perdre).
        scene_file_errors: Vec<String>,
        scene_form_error: Option<String>,
        scene_message: Option<String>,
        scene_eq_choices: Vec<String>,
        /// Contenu du panneau rapide (`panneau.txt`).
        panel_prefs: momentum_core::quick_panel::PanelPrefs,
        /// Panneau rapide (clic gauche sur l'icône).
        panel: QuickPanel,
        panel_visible: bool,
        /// Le panneau a été au premier plan depuis son ouverture : il se
        /// ferme dès qu'une autre fenêtre prend sa place.
        panel_focus_seen: bool,
        /// Fermeture par perte du premier plan : un clic sur l'icône donne
        /// d'abord le premier plan à la barre des tâches (le panneau se
        /// ferme), puis arrive à l'icône. Ce clic ne doit pas le rouvrir.
        panel_closed_by_focus: Option<std::time::Instant>,
        /// Surveille le premier plan pour fermer le panneau.
        panel_timer: slint::Timer,
    }

    /// Libellé du lancement automatique dans le menu.
    #[cfg(windows)]
    const AUTOSTART_LABEL: &str = "Lancer au démarrage de Windows";
    #[cfg(target_os = "linux")]
    const AUTOSTART_LABEL: &str = "Lancer à l'ouverture de session";

    /// Nombre maximal d'incidents gardés pour le diagnostic.
    const MAX_INCIDENTS: usize = 10;
    /// Après une fermeture par perte du premier plan, un clic sur l'icône
    /// dans ce délai est celui qui l'a causée : il ne rouvre pas le panneau
    /// (la vérification du premier plan passe toutes les 250 ms).
    const PANEL_REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(700);

    /// Horodatage Unix en secondes.
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    fn load_battery_log() -> BatteryLog {
        paths::battery_file()
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|text| BatteryLog::parse(&text, unix_now()))
            .unwrap_or_default()
    }

    fn store_battery_log(log: &BatteryLog) {
        let Some(path) = paths::battery_file() else {
            return;
        };
        if let Err(e) = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, log.to_text()))
        {
            tracing::warn!(%e, "enregistrement de l'historique de la batterie");
        }
    }

    thread_local! {
        static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    }

    /// Au-delà, l'indicateur « envoi au casque » s'efface même sans réponse
    /// (le worker signale lui-même refus et pertes de connexion).
    const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

    /// Canal des commandes de l'interface : chaque envoi affiche « envoi
    /// au casque » jusqu'à la réponse du worker (ou au délai maximal).
    #[derive(Clone)]
    struct Commands(Sender<WorkerCommand>);

    impl Commands {
        fn send(&self, command: WorkerCommand) -> Result<(), mpsc::SendError<WorkerCommand>> {
            let silent = matches!(command, WorkerCommand::Quit | WorkerCommand::Query(_));
            self.0.send(command)?;
            if !silent {
                let _ = slint::invoke_from_event_loop(|| with_app(App::show_busy));
            }
            Ok(())
        }

        /// Canal brut, pour le canal local de `ctl` (sans indicateur).
        #[cfg(windows)]
        fn raw(&self) -> Sender<WorkerCommand> {
            self.0.clone()
        }
    }

    /// Nom du système dans les libellés de la fenêtre.
    #[cfg(windows)]
    const SYSTEM_NAME: &str = "Windows";
    #[cfg(target_os = "linux")]
    const SYSTEM_NAME: &str = "Linux";

    /// Fichier du thème choisi (`apparence.txt`).
    fn appearance_path() -> Option<PathBuf> {
        Some(paths::data_dir()?.join("apparence.txt"))
    }

    /// Thème sombre, sauf si `apparence.txt` contient « theme = clair ».
    fn parse_dark_theme(text: &str) -> bool {
        !text.lines().any(|line| {
            line.split_once('=')
                .is_some_and(|(k, v)| k.trim() == "theme" && v.trim() == "clair")
        })
    }

    fn load_dark_theme() -> bool {
        appearance_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .is_none_or(|text| parse_dark_theme(&text))
    }

    fn store_dark_theme(dark: bool) {
        let Some(path) = appearance_path() else {
            return;
        };
        let text = format!(
            "# Apparence d'OpenHeadset Control : theme = sombre ou clair\ntheme = {}\n",
            if dark { "sombre" } else { "clair" }
        );
        if let Err(e) = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, text))
        {
            tracing::warn!(%e, "enregistrement du thème");
        }
    }

    /// Exécute `f` sur l'état de l'interface (thread principal uniquement).
    fn with_app(f: impl FnOnce(&mut App)) {
        APP.with(|app| {
            if let Some(app) = app.borrow_mut().as_mut() {
                f(app);
            }
        });
    }

    fn icon(kind: IconKind) -> anyhow::Result<Icon> {
        Icon::from_rgba(icon_rgba(kind, ICON_SIZE), ICON_SIZE, ICON_SIZE)
            .context("création de l'icône")
    }

    impl App {
        fn on_menu(&mut self, event: MenuEvent) {
            if event.id == *self.view.quit.id() {
                tracing::info!("arrêt demandé : menu « Quitter »");
                QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
                if !self.demo && self.battery_log.interrupt(unix_now()) {
                    store_battery_log(&self.battery_log);
                }
                let _ = self.commands.send(WorkerCommand::Quit);
                let _ = self.window.hide();
                let _ = slint::quit_event_loop();
                return;
            }
            if event.id == *self.view.open.id() {
                self.show_window(Page::Home);
                return;
            }
            if event.id == *self.view.equalizer.id() {
                self.show_window(Page::Sound);
                return;
            }
            if event.id == *self.view.reload_scenes.id() {
                self.reload_scenes();
                self.refresh();
                return;
            }
            if let Some(name) = self
                .scene_items
                .iter()
                .find(|(_, item)| event.id == *item.id())
                .map(|(name, _)| name.clone())
            {
                self.apply_scene(&name);
                return;
            }
            if event.id == *self.view.autostart.id() {
                let autostart = Autostart::new();
                let wanted = !autostart.is_enabled();
                match autostart.set(wanted) {
                    Ok(()) => tracing::info!(actif = wanted, "lancement au démarrage"),
                    Err(e) => tracing::warn!(%e, "lancement au démarrage"),
                }
                // Toujours afficher l'état réel du registre.
                self.view.autostart.set_checked(autostart.is_enabled());
                return;
            }
            if let Some(command) = self.view.command_for(&event.id, self.state.as_ref()) {
                let _ = self.commands.send(command);
                self.clear_problem();
            }
            // Windows coche/décoche tout seul au clic : on remet l'état réel,
            // la confirmation du casque arrivera juste après.
            self.refresh();
        }

        /// Raccourci clavier global.
        fn on_hotkey(&mut self, action: HotkeyAction) {
            if action == HotkeyAction::OpenWindow {
                self.show_window(Page::Home);
                return;
            }
            if let HotkeyAction::Scene(name) = &action {
                self.apply_scene(name);
                return;
            }
            let Some(state) = self.state else { return };
            if self.read_only.is_some() {
                return;
            }
            let command = match action {
                HotkeyAction::CycleNoise => Some(WorkerCommand::SetMode(hotkeys::next_noise_mode(
                    state.noise.mode(),
                ))),
                HotkeyAction::ToggleProfile => {
                    hotkeys::profile_toggle(&state).map(WorkerCommand::SetSoundProfile)
                }
                HotkeyAction::OpenWindow | HotkeyAction::Scene(_) => None,
            };
            if let Some(command) = command {
                let _ = self.commands.send(command);
                self.clear_problem();
            }
        }

        /// Relit `scenes.txt` (créé avec des exemples s'il manque) et
        /// reconstruit le sous-menu et les boutons de l'Accueil.
        fn reload_scenes(&mut self) {
            let (list, errors) = paths::load_scenes();
            for (_, item) in self.scene_items.drain(..) {
                let _ = self.view.scenes_menu.remove(&item);
            }
            let _ = self.view.scenes_menu.remove(&self.view.reload_scenes);
            for scene in &list {
                let item = MenuItem::new(&scene.name, true, None);
                let _ = self.view.scenes_menu.append(&item);
                self.scene_items.push((scene.name.clone(), item));
            }
            let _ = self.view.scenes_menu.append(&self.view.reload_scenes);
            self.window
                .set_scene_names(strings_model(list.iter().map(|s| s.name.clone()).collect()));

            if !errors.is_empty() {
                tracing::warn!(?errors, "scènes");
                self.notice = Some(format!("Fichier des scènes : {}", errors.join(" ; ")));
            }
            self.scenes = list;
            self.apply_panel_prefs();
            self.reload_hotkey_editor();
        }

        /// Applique une scène relue dans le fichier (les modifications à la
        /// main sont prises en compte sans redémarrer).
        fn apply_scene(&mut self, name: &str) {
            self.reload_scenes();
            if self.state.is_none() || self.read_only.is_some() {
                self.problem = Some("Scène impossible : casque absent ou en lecture seule".into());
            } else if let Some(scene) = scenes::find(&self.scenes, name).cloned() {
                let _ = self.commands.send(WorkerCommand::ApplyScene {
                    scene,
                    presets: paths::scene_presets(),
                    reply: None,
                });
                self.problem = None;
                self.notice = Some(format!("Scène « {name} » en cours…"));
            } else {
                self.problem = Some(format!("Scène « {name} » introuvable dans le fichier"));
            }
            self.refresh();
        }

        /// Ouvre le panneau rapide près du clic, ou le ferme s'il est ouvert.
        fn toggle_panel(&mut self, x: i32, y: i32) {
            if self.panel_visible {
                self.hide_panel();
                return;
            }
            if self
                .panel_closed_by_focus
                .take()
                .is_some_and(|at| at.elapsed() < PANEL_REOPEN_GUARD)
            {
                return;
            }
            self.refresh();
            if let Err(e) = self.panel.show() {
                tracing::warn!(%e, "ouverture du panneau rapide");
                return;
            }
            self.panel_visible = true;
            self.panel_focus_seen = false;
            let size = self.panel.window().size();
            let area = crate::window::work_area_at(x, y).unwrap_or((0, 0, 1920, 1080));
            let (px, py) = momentum_core::ui::panel_position(
                (x, y),
                (
                    i32::try_from(size.width).unwrap_or(400),
                    i32::try_from(size.height).unwrap_or(320),
                ),
                area,
            );
            self.panel
                .window()
                .set_position(slint::PhysicalPosition::new(px, py));
            crate::window::focus_panel();
        }

        fn hide_panel(&mut self) {
            self.panel_visible = false;
            let _ = self.panel.hide();
        }

        /// Appelé régulièrement : ferme le panneau quand il perd le premier plan.
        fn watch_panel(&mut self) {
            if !self.panel_visible {
                return;
            }
            if crate::window::panel_has_focus() {
                self.panel_focus_seen = true;
            } else if self.panel_focus_seen {
                self.hide_panel();
                self.panel_closed_by_focus = Some(std::time::Instant::now());
            }
        }

        /// Remplit l'éditeur des raccourcis : actions fixes puis une ligne
        /// par scène, avec les touches du fichier.
        fn reload_hotkey_editor(&mut self) {
            let (keys, _) = hotkeys::parse_config(&read_hotkey_config());
            let mut actions: Vec<HotkeyAction> = HotkeyAction::BUILTIN.to_vec();
            actions.extend(
                self.scenes
                    .iter()
                    .map(|s| HotkeyAction::Scene(s.name.clone())),
            );
            self.hotkey_edits = actions
                .into_iter()
                .map(|action| {
                    let spec = keys
                        .iter()
                        .find(|(a, _)| *a == action)
                        .map(|(_, k)| hotkeys::format_hotkey(k))
                        .unwrap_or_default();
                    (action, spec)
                })
                .collect();
            self.window.set_hotkey_rows(ModelRc::new(VecModel::from(
                self.hotkey_edits
                    .iter()
                    .map(|(action, spec)| HotkeyRow {
                        label: action.label().into(),
                        spec: spec.clone().into(),
                    })
                    .collect::<Vec<_>>(),
            )));
        }

        /// Écrit `raccourcis.txt` et réenregistre les raccourcis.
        fn save_hotkeys(&mut self) {
            let text = match hotkeys::build_config(&self.hotkey_edits) {
                Ok(text) => text,
                Err(reason) => {
                    self.notice = Some(format!("Raccourcis non enregistrés : {reason}."));
                    self.refresh();
                    return;
                }
            };
            if let Some(path) = hotkeys_path()
                && let Err(e) = path
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .and_then(|()| fs::write(&path, &text))
            {
                self.notice = Some(format!("Raccourcis non enregistrés : {e}."));
                self.refresh();
                return;
            }
            if let Some(thread) = self.hotkey_thread.take() {
                thread.stop();
            }
            let (thread, notice) = register_hotkeys(&text);
            self.hotkey_thread = thread;
            self.notice =
                Some(notice.unwrap_or_else(|| "Raccourcis enregistrés et actifs.".to_string()));
            self.reload_hotkey_editor();
            self.refresh();
        }

        /// Gains saisis au clavier (dB) appliqués au casque.
        fn apply_gains_text(&mut self, text: &str) {
            let Some(eq) = self.eq.as_ref() else { return };
            match presets::parse_gains_db(text, eq.gains.len(), eq.min_gain, eq.max_gain) {
                Ok(gains) => {
                    let _ = self.commands.send(WorkerCommand::SetEqGains(gains));
                    self.window.set_gains_text("".into());
                    self.problem = None;
                    self.notice = None;
                }
                Err(reason) => self.notice = Some(format!("Gains non appliqués : {reason}.")),
            }
            self.refresh();
        }

        /// Écrit la courbe actuelle dans Documents.
        fn export_curve(&mut self) {
            let Some(eq) = self.eq.as_ref() else { return };
            let model = self.info.as_ref().and_then(|i| i.model.as_deref());
            let path = export_path().with_file_name(format!(
                "momentum-egaliseur-{}.txt",
                momentum_core::profile::file_stamp(unix_now())
            ));
            let text = presets::format_curve(model, &eq.gains, eq.bass_boost);
            self.notice = Some(match fs::write(&path, text) {
                Ok(()) => format!("Courbe exportée dans {}.", path.display()),
                Err(e) => format!("Export de la courbe impossible : {e}."),
            });
            self.refresh();
        }

        /// Choisit un fichier de courbe et l'applique (gains, puis basses).
        fn import_curve(&mut self) {
            let Some(eq) = self.eq.clone() else { return };
            let Some(path) = crate::window::pick_curve_file() else {
                return;
            };
            let model = self.info.as_ref().and_then(|i| i.model.clone());
            let parsed = fs::read_to_string(&path)
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
            self.notice = Some(match parsed {
                Ok(curve) => {
                    let _ = self.commands.send(WorkerCommand::SetEqGains(curve.gains));
                    if let Some(on) = curve.bass_boost.filter(|&on| on != eq.bass_boost) {
                        let _ = self.commands.send(WorkerCommand::SetBassBoost(on));
                    }
                    format!("Courbe importée depuis {}.", path.display())
                }
                Err(reason) => format!("Courbe non importée : {reason}."),
            });
            self.refresh();
        }

        /// Mémorise la courbe actuelle comme A (la suite des réglages = B).
        fn remember_a(&mut self) {
            if let Some(eq) = self.eq.as_ref() {
                self.ab_a = Some(eq.gains.clone());
                self.ab_b = None;
                self.ab_on_a = false;
            }
            self.refresh();
        }

        /// Bascule entre A et B ; les deux courbes sont gardées.
        fn toggle_ab(&mut self) {
            let (Some(a), Some(eq)) = (self.ab_a.clone(), self.eq.as_ref()) else {
                return;
            };
            if self.ab_on_a {
                let b = self.ab_b.clone().unwrap_or_else(|| eq.gains.clone());
                let _ = self.commands.send(WorkerCommand::SetEqGains(b));
                self.ab_on_a = false;
            } else {
                self.ab_b = Some(eq.gains.clone());
                let _ = self.commands.send(WorkerCommand::SetEqGains(a));
                self.ab_on_a = true;
            }
            self.refresh();
        }

        /// Garde un incident pour le diagnostic.
        fn remember_incident(&mut self, message: &str) {
            if self.incidents.len() == MAX_INCIDENTS {
                self.incidents.pop_front();
            }
            self.incidents.push_back((unix_now(), message.to_string()));
        }

        /// Exporte l'historique de la batterie dans Documents.
        fn export_battery(&mut self) {
            let path = export_path().with_file_name(format!(
                "momentum-batterie-{}.csv",
                momentum_core::profile::file_stamp(unix_now())
            ));
            self.notice = Some(match fs::write(&path, self.battery_log.to_text()) {
                Ok(()) => format!("Historique de la batterie exporté dans {}.", path.display()),
                Err(e) => format!("Export de l'historique impossible : {e}."),
            });
            self.refresh();
        }

        fn clear_battery(&mut self) {
            self.battery_log.clear();
            store_battery_log(&self.battery_log);
            self.notice = Some("Historique de la batterie effacé.".to_string());
            self.refresh();
        }

        /// Prépare le diagnostic expurgé (affiché avant toute copie).
        /// Ouvre l'éditeur sur le contenu actuel de `scenes.txt`.
        fn load_scene_editor(&mut self) {
            // Crée le fichier d'exemples s'il manque.
            let _ = paths::load_scenes();
            let text = paths::scenes_file().and_then(|p| fs::read_to_string(p).ok());
            let (list, errors) = scenes::parse_scenes(text.as_deref().unwrap_or(""));
            self.scene_draft = list
                .into_iter()
                .map(|s| (Some(s.name.clone()), s))
                .collect();
            self.scene_selected = self
                .scene_selected
                .min(self.scene_draft.len().saturating_sub(1));
            self.scene_dirty = false;
            self.scene_form_error = None;
            self.scene_loaded_text = text;
            self.scene_message = (!errors.is_empty()).then(|| {
                format!(
                    "⚠ scenes.txt contient des erreurs : l'enregistrement est désactivé pour ne \
                     rien perdre. Corrigez le fichier (« Ouvrir scenes.txt »), puis revenez sur \
                     cette page. {}",
                    errors.join(" ; ")
                )
            });
            self.scene_file_errors = errors;
            self.push_scene_form();
            self.refresh_scene_editor();
        }

        /// Remplit le formulaire avec la scène choisie.
        fn push_scene_form(&mut self) {
            let scene = self
                .scene_draft
                .get(self.scene_selected)
                .map(|(_, s)| s.clone());
            self.scene_eq_choices =
                viewmodel::equalizer_choices(scene.as_ref(), &paths::scene_presets());
            let form = scene
                .as_ref()
                .map(|s| viewmodel::scene_form(s, &self.scene_eq_choices))
                .unwrap_or_default();
            let w = &self.window;
            w.set_scene_eq_choices(strings_model(self.scene_eq_choices.clone()));
            w.set_scene_name(form.name.into());
            w.set_scene_sound(form.sound);
            w.set_scene_spatial(form.spatial);
            w.set_scene_noise(form.noise);
            w.set_scene_level(form.level.into());
            w.set_scene_anti_wind(form.anti_wind);
            w.set_scene_touch(form.touch);
            w.set_scene_equalizer(form.equalizer);
            w.set_scene_bass_boost(form.bass_boost);
        }

        /// Liste, aperçu, boutons et panneau de la page Scènes.
        fn refresh_scene_editor(&self) {
            let w = &self.window;
            w.set_scene_list(strings_model(
                self.scene_draft
                    .iter()
                    .map(|(_, s)| s.name.clone())
                    .collect(),
            ));
            w.set_scene_selected(i32::try_from(self.scene_selected).unwrap_or(0));
            let scenes: Vec<Scene> = self.scene_draft.iter().map(|(_, s)| s.clone()).collect();
            let selected = scenes.get(self.scene_selected);
            w.set_scene_preview(
                selected
                    .map_or(String::new(), viewmodel::scene_preview)
                    .into(),
            );
            let list_error = scenes::check_scenes(&scenes).err();
            w.set_scene_error(
                self.scene_form_error
                    .clone()
                    .or(list_error.clone())
                    .unwrap_or_default()
                    .into(),
            );
            w.set_scene_message(self.scene_message.clone().unwrap_or_default().into());
            w.set_scene_dirty(self.scene_dirty);
            w.set_scene_can_save(
                self.scene_dirty
                    && self.scene_file_errors.is_empty()
                    && self.scene_form_error.is_none()
                    && list_error.is_none(),
            );
            w.set_scene_can_apply(
                !self.scene_dirty
                    && selected.is_some()
                    && self.state.is_some()
                    && self.read_only.is_none(),
            );
            let saved: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
            let in_panel = self
                .scene_draft
                .get(self.scene_selected)
                .and_then(|(origin, _)| origin.as_deref())
                .is_some_and(|name| {
                    self.panel_prefs
                        .shown_scenes(&saved)
                        .iter()
                        .any(|n| n.to_lowercase() == name.to_lowercase())
                });
            w.set_scene_in_panel(in_panel);
            w.set_panel_show_noise(self.panel_prefs.noise);
            w.set_panel_show_level(self.panel_prefs.level);
            w.set_panel_show_profile(self.panel_prefs.profile);
        }

        fn scene_select(&mut self, index: usize) {
            if index >= self.scene_draft.len() {
                return;
            }
            if self.scene_form_error.take().is_some() {
                self.scene_message =
                    Some("Modification non valide abandonnée (scène précédente).".to_string());
            }
            self.scene_selected = index;
            self.push_scene_form();
            self.refresh_scene_editor();
        }

        /// Le formulaire a changé : la scène du brouillon suit s'il est valide.
        fn scene_form_changed(&mut self) {
            let w = &self.window;
            let form = viewmodel::SceneForm {
                name: w.get_scene_name().to_string(),
                sound: w.get_scene_sound(),
                spatial: w.get_scene_spatial(),
                noise: w.get_scene_noise(),
                level: w.get_scene_level().to_string(),
                anti_wind: w.get_scene_anti_wind(),
                touch: w.get_scene_touch(),
                equalizer: w.get_scene_equalizer(),
                bass_boost: w.get_scene_bass_boost(),
            };
            self.scene_dirty = true;
            match viewmodel::scene_from_form(&form, &self.scene_eq_choices) {
                Ok(scene) => {
                    if let Some((_, slot)) = self.scene_draft.get_mut(self.scene_selected) {
                        *slot = scene;
                    }
                    self.scene_form_error = None;
                }
                Err(e) => self.scene_form_error = Some(e),
            }
            self.refresh_scene_editor();
        }

        /// Nom libre : `base`, puis `base 2`, `base 3`…
        fn free_scene_name(&self, base: &str) -> String {
            let taken = |n: &str| {
                self.scene_draft
                    .iter()
                    .any(|(_, s)| s.name.to_lowercase() == n.to_lowercase())
            };
            let base: String = base.chars().take(scenes::MAX_NAME_CHARS - 3).collect();
            if !taken(&base) {
                return base;
            }
            (2..)
                .map(|i| format!("{base} {i}"))
                .find(|n| !taken(n))
                .unwrap_or(base)
        }

        fn scene_new(&mut self) {
            let scene = Scene {
                name: self.free_scene_name("Nouvelle scène"),
                noise: Some(momentum_core::device::NoiseModeRequest::Adaptive),
                ..Scene::default()
            };
            self.scene_draft.push((None, scene));
            self.scene_after_list_change(self.scene_draft.len() - 1);
        }

        fn scene_duplicate(&mut self) {
            let Some((_, scene)) = self.scene_draft.get(self.scene_selected) else {
                return;
            };
            let mut copy = scene.clone();
            copy.name = self.free_scene_name(&format!("{} (copie)", scene.name));
            self.scene_draft
                .insert(self.scene_selected + 1, (None, copy));
            self.scene_after_list_change(self.scene_selected + 1);
        }

        fn scene_delete(&mut self) {
            if self.scene_selected < self.scene_draft.len() {
                self.scene_draft.remove(self.scene_selected);
                let next = self
                    .scene_selected
                    .min(self.scene_draft.len().saturating_sub(1));
                self.scene_after_list_change(next);
            }
        }

        fn scene_move(&mut self, delta: i32) {
            let from = self.scene_selected;
            let Some(to) = from.checked_add_signed(delta as isize) else {
                return;
            };
            if to < self.scene_draft.len() && from < self.scene_draft.len() {
                self.scene_draft.swap(from, to);
                self.scene_after_list_change(to);
            }
        }

        fn scene_after_list_change(&mut self, select: usize) {
            self.scene_dirty = true;
            self.scene_form_error = None;
            self.scene_message = None;
            self.scene_selected = select;
            self.push_scene_form();
            self.refresh_scene_editor();
        }

        /// Écrit `scenes.txt` (sans rien envoyer au casque) et fait suivre
        /// les renommages et suppressions dans les raccourcis et le panneau.
        fn scene_save(&mut self) {
            let scenes: Vec<Scene> = self.scene_draft.iter().map(|(_, s)| s.clone()).collect();
            if let Err(e) = scenes::check_scenes(&scenes) {
                self.scene_message = Some(format!("Non enregistré : {e}."));
                self.refresh_scene_editor();
                return;
            }
            let Some(path) = paths::scenes_file() else {
                return;
            };
            if fs::read_to_string(&path).ok() != self.scene_loaded_text {
                self.scene_message = Some(
                    "Non enregistré : scenes.txt a été modifié ailleurs depuis l'ouverture de \
                     l'éditeur. « Annuler les modifications » recharge le fichier."
                        .to_string(),
                );
                self.refresh_scene_editor();
                return;
            }
            if let Err(e) = paths::write_user_file(&path, &scenes::format_scenes(&scenes)) {
                self.scene_message = Some(format!("Non enregistré : {e}."));
                self.refresh_scene_editor();
                return;
            }
            // Renommages et suppressions : raccourcis et panneau suivent.
            let mut changes: Vec<(String, Option<String>)> = self
                .scene_draft
                .iter()
                .filter_map(|(origin, s)| {
                    origin
                        .as_ref()
                        .filter(|o| **o != s.name)
                        .map(|o| (o.clone(), Some(s.name.clone())))
                })
                .collect();
            for saved in &self.scenes {
                let kept = self
                    .scene_draft
                    .iter()
                    .any(|(origin, _)| origin.as_deref() == Some(saved.name.as_str()));
                if !kept {
                    changes.push((saved.name.clone(), None));
                }
            }
            let mut message = "Scènes enregistrées (rien n'a été envoyé au casque).".to_string();
            if !changes.is_empty() {
                let before = read_hotkey_config();
                let mut text = before.clone();
                for (old, new) in &changes {
                    text = hotkeys::rename_scene(&text, old, new.as_deref());
                    self.panel_prefs.rename_scene(old, new.as_deref());
                }
                self.store_panel_prefs();
                if text != before {
                    match hotkeys_path().map(|p| paths::write_user_file(&p, &text)) {
                        Some(Ok(())) => {
                            if let Some(thread) = self.hotkey_thread.take() {
                                thread.stop();
                            }
                            let (thread, _) = register_hotkeys(&text);
                            self.hotkey_thread = thread;
                            message.push_str(" Raccourcis mis à jour.");
                        }
                        Some(Err(e)) => {
                            message.push_str(&format!(" Raccourcis non mis à jour : {e}."));
                        }
                        None => {}
                    }
                }
            }
            self.reload_scenes();
            self.load_scene_editor();
            self.scene_message = Some(message);
            self.refresh_scene_editor();
            self.refresh();
        }

        /// Applique la scène enregistrée choisie (action séparée).
        fn scene_apply_selected(&mut self) {
            if let Some((Some(name), _)) = self.scene_draft.get(self.scene_selected).cloned() {
                self.apply_scene(&name);
            }
        }

        /// Montre ou retire la scène choisie du panneau rapide.
        fn scene_panel_toggled(&mut self, shown: bool) {
            let origin = self
                .scene_draft
                .get(self.scene_selected)
                .and_then(|(o, _)| o.clone());
            let Some(name) = origin else {
                self.scene_message =
                    Some("Enregistrez d'abord cette scène pour la montrer dans le panneau.".into());
                self.refresh_scene_editor();
                return;
            };
            let all: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
            match self.panel_prefs.set_scene(&all, &name, shown) {
                Ok(()) => {
                    self.store_panel_prefs();
                    self.scene_message = None;
                }
                Err(e) => self.scene_message = Some(format!("{}.", capitalize_first(&e))),
            }
            self.apply_panel_prefs();
            self.refresh_scene_editor();
        }

        /// Sections du panneau rapide cochées dans la page Scènes.
        fn panel_sections_changed(&mut self) {
            let w = &self.window;
            self.panel_prefs.noise = w.get_panel_show_noise();
            self.panel_prefs.level = w.get_panel_show_level();
            self.panel_prefs.profile = w.get_panel_show_profile();
            self.store_panel_prefs();
            self.apply_panel_prefs();
        }

        fn store_panel_prefs(&mut self) {
            if let Some(path) = paths::panel_file()
                && let Err(e) = paths::write_user_file(&path, &self.panel_prefs.to_text())
            {
                self.scene_message = Some(format!("Panneau non enregistré : {e}."));
            }
        }

        /// Contenu du panneau rapide selon les préférences.
        fn apply_panel_prefs(&self) {
            let p = &self.panel;
            p.set_show_noise(self.panel_prefs.noise);
            p.set_show_level(self.panel_prefs.level);
            p.set_show_profile(self.panel_prefs.profile);
            let all: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
            p.set_scene_names(strings_model(self.panel_prefs.shown_scenes(&all)));
        }

        /// Lit les sorties audio de Windows (lecture seule) pour la page
        /// « Connexion et audio ».
        fn read_windows_audio(&mut self) {
            let name = self
                .info
                .as_ref()
                .and_then(|i| i.name.clone())
                .or_else(|| self.device_name.clone());
            // En démonstration, le système n'est pas lu : pas de mélange entre
            // données réelles et casque simulé.
            self.windows_audio = Some(if self.demo {
                Err("mode démonstration : sorties audio du système non lues".to_string())
            } else {
                momentum_service::audio_output::read(name.as_deref())
            });
            self.windows_at = Some(momentum_service::system::clock());
            self.refresh();
        }

        fn prepare_diagnostic(&self) {
            let now = unix_now();
            let incidents: Vec<(u64, String)> = self
                .incidents
                .iter()
                .map(|(time, message)| (now.saturating_sub(*time), message.clone()))
                .collect();
            // Dossier personnel et nom d'utilisateur, masqués du diagnostic.
            let profile = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            let user = std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_default();
            let system = momentum_service::system::os_version();
            let mut secrets: Vec<&str> = vec![profile.as_str(), user.as_str()];
            secrets.extend(self.device_name.as_deref());
            secrets.extend(self.info.as_ref().and_then(|i| i.name.as_deref()));
            secrets.extend(self.paired.iter().map(|d| d.name.as_str()));
            let text = diagnostic::build(&DiagnosticInput {
                app_version: env!("CARGO_PKG_VERSION"),
                os_version: system.as_deref(),
                connected: self.state.is_some(),
                read_only: self.read_only.as_deref(),
                info: self.info.as_ref(),
                state: self.state.as_ref(),
                eq_bands: self.eq.as_ref().map(|e| e.gains.len()),
                paired_devices: self.state.is_some().then_some(self.paired.len()),
                battery_entries: self.battery_log.entries().len(),
                incidents: &incidents,
                secrets: &secrets,
            });
            self.window.set_diagnostic(text.into());
        }

        /// Une commande est partie : attente de la confirmation du casque.
        fn show_busy(&mut self) {
            self.window.set_busy(true);
            self.panel.set_busy(true);
            self.busy_timer
                .start(slint::TimerMode::SingleShot, BUSY_TIMEOUT, || {
                    with_app(App::clear_busy)
                });
        }

        /// Réponse du worker (état relu, refus, perte) ou délai écoulé.
        fn clear_busy(&mut self) {
            self.busy_timer.stop();
            self.window.set_busy(false);
            self.panel.set_busy(false);
        }

        /// Efface l'incident affiché, s'il y en a un (nouvelle commande).
        fn clear_problem(&mut self) {
            let notice = self.notice.take().is_some();
            if self.problem.take().is_some() || notice {
                self.refresh();
            }
        }

        /// Préréglages de l'app (selon l'égaliseur lu) puis personnels.
        fn all_presets(&self) -> Vec<Preset> {
            let bands = self.eq.as_ref().map_or(0, |e| e.gains.len());
            let mut all = presets::builtin_presets(bands);
            all.extend(
                self.user_presets
                    .iter()
                    .filter(|p| p.gains.len() == bands)
                    .cloned(),
            );
            all
        }

        fn save_preset(&mut self, name: &str) {
            let Some(gains) = self.eq.as_ref().map(|e| e.gains.clone()) else {
                return;
            };
            self.notice = Some(
                match presets::upsert(&mut self.user_presets, name, &gains) {
                    Ok(()) => {
                        store_user_presets(&self.user_presets);
                        self.window.set_new_preset_name("".into());
                        format!("Préréglage « {} » enregistré sur ce PC.", name.trim())
                    }
                    Err(reason) => format!("Préréglage non enregistré : {reason}."),
                },
            );
            self.refresh();
        }

        fn delete_preset(&mut self, index: usize) {
            if let Some(preset) = self.all_presets().get(index)
                && presets::remove(&mut self.user_presets, &preset.name)
            {
                store_user_presets(&self.user_presets);
                self.notice = Some(format!("Préréglage « {} » supprimé.", preset.name));
            }
            self.refresh();
        }

        /// Recopie l'état courant dans le menu, l'infobulle, l'icône et la
        /// fenêtre.
        fn refresh(&self) {
            let problem = self.problem.as_deref().or(self.read_only.as_deref());
            let writable = self.read_only.is_none();
            self.view.show(
                self.device_name.as_deref(),
                self.state.as_ref(),
                problem,
                writable,
            );
            let _ = self.tray.set_tooltip(Some(tooltip(
                self.device_name.as_deref(),
                self.state.as_ref(),
                problem,
            )));
            if let Ok(i) = icon(IconKind::from_state(self.state.as_ref())) {
                let _ = self.tray.set_icon(Some(i));
            }
            // Fenêtre : tout vient du modèle de vue Rust.
            // Le nom lu sur le casque est à jour ; celui de Windows peut
            // rester l'ancien jusqu'à la prochaine connexion.
            let name = self
                .info
                .as_ref()
                .and_then(|i| i.name.as_deref())
                .or(self.device_name.as_deref());
            let v = viewmodel::main_view(
                name,
                self.state.as_ref(),
                &self.paired,
                self.problem.as_deref(),
                self.read_only.as_deref(),
            )
            .with_notice(self.notice.as_deref());
            let sv = viewmodel::sound_view(
                self.state.as_ref(),
                self.eq.as_ref().map(|e| e.gains.as_slice()),
                &self.all_presets(),
                v.writable,
            );
            let w = &self.window;
            w.set_audio_mode(sv.audio_mode);
            w.set_modes_enabled(sv.modes_enabled);
            w.set_profile_available(sv.profile_available);
            let preset_count = i32::try_from(sv.preset_names.len()).unwrap_or(i32::MAX);
            w.set_preset_names(strings_model(sv.preset_names));
            w.set_preset_removable(ModelRc::new(VecModel::from(sv.removable)));
            w.set_preset_status(sv.preset_status.into());
            // Ne pas écraser la sélection de l'utilisateur dans la liste,
            // sauf si elle est hors limites ou qu'un préréglage correspond.
            if sv.current_preset >= 0 {
                w.set_preset_index(sv.current_preset);
            } else if w.get_preset_index() >= preset_count {
                w.set_preset_index(-1);
            }
            w.set_device_title(v.title.into());
            w.set_connection(v.connection.into());
            w.set_connected(v.connected);
            w.set_can_write(v.writable);
            w.set_battery_text(v.battery_text.into());
            w.set_battery_level(self.state.map_or(-1, |s| i32::from(s.battery)));
            w.set_device_rows(rows_model(viewmodel::device_rows(self.info.as_ref())));
            w.set_banner(v.banner.into());
            w.set_banner_is_warning(v.banner_is_warning);
            w.set_waiting(v.waiting.into());
            w.set_home_rows(rows_model(v.home));
            let st = viewmodel::settings_view(self.state.as_ref(), self.info.as_ref(), v.writable);
            w.set_smart_pause_on(st.smart_pause.on);
            w.set_smart_pause_enabled(st.smart_pause.enabled);
            w.set_auto_answer_on(st.auto_answer.on);
            w.set_auto_answer_enabled(st.auto_answer.enabled);
            w.set_comfort_on(st.comfort_call.on);
            w.set_comfort_enabled(st.comfort_call.enabled);
            w.set_battery_protection_on(st.battery_protection.on);
            w.set_battery_protection_enabled(st.battery_protection.enabled);
            w.set_auto_off(st.auto_off);
            w.set_auto_off_enabled(st.auto_off_enabled);
            w.set_prompt_mode(st.prompt_mode);
            w.set_prompts_enabled(st.prompts_enabled);
            w.set_headset_name(st.name.into());
            w.set_rename_enabled(st.rename_enabled);
            w.set_prompt_volume(st.prompt_volume as f32);
            w.set_prompt_volume_enabled(st.prompt_volume_enabled);
            w.set_settings_info(rows_model(st.info));
            let bv = viewmodel::battery_view(
                &self.battery_log,
                unix_now(),
                self.battery_span,
                self.state.map(|s| s.battery),
            );
            let p = &self.panel;
            p.set_device_title(w.get_device_title());
            p.set_battery_text(w.get_battery_text());
            p.set_connected(w.get_connected());
            p.set_can_write(w.get_can_write());
            p.set_noise_mode(w.get_noise_mode());
            p.set_level(w.get_level());
            let s = self.state.unwrap_or_default();
            p.set_profile_on(s.audio_mode == Some(AudioMode::Personalization));
            p.set_profile_available(
                self.state.is_some() && s.sound_profile == Some(SoundProfileStatus::Received),
            );
            w.set_ab_ready(self.ab_a.is_some() && self.eq.is_some());
            w.set_ab_status(
                match (&self.ab_a, self.ab_on_a) {
                    (None, _) => "Mémorisez une courbe A, modifiez-la (B), puis comparez.",
                    (Some(_), true) => "Vous écoutez A (courbe mémorisée).",
                    (Some(_), false) => "Vous écoutez B (réglages actuels) ; A est mémorisée.",
                }
                .into(),
            );
            w.set_gains_placeholder(
                self.eq
                    .as_ref()
                    .map_or(String::new(), |e| presets::format_gains_db(&e.gains))
                    .into(),
            );
            w.set_battery_path(bv.path.into());
            w.set_battery_current(bv.current.into());
            w.set_battery_estimate(bv.estimate.into());
            w.set_battery_axis_start(bv.axis_start.into());
            w.set_battery_rows(rows_model(bv.details));
            w.set_battery_span(self.battery_span as i32);
            let av = viewmodel::audio_view(&viewmodel::AudioInputs {
                device_name: name,
                state: self.state.as_ref(),
                connections: &self.paired,
                control_since: self.control_since.as_deref(),
                connections_at: self.connections_at.as_deref(),
                codec_since: self.codec_since.as_deref(),
                rate_since: self.rate_since.as_deref(),
                windows: self.windows_audio.as_ref(),
                windows_at: self.windows_at.as_deref(),
            });
            w.set_audio_findings(ModelRc::new(VecModel::from(
                av.findings
                    .into_iter()
                    .map(|(problem, text)| Finding {
                        problem,
                        text: text.into(),
                    })
                    .collect::<Vec<_>>(),
            )));
            w.set_audio_control(info_model(av.control));
            w.set_audio_windows(info_model(av.windows));
            w.set_audio_stream(info_model(av.stream));
            self.refresh_scene_editor();
            w.set_about_rows(rows_model(viewmodel::about_rows(
                env!("CARGO_PKG_VERSION"),
                self.info.as_ref(),
            )));
            w.set_connections(ModelRc::new(VecModel::from(
                v.connections
                    .into_iter()
                    .map(|c| ConnectionRow {
                        name: c.name.into(),
                        status: c.status.into(),
                        action: c.action.into(),
                        can_act: c.can_act,
                    })
                    .collect::<Vec<_>>(),
            )));
            w.set_stream(v.stream.into());
            let c = v.controls;
            w.set_noise_mode(c.noise_mode);
            w.set_level(c.level as f32);
            w.set_anti_wind(c.anti_wind);
            w.set_spatial_on(c.spatial_on);
            w.set_spatial_enabled(c.spatial_enabled);
            w.set_head_tracking_on(c.head_tracking_on);
            w.set_head_tracking_enabled(c.head_tracking_enabled);
            let multipoint = self.info.as_ref().and_then(|i| i.multipoint);
            w.set_multipoint_on(multipoint == Some(true));
            w.set_multipoint_enabled(
                self.state.is_some() && self.read_only.is_none() && multipoint.is_some(),
            );
            w.set_touch_on(c.touch_on);
            w.set_touch_enabled(c.touch_enabled);
        }

        fn on_worker(&mut self, event: WorkerEvent) {
            // Toute réponse du casque (état relu, refus, perte) termine
            // l'attente d'une commande.
            if !matches!(
                event,
                WorkerEvent::Searching | WorkerEvent::Connected { .. }
            ) {
                self.clear_busy();
            }
            match event {
                WorkerEvent::Searching => {
                    self.device_name = None;
                    self.state = None;
                    self.control_since = None;
                }
                WorkerEvent::Connected { name, read_only } => {
                    let alert = self.alerts.on_connected(&name);
                    if !self.demo {
                        toast(alert);
                    }
                    self.control_since = Some(momentum_service::system::clock());
                    self.device_name = Some(name);
                    self.state = None;
                    // Reconnecté : l'incident précédent est réglé.
                    self.problem = None;
                    self.read_only = read_only
                        .map(|model| format!("Modèle « {model} » non validé : lecture seule"));
                }
                WorkerEvent::State(s) => {
                    let alert = self.alerts.on_battery(s.battery);
                    if !self.demo {
                        toast(alert);
                    }
                    // Les données simulées n'entrent pas dans l'historique.
                    if !self.demo && self.battery_log.record(unix_now(), s.battery) {
                        store_battery_log(&self.battery_log);
                    }
                    let before = self.state.unwrap_or_default();
                    if self.state.is_none() || before.codec != s.codec {
                        self.codec_since = Some(momentum_service::system::clock());
                    }
                    if self.state.is_none() || before.sample_rate_hz != s.sample_rate_hz {
                        self.rate_since = Some(momentum_service::system::clock());
                    }
                    self.state = Some(s);
                }
                WorkerEvent::Equalizer(eq) => {
                    self.show_equalizer(&eq);
                    self.eq = Some(eq);
                }
                WorkerEvent::Exported(Ok(path)) => {
                    self.notice = Some(format!(
                        "Profil exporté dans {} (données auditives personnelles).",
                        path.display()
                    ));
                }
                WorkerEvent::Exported(Err(reason)) => {
                    self.problem = Some(format!("Export du profil impossible : {reason}"));
                }
                WorkerEvent::Connections(devices) => {
                    self.paired = devices;
                    self.connections_at = Some(momentum_service::system::clock());
                }
                WorkerEvent::Info(info) => self.info = Some(info),
                WorkerEvent::Lost(reason) => {
                    let alert = self.alerts.on_lost();
                    if !self.demo {
                        toast(alert);
                    }
                    self.control_since = None;
                    tracing::warn!(%reason, "connexion perdue");
                    if !self.demo && self.battery_log.interrupt(unix_now()) {
                        store_battery_log(&self.battery_log);
                    }
                    let message = format!("Connexion perdue : {reason}");
                    self.remember_incident(&message);
                    self.problem = Some(message);
                }
                WorkerEvent::Refused(reason) => {
                    tracing::warn!(%reason, "réglage refusé");
                    let message = format!("Réglage refusé : {reason}");
                    self.remember_incident(&message);
                    self.problem = Some(message);
                }
                WorkerEvent::SceneApplied(report) => {
                    if report.is_complete() {
                        self.notice = Some(report.summary());
                    } else {
                        tracing::warn!(details = %report.details(), "scène appliquée en partie");
                        self.remember_incident(&report.summary());
                        self.notice = None;
                        self.problem = Some(report.summary());
                    }
                }
            }
            self.refresh();
        }

        /// Affiche la fenêtre principale sur la page demandée.
        fn show_window(&self, page: Page) {
            // À la première ouverture, Slint dimensionne la fenêtre d'après la
            // page affichée : on l'ouvre sur l'Accueil (900 × 640), puis on
            // passe à la page demandée.
            self.window.set_page(page as i32);
            if let Err(e) = self.window.show() {
                tracing::warn!(%e, "ouverture de la fenêtre");
            }
        }

        fn show_equalizer(&mut self, eq: &EqState) {
            let count = eq.gains.len();
            let bands: Vec<Band> = eq
                .gains
                .iter()
                .enumerate()
                .map(|(i, &g)| Band {
                    label: band_label(i, count).into(),
                    gain: f32::from(g) / 10.0,
                })
                .collect();
            self.bands.set_vec(bands);
            self.window.set_min_gain(f32::from(eq.min_gain) / 10.0);
            self.window.set_max_gain(f32::from(eq.max_gain) / 10.0);
            self.window.set_bass_boost(eq.bass_boost);
            self.window.set_eq_status(
                format!(
                    "Égaliseur graphique — {count} bandes, de {} à {} dB",
                    eq.min_gain / 10,
                    eq.max_gain / 10
                )
                .into(),
            );
        }
    }

    fn toast(alert: Option<Alert>) {
        if let Some(a) = alert {
            momentum_service::desktop::show_toast(&a.title, &a.body);
        }
    }

    /// `%LOCALAPPDATA%\\momentum-ctl-win\\raccourcis.txt`.
    fn hotkeys_path() -> Option<PathBuf> {
        paths::hotkeys_file()
    }

    /// Lit `raccourcis.txt`, créé avec les valeurs par défaut s'il manque.
    fn read_hotkey_config() -> String {
        paths::load_hotkey_config()
    }

    /// Enregistre les raccourcis du texte. Renvoie le thread et un message
    /// si une ligne est invalide ou un raccourci déjà pris.
    fn register_hotkeys(text: &str) -> (Option<HotkeyThread>, Option<String>) {
        let (keys, mut errors) = hotkeys::parse_config(text);
        let (thread, refused) = momentum_service::desktop::spawn_hotkeys(keys, |action| {
            let _ = slint::invoke_from_event_loop(move || with_app(|app| app.on_hotkey(action)));
        });
        errors.extend(refused);
        if errors.is_empty() {
            (thread, None)
        } else {
            tracing::warn!(?errors, "raccourcis clavier");
            (
                thread,
                Some(format!("Raccourcis clavier : {}", errors.join(" ; "))),
            )
        }
    }

    fn strings_model(items: Vec<String>) -> ModelRc<slint::SharedString> {
        ModelRc::new(VecModel::from(
            items
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        ))
    }

    fn store_user_presets(list: &[Preset]) {
        let Some(path) = paths::presets_file() else {
            return;
        };
        if let Err(e) = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, presets::serialize_user_presets(list)))
        {
            tracing::warn!(%e, "enregistrement des préréglages");
        }
    }

    /// Fichier d'export du profil : Documents, sinon le dossier de l'appli.
    fn export_path() -> PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let name = format!(
            "momentum-profil-{}.json",
            momentum_core::profile::file_stamp(now)
        );
        let documents = std::env::var_os("USERPROFILE")
            .map(|h| PathBuf::from(h).join("Documents"))
            .filter(|d| d.is_dir());
        documents
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("momentum-ctl-win"))
            })
            .unwrap_or_default()
            .join(name)
    }

    /// Convertit des lignes du modèle de vue en modèle Slint.
    /// Première lettre en capitale.
    fn capitalize_first(text: &str) -> String {
        let mut chars = text.chars();
        chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect())
            .unwrap_or_default()
    }

    fn info_model(rows: Vec<viewmodel::InfoRow>) -> ModelRc<InfoRow> {
        ModelRc::new(VecModel::from(
            rows.into_iter()
                .map(|r| InfoRow {
                    label: r.label.into(),
                    value: r.value.into(),
                    source: r.source.into(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    fn rows_model(rows: Vec<viewmodel::Row>) -> ModelRc<Row> {
        ModelRc::new(VecModel::from(
            rows.into_iter()
                .map(|r| Row {
                    label: r.label.into(),
                    value: r.value.into(),
                })
                .collect::<Vec<_>>(),
        ))
    }

    /// Programme qui ouvre `scenes.txt`.
    #[cfg(windows)]
    const TEXT_EDITOR: &str = "notepad.exe";
    #[cfg(target_os = "linux")]
    const TEXT_EDITOR: &str = "xdg-open";

    /// Commande des paramètres de son du système.
    #[cfg(windows)]
    const SOUND_SETTINGS: [&str; 2] = ["explorer.exe", "ms-settings:sound"];
    #[cfg(target_os = "linux")]
    const SOUND_SETTINGS: [&str; 2] = ["systemsettings", "kcm_pulseaudio"];

    /// Taille au-delà de laquelle le journal est archivé (au démarrage et
    /// en cours de route).
    const LOG_MAX_BYTES: u64 = 1024 * 1024;

    /// `momentum-tray.log` dans le dossier de données (`%LOCALAPPDATA%` ou
    /// `~/.local/share`, puis `momentum-ctl-win`) ; au-delà de 1 Mo, il
    /// devient `momentum-tray.old.log` et un journal neuf commence.
    fn open_log_file() -> Option<(PathBuf, RotatingFile)> {
        let dir = paths::data_dir()?;
        fs::create_dir_all(&dir).ok()?;
        let path = dir.join("momentum-tray.log");
        let file = RotatingFile::open(
            path.clone(),
            dir.join("momentum-tray.old.log"),
            LOG_MAX_BYTES,
        )
        .ok()?;
        Some((path, file))
    }

    /// Vrai une fois « Quitter » choisi : distingue un arrêt voulu d'une fin
    /// inattendue de la boucle d'événements dans le journal.
    static QUIT_REQUESTED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    /// Écrit tout plantage dans le journal avant l'arrêt (en release,
    /// `panic = abort` ne laisse sinon aucune trace). Un arrêt forcé de
    /// l'extérieur (gestionnaire des tâches, fin de session) n'en laisse
    /// pas : l'absence de ligne d'arrêt le signale alors.
    fn install_panic_logging() {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            tracing::error!(
                thread = thread.name().unwrap_or("sans nom"),
                %info,
                "plantage de momentum-tray"
            );
            default(info);
        }));
    }

    fn init_logging() {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "momentum_core=info,momentum_service=info,momentum_tray=info".into()
        });
        let builder = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false);
        match open_log_file() {
            Some((path, file)) => {
                builder
                    .with_writer(std::io::stderr.and(Mutex::new(file)))
                    .init();
                tracing::info!(journal = %path.display(), "démarrage de momentum-tray");
            }
            None => builder.with_writer(std::io::stderr).init(),
        }
    }

    /// Captures de vérification : chaque étape règle taille, page et thème,
    /// puis l'étape suivante enregistre le rendu. Le minuteur renvoyé doit
    /// rester en vie jusqu'à la fin.
    fn start_capture(dir: PathBuf) -> anyhow::Result<slint::Timer> {
        fs::create_dir_all(&dir).context("dossier des captures")?;
        with_app(|app| app.show_window(Page::Home));
        let step = std::cell::Cell::new(0usize);
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(1500),
            move || {
                let i = step.get();
                step.set(i + 1);
                with_app(|app| {
                    let w = &app.window;
                    if let Some(shot) = i.checked_sub(1).and_then(|p| crate::capture::SHOTS.get(p))
                    {
                        match w.window().take_snapshot() {
                            Ok(buf) => {
                                let path = dir.join(format!("{}.png", shot.name));
                                match crate::capture::write_png(
                                    &path,
                                    buf.width(),
                                    buf.height(),
                                    buf.as_bytes(),
                                ) {
                                    Ok(()) => tracing::info!(fichier = %path.display(), "capture"),
                                    Err(e) => tracing::warn!(%e, "capture"),
                                }
                            }
                            Err(e) => tracing::warn!(%e, "capture de la fenêtre"),
                        }
                    }
                    match crate::capture::SHOTS.get(i) {
                        Some(shot) => {
                            w.set_dark_theme(shot.dark);
                            app.panel.global::<Theme>().set_dark(shot.dark);
                            w.set_page(shot.page);
                            w.window()
                                .set_size(slint::LogicalSize::new(shot.width, shot.height));
                        }
                        None => {
                            QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
                            let _ = slint::quit_event_loop();
                        }
                    }
                });
            },
        );
        Ok(timer)
    }

    /// Logo d'OpenHeadset Control (barre de titre, barre des tâches) ; à
    /// défaut, l'icône dessinée du casque.
    fn window_icon() -> Image {
        const LOGO: &[u8] = include_bytes!("../../../assets/logo/openheadset.svg");
        if let Ok(logo) = Image::load_from_svg_data(LOGO) {
            return logo;
        }
        let pixels = icon_rgba(IconKind::Anc, ICON_SIZE);
        Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            &pixels, ICON_SIZE, ICON_SIZE,
        ))
    }

    pub fn main() -> anyhow::Result<()> {
        init_logging();
        install_panic_logging();

        // « --demo » : données simulées, sans casque ; « --capture <dossier> »
        // (démonstration) : rendus PNG à plusieurs tailles, puis arrêt.
        let capture_dir = std::env::args()
            .skip_while(|a| a != "--capture")
            .nth(1)
            .map(PathBuf::from);
        let demo = capture_dir.is_some() || std::env::args().any(|a| a == "--demo");
        if demo {
            tracing::info!("mode démonstration : données simulées, aucun casque");
        }

        // Instance unique : une deuxième instance demande simplement à la
        // première d'ouvrir sa fenêtre, puis s'arrête. La démonstration
        // peut tourner à côté de l'icône réelle.
        let show_request = if demo {
            None
        } else {
            match acquire_instance().context("verrou d'instance unique")? {
                Instance::First(request) => Some(request),
                Instance::AlreadyRunning => {
                    tracing::info!("déjà lancé : ouverture de la fenêtre de l'instance existante");
                    return Ok(());
                }
            }
        };

        // La fenêtre est créée tout de suite (cachée) : cela initialise aussi
        // la boucle d'événements Slint.
        let window = MainWindow::new().context("création de la fenêtre Slint")?;
        let bands = Rc::new(VecModel::<Band>::default());
        window.set_bands(ModelRc::from(bands.clone()));
        window.set_app_icon(window_icon());
        window.set_auto_off_labels(strings_model(
            viewmodel::AUTO_OFF_LABELS
                .iter()
                .map(|m| m.to_string())
                .collect(),
        ));
        window.set_prompt_modes(strings_model(
            viewmodel::PROMPT_MODES
                .iter()
                .map(|m| m.to_string())
                .collect(),
        ));
        window.set_noise_modes(strings_model(
            viewmodel::NOISE_MODES
                .iter()
                .map(|m| m.to_string())
                .collect(),
        ));
        window.set_battery_spans(strings_model(
            viewmodel::BATTERY_SPANS
                .iter()
                .map(|(label, _)| label.to_string())
                .collect(),
        ));
        window.set_audio_modes(strings_model(
            viewmodel::AUDIO_MODES
                .iter()
                .map(|m| m.to_string())
                .collect(),
        ));

        let dark = load_dark_theme();
        window.set_dark_theme(dark);
        window.set_demo(demo);
        window.set_hotkeys_supported(cfg!(windows));
        window.set_system_name(SYSTEM_NAME.into());
        window.on_theme_changed(|dark| {
            store_dark_theme(dark);
            with_app(|app| app.panel.global::<Theme>().set_dark(dark));
        });

        let panel = QuickPanel::new().context("création du panneau rapide")?;
        panel.global::<Theme>().set_dark(dark);
        panel.set_noise_modes(strings_model(
            viewmodel::NOISE_MODES
                .iter()
                .map(|m| m.to_string())
                .collect(),
        ));

        // Sous Linux, l'icône et son menu passent par GTK : il doit être
        // initialisé sur ce thread, et ses événements pompés régulièrement
        // (la boucle d'événements est celle de Slint).
        #[cfg(target_os = "linux")]
        let gtk_pump = {
            gtk::init().context("initialisation de GTK (icône de la barre système)")?;
            let timer = slint::Timer::default();
            timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(50),
                || {
                    while gtk::events_pending() {
                        gtk::main_iteration_do(false);
                    }
                },
            );
            timer
        };

        let view = MenuView::build().context("construction du menu")?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(view.menu.clone()))
            // Clic gauche : la fenêtre ; clic droit : le menu.
            .with_menu_on_left_click(false)
            .with_tooltip(tooltip(None, None, None))
            .with_icon(icon(IconKind::Disconnected)?)
            .build()
            .context("création de l'icône de la barre système")?;
        view.show(None, None, None, true);

        // Clic gauche : panneau rapide près de l'icône ; clic droit : le menu.
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                position,
                ..
            } = event
            {
                let (x, y) = (position.x as i32, position.y as i32);
                let _ =
                    slint::invoke_from_event_loop(move || with_app(|app| app.toggle_panel(x, y)));
            }
        }));

        // Le gestionnaire de menu peut être appelé hors de la boucle Slint :
        // on repasse toujours par elle.
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let _ = slint::invoke_from_event_loop(move || with_app(|app| app.on_menu(event)));
        }));

        let (raw_commands, commands_rx) = mpsc::channel::<WorkerCommand>();
        let commands = Commands(raw_commands);

        // Rappels de la fenêtre → commandes pour le casque.
        let tx = commands.clone();
        window.on_band_released(move |band, db| {
            // dB → dixièmes de dB, bornés par la plage du casque côté bibliothèque.
            let tenths = (db * 10.0)
                .round()
                .clamp(f32::from(i8::MIN), f32::from(i8::MAX)) as i8;
            if let Ok(band) = u8::try_from(band) {
                let _ = tx.send(WorkerCommand::SetEqBand(band, tenths));
                with_app(App::clear_problem);
            }
        });
        let tx = commands.clone();
        window.on_bass_boost_toggled(move |on| {
            let _ = tx.send(WorkerCommand::SetBassBoost(on));
            with_app(App::clear_problem);
        });
        // Accueil : chaque action devient une commande pour le casque. La
        // fenêtre est ensuite resynchronisée sur l'état réel (relu ou notifié).
        let send = {
            let tx = commands.clone();
            move |command: WorkerCommand| {
                let _ = tx.send(command);
                with_app(|app| {
                    app.clear_problem();
                    app.refresh();
                });
            }
        };
        let s = send.clone();
        window.on_noise_mode_clicked(move |i| {
            let mode = match i {
                0 => NoiseModeRequest::Adaptive,
                1 => NoiseModeRequest::Custom,
                _ => NoiseModeRequest::Off,
            };
            s(WorkerCommand::SetMode(mode));
        });
        let s = send.clone();
        window.on_level_released(move |v| {
            s(WorkerCommand::SetLevel(v.round().clamp(0.0, 100.0) as u8));
        });
        let s = send.clone();
        window.on_anti_wind_toggled(move |on| s(WorkerCommand::SetAntiWind(on)));
        let s = send.clone();
        window.on_spatial_toggled(move |on| s(WorkerCommand::SetSpatial(on)));
        let s = send.clone();
        window.on_head_tracking_toggled(move |on| s(WorkerCommand::SetHeadTracking(on)));
        let s = send.clone();
        window.on_multipoint_changed(move |on| s(WorkerCommand::SetMultipoint(on)));
        let s = send.clone();
        window.on_touch_toggled(move |on| s(WorkerCommand::SetTouchControls(on)));
        // Réglages.
        let s = send.clone();
        window.on_smart_pause_toggled(move |on| s(WorkerCommand::SetSmartPause(on)));
        let s = send.clone();
        window.on_auto_answer_toggled(move |on| s(WorkerCommand::SetAutoAnswer(on)));
        let s = send.clone();
        window.on_comfort_toggled(move |on| s(WorkerCommand::SetComfortCall(on)));
        let s = send.clone();
        window.on_battery_protection_toggled(move |on| s(WorkerCommand::SetBatteryProtection(on)));
        let s = send.clone();
        window.on_auto_off_selected(move |i| {
            if let Some(&secs) = usize::try_from(i)
                .ok()
                .and_then(|i| AUTO_OFF_CHOICES.get(i))
            {
                s(WorkerCommand::SetAutoOff(secs));
            }
        });
        let s = send.clone();
        window.on_connection_clicked(move |index, name| {
            // L'appareil affiché à cette ligne, s'il porte toujours ce nom ;
            // le worker le retrouve ensuite par son adresse.
            let device = APP.with(|app| {
                app.borrow().as_ref().and_then(|a| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|i| a.paired.get(i))
                        .filter(|d| d.name == name.as_str())
                        .cloned()
                })
            });
            if let Some(device) = device {
                let connect = !device.connected;
                s(WorkerCommand::ToggleConnection { device, connect });
            }
        });
        let s = send.clone();
        window.on_rename(move |name| {
            s(WorkerCommand::SetName(name.trim().to_string()));
        });
        let s = send.clone();
        window.on_prompt_volume_released(move |pct| {
            let pct = pct.round().clamp(0.0, 100.0) as u8;
            s(WorkerCommand::SetPromptVolume(
                momentum_core::device::prompt_volume_from_percent(pct),
            ));
        });
        let s = send.clone();
        window.on_prompt_mode_clicked(move |i| {
            s(WorkerCommand::SetPromptMode(if i == 0 {
                PromptMode::TonesOnly
            } else {
                PromptMode::TonesAndVoice
            }))
        });
        // Mon son.
        let s = send.clone();
        window.on_audio_mode_clicked(move |i| {
            s(match i {
                1 => WorkerCommand::SetSpeechClarity(true),
                2 => WorkerCommand::SetSoundProfile(true),
                _ => WorkerCommand::SetSpeechClarity(false),
            })
        });
        let s = send.clone();
        window.on_export_profile(move || s(WorkerCommand::ExportProfile(export_path())));
        let s = send;
        window.on_apply_preset(move |i| {
            let gains = APP.with(|app| {
                app.borrow()
                    .as_ref()
                    .and_then(|a| a.all_presets().get(i as usize).map(|p| p.gains.clone()))
            });
            if let Some(gains) = gains {
                s(WorkerCommand::SetEqGains(gains));
            }
        });
        window.on_save_preset(move |name| with_app(|app| app.save_preset(&name)));
        window.on_scene_clicked(move |name| with_app(|app| app.apply_scene(&name)));
        window.on_battery_span_selected(move |i| {
            with_app(|app| {
                app.battery_span = usize::try_from(i).unwrap_or(0);
                app.refresh();
            })
        });
        window.on_export_battery(move || with_app(App::export_battery));
        window.on_clear_battery(move || with_app(App::clear_battery));
        window.on_prepare_diagnostic(move || with_app(|app| app.prepare_diagnostic()));
        window.on_audio_page_shown(move || with_app(App::read_windows_audio));
        window.set_scene_switch_choices(strings_model(
            viewmodel::SCENE_SWITCH_CHOICES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ));
        window.set_scene_sound_choices(strings_model(
            viewmodel::SCENE_SOUND_CHOICES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ));
        window.set_scene_noise_choices(strings_model(
            viewmodel::SCENE_NOISE_CHOICES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        ));
        window.on_scenes_page_shown(move || {
            with_app(|app| {
                // Pas de rechargement par-dessus des modifications en cours.
                if !app.scene_dirty {
                    app.load_scene_editor();
                }
            });
        });
        window.on_scene_select(move |i| {
            with_app(|app| app.scene_select(usize::try_from(i).unwrap_or(0)));
        });
        window.on_scene_form_changed(move || with_app(App::scene_form_changed));
        window.on_scene_new(move || with_app(App::scene_new));
        window.on_scene_duplicate(move || with_app(App::scene_duplicate));
        window.on_scene_delete(move || with_app(App::scene_delete));
        window.on_scene_move(move |delta| with_app(|app| app.scene_move(delta)));
        window.on_scene_save(move || with_app(App::scene_save));
        window.on_scene_revert(move || with_app(App::load_scene_editor));
        window.on_scene_apply(move || with_app(App::scene_apply_selected));
        window.on_scene_panel_toggled(move |on| with_app(|app| app.scene_panel_toggled(on)));
        window.on_panel_sections_changed(move || with_app(App::panel_sections_changed));
        window.on_open_scenes_file(move || {
            if let Some(path) = paths::scenes_file()
                && let Err(e) = std::process::Command::new(TEXT_EDITOR).arg(&path).spawn()
            {
                tracing::warn!(%e, "ouverture de scenes.txt");
            }
        });
        window.on_open_sound_settings(move || {
            // Ouvre seulement la page du système : rien n'y est changé.
            if let Err(e) = std::process::Command::new(SOUND_SETTINGS[0])
                .args(&SOUND_SETTINGS[1..])
                .spawn()
            {
                tracing::warn!(%e, "ouverture des paramètres de son");
            }
        });
        // Panneau rapide : mêmes commandes que l'Accueil.
        let tx = commands.clone();
        panel.on_noise_mode_clicked(move |i| {
            let mode = match i {
                0 => NoiseModeRequest::Adaptive,
                1 => NoiseModeRequest::Custom,
                _ => NoiseModeRequest::Off,
            };
            let _ = tx.send(WorkerCommand::SetMode(mode));
            with_app(App::clear_problem);
        });
        let tx = commands.clone();
        panel.on_level_released(move |v| {
            let _ = tx.send(WorkerCommand::SetLevel(v.round().clamp(0.0, 100.0) as u8));
            with_app(App::clear_problem);
        });
        let tx = commands.clone();
        panel.on_profile_toggled(move |on| {
            let _ = tx.send(WorkerCommand::SetSoundProfile(on));
            with_app(App::clear_problem);
        });
        panel.on_scene_clicked(move |name| with_app(|app| app.apply_scene(&name)));
        panel.on_open_window(move || {
            with_app(|app| {
                app.hide_panel();
                app.show_window(Page::Home);
            })
        });
        panel.on_close_panel(move || with_app(App::hide_panel));
        window.on_hotkey_edited(move |i, text| {
            with_app(|app| {
                if let Some(entry) = usize::try_from(i)
                    .ok()
                    .and_then(|i| app.hotkey_edits.get_mut(i))
                {
                    entry.1 = text.to_string();
                }
            })
        });
        window.on_save_hotkeys(move || with_app(App::save_hotkeys));
        window.on_apply_gains_text(move |text| with_app(|app| app.apply_gains_text(&text)));
        window.on_export_curve(move || with_app(App::export_curve));
        window.on_import_curve(move || with_app(App::import_curve));
        window.on_remember_a(move || with_app(App::remember_a));
        window.on_toggle_ab(move || with_app(App::toggle_ab));
        window.on_reload_scenes(move || {
            with_app(|app| {
                app.reload_scenes();
                app.refresh();
            })
        });
        window.on_delete_preset(move |i| with_app(|app| app.delete_preset(i as usize)));
        let tx = commands.clone();
        window.on_reset_flat(move || {
            let _ = tx.send(WorkerCommand::SetEqFlat);
            with_app(App::clear_problem);
        });
        std::thread::Builder::new()
            .name("casque".into())
            .spawn(move || {
                let notify = |event: WorkerEvent| {
                    let _ =
                        slint::invoke_from_event_loop(move || with_app(|app| app.on_worker(event)));
                };
                if demo {
                    crate::demo::run(commands_rx, notify);
                } else {
                    worker::run(commands_rx, notify);
                }
            })
            .context("lancement du thread casque")?;

        // Pas de raccourcis globaux en démonstration (ceux de l'icône réelle
        // resteraient pris).
        let (hotkey_thread, hotkey_notice) = if demo {
            (None, None)
        } else {
            register_hotkeys(&read_hotkey_config())
        };
        APP.with(|app| {
            *app.borrow_mut() = Some(App {
                view,
                tray,
                window,
                bands,
                commands,
                demo,
                busy_timer: slint::Timer::default(),
                device_name: None,
                state: None,
                paired: Vec::new(),
                eq: None,
                info: None,
                user_presets: paths::load_user_presets(),
                notice: hotkey_notice,
                alerts: AlertTracker::new(),
                problem: None,
                read_only: None,
                scenes: Vec::new(),
                scene_items: Vec::new(),
                battery_log: load_battery_log(),
                battery_span: 0,
                incidents: std::collections::VecDeque::new(),
                hotkey_thread,
                hotkey_edits: Vec::new(),
                ab_a: None,
                ab_b: None,
                ab_on_a: false,
                panel,
                panel_visible: false,
                panel_focus_seen: false,
                panel_closed_by_focus: None,
                control_since: None,
                connections_at: None,
                codec_since: None,
                rate_since: None,
                windows_audio: None,
                windows_at: None,
                scene_draft: Vec::new(),
                scene_selected: 0,
                scene_dirty: false,
                scene_loaded_text: None,
                scene_file_errors: Vec::new(),
                scene_form_error: None,
                scene_message: None,
                scene_eq_choices: Vec::new(),
                panel_prefs: paths::load_panel_prefs(),
                panel_timer: slint::Timer::default(),
            });
        });
        with_app(App::reload_scenes);
        with_app(|app| {
            app.panel_timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(250),
                || with_app(App::watch_panel),
            );
        });
        // Canal local pour `ctl` (le worker reste seul maître du casque).
        #[cfg(windows)]
        if !demo
            && let Some(commands) = APP.with(|app| app.borrow().as_ref().map(|a| a.commands.raw()))
        {
            momentum_service::ipc_server::spawn(commands);
        }

        momentum_service::desktop::register_notifications();
        if std::env::args().any(|a| a == "--test-notification") {
            momentum_service::desktop::show_toast(
                "MOMENTUM 5",
                "Les notifications de l'icône fonctionnent.",
            );
        }

        // Demandes d'ouverture venues d'une deuxième instance.
        if let Some(show_request) = show_request {
            std::thread::Builder::new()
                .name("instance".into())
                .spawn(move || {
                    while show_request.wait() {
                        let _ = slint::invoke_from_event_loop(|| {
                            with_app(|app| app.show_window(Page::Home))
                        });
                    }
                })
                .context("lancement du thread d'instance")?;
        }

        // « --fenetre » : ouvre la fenêtre dès le lancement ; « --egaliseur »
        // l'ouvre sur « Mon son » ; « --panneau » ouvre le panneau rapide
        // près du pointeur.
        if std::env::args().any(|a| a == "--panneau") {
            let (x, y) = crate::window::cursor_position();
            with_app(|app| app.toggle_panel(x, y));
        } else if std::env::args().any(|a| a == "--egaliseur") {
            with_app(|app| app.show_window(Page::Sound));
        } else if std::env::args().any(|a| a == "--fenetre") {
            with_app(|app| app.show_window(Page::Home));
        }
        with_app(|app| app.refresh());
        let capture_timer = match capture_dir {
            Some(dir) => Some(start_capture(dir)?),
            None => None,
        };

        // Fermer la fenêtre la cache seulement : l'application continue dans
        // la barre système jusqu'à « Quitter ».
        let ended = slint::run_event_loop_until_quit().context("boucle d'événements Slint");
        if QUIT_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("arrêt de momentum-tray");
        } else {
            tracing::warn!(
                ?ended,
                "fin inattendue de la boucle d'événements (sans « Quitter »)"
            );
        }
        ended?;
        drop(capture_timer);
        #[cfg(target_os = "linux")]
        drop(gtk_pump);
        APP.with(|app| app.borrow_mut().take());
        Ok(())
    }
}
