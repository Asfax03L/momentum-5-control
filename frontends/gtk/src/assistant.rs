//! Assistant de premier lancement : une étape à la fois (textes communs avec
//! WinUI et le client KDE, `momentum_core::assistant`). Montré une fois, puis
//! rouvert depuis « À propos ». Fermé, de quelque façon que ce soit : noté vu.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use momentum_client::demo;
use momentum_core::assistant::{self, Platform, Step, StepAction};
use momentum_core::i18n::{tr, trf};

pub struct Assistant {
    dialog: adw::Dialog,
    steps: Vec<Step>,
    step: Cell<usize>,
    open: Cell<bool>,
    connected: Cell<bool>,
    progress: gtk::Label,
    text: gtk::Label,
    bluetooth: gtk::Button,
    connection: gtk::Label,
    previous: gtk::Button,
    next: gtk::Button,
}

/// Ouvre les réglages Bluetooth du bureau : ceux de Plasma d'abord sous KDE,
/// ceux de GNOME d'abord ailleurs.
fn open_bluetooth_settings() {
    let gnome = ("gnome-control-center", "bluetooth");
    let plasma = ("systemsettings", "kcm_bluetooth");
    let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.contains("KDE"));
    let order = if kde {
        [plasma, gnome]
    } else {
        [gnome, plasma]
    };
    for (program, panel) in order {
        if std::process::Command::new(program)
            .arg(panel)
            .spawn()
            .is_ok()
        {
            return;
        }
    }
    tracing::warn!("réglages Bluetooth du bureau introuvables");
}

impl Assistant {
    pub fn new() -> Rc<Self> {
        let label = || {
            let l = gtk::Label::new(None);
            l.set_wrap(true);
            l.set_xalign(0.0);
            l
        };
        let progress = label();
        progress.add_css_class("dim-label");
        progress.add_css_class("caption");
        let text = label();
        let bluetooth = gtk::Button::with_label(tr("Ouvrir les paramètres Bluetooth"));
        bluetooth.set_halign(gtk::Align::Start);
        bluetooth.connect_clicked(|_| open_bluetooth_settings());
        let connection = label();
        connection.add_css_class("heading");

        let skip = gtk::Button::with_label(tr("Passer l'assistant"));
        let previous = gtk::Button::with_label(tr("Précédent"));
        let next = gtk::Button::with_label(tr("Suivant"));
        next.add_css_class("suggested-action");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        buttons.set_halign(gtk::Align::End);
        buttons.set_margin_top(6);
        buttons.append(&skip);
        buttons.append(&previous);
        buttons.append(&next);

        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        column.set_margin_top(6);
        column.set_margin_bottom(18);
        column.set_margin_start(18);
        column.set_margin_end(18);
        column.append(&progress);
        column.append(&text);
        column.append(&bluetooth);
        column.append(&connection);
        column.append(&buttons);

        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&column));
        let dialog = adw::Dialog::builder()
            .content_width(520)
            .child(&view)
            .build();

        let this = Rc::new(Self {
            dialog,
            steps: assistant::steps(Platform::Linux),
            step: Cell::new(0),
            open: Cell::new(false),
            connected: Cell::new(false),
            progress,
            text,
            bluetooth,
            connection,
            previous,
            next,
        });
        {
            let this = this.clone();
            skip.connect_clicked(move |_| {
                this.dialog.close();
            });
        }
        {
            let assistant = this.clone();
            this.previous.connect_clicked(move |_| {
                assistant.step.set(assistant.step.get().saturating_sub(1));
                assistant.show_step();
            });
        }
        {
            let assistant = this.clone();
            this.next.connect_clicked(move |_| assistant.advance());
        }
        {
            let assistant = this.clone();
            this.dialog.connect_closed(move |_| {
                assistant.open.set(false);
                if !demo::requested()
                    && let Err(e) = assistant::mark_seen()
                {
                    tracing::warn!(%e, "assistant : état non enregistré");
                }
            });
        }
        this
    }

    /// Étape suivante, ou fermeture à la dernière.
    fn advance(&self) {
        if self.step.get() + 1 >= self.steps.len() {
            self.dialog.close();
        } else {
            self.step.set(self.step.get() + 1);
            self.show_step();
        }
    }

    fn show_step(&self) {
        let index = self.step.get();
        let Some(step) = self.steps.get(index) else {
            return;
        };
        self.dialog.set_title(step.title);
        self.progress.set_text(&trf(
            "Premier lancement — étape {} sur {}",
            &[&(index + 1), &self.steps.len()],
        ));
        self.text.set_text(step.text);
        self.bluetooth
            .set_visible(step.action == Some(StepAction::BluetoothSettings));
        self.connection.set_visible(step.shows_connection);
        self.connection
            .set_text(assistant::connection_text(self.connected.get()));
        self.previous.set_sensitive(index > 0);
        self.next.set_label(if index + 1 == self.steps.len() {
            tr("Terminer")
        } else {
            tr("Suivant")
        });
    }

    /// Ouvre l'assistant à sa première étape.
    pub fn present(&self, parent: &impl IsA<gtk::Widget>) {
        self.step.set(0);
        self.show_step();
        self.open.set(true);
        self.dialog.present(Some(parent));
    }

    /// État du casque, montré à l'étape d'appairage.
    pub fn set_connected(&self, connected: bool) {
        if self.connected.replace(connected) != connected && self.open.get() {
            self.show_step();
        }
    }

    /// Essai de l'assistant : fenêtre ouverte, nombre d'étapes, étape
    /// affichée (titre, texte), appui sur « Suivant ».
    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    pub fn count(&self) -> usize {
        self.steps.len()
    }

    pub fn shown(&self) -> (String, String) {
        (
            self.dialog.title().to_string(),
            self.text.text().to_string(),
        )
    }

    pub fn press_next(&self) {
        self.next.emit_clicked();
    }
}
