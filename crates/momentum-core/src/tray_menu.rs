//! Menu de l'icône de la zone de notification (clic droit), en Rust pur :
//! même contenu pour WinUI (`tray-icon`) et le client KDE (StatusNotifierItem sur zbus), repris
//! de l'ancienne icône Slint.
//!
//! Chaque interface dessine les [`MenuEntry`] avec sa bibliothèque, puis
//! renvoie l'[`MenuAction`] choisie ; la traduction en commande pour le
//! service est dans `momentum_client::menu`. Une coche ne change qu'une
//! fois le nouvel état relu sur le casque ; un réglage non lu reste grisé.

use crate::device::SoundProfileStatus;
use crate::device::{AUTO_OFF_CHOICES, AudioMode, HeadsetState, NoiseModeRequest, PromptMode};
use crate::i18n::{tr, trf};
use crate::protocol::messages::M5NoiseMode;
use crate::ui::{LEVEL_PRESETS, audio_summary};

/// Choix possibles dans le menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    ReconnectHeadset,
    OpenWindow,
    /// Fenêtre ouverte sur la page Égaliseur.
    OpenEqualizer,
    Mode(NoiseModeRequest),
    Level(u8),
    AntiWind,
    Spatial,
    SpeechClarity,
    SoundProfile,
    TouchControls,
    BatteryProtection,
    SmartPause,
    AutoAnswer,
    ComfortCall,
    AutoOff(u16),
    Prompts(PromptMode),
    /// Applique la scène nommée.
    Scene(String),
    ReloadScenes,
    Autostart,
    Quit,
}

/// Entrée du menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// Ligne d'information, non cliquable.
    Info(String),
    Item {
        label: String,
        action: MenuAction,
        enabled: bool,
    },
    Check {
        label: String,
        action: MenuAction,
        checked: bool,
        enabled: bool,
    },
    Submenu {
        label: String,
        enabled: bool,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

/// Ce que le menu montre, hors état du casque.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuContext<'a> {
    /// Nom du casque, s'il est connu.
    pub name: Option<&'a str>,
    /// Incident à afficher en tête (connexion perdue, réglage refusé…).
    pub problem: Option<&'a str>,
    /// Écritures permises (modèle validé, service joignable).
    pub writable: bool,
    /// Service joignable et aucune demande en cours (indépendant du casque).
    pub can_reconnect: bool,
    /// Noms des scènes de `scenes.txt`.
    pub scenes: &'a [String],
    /// Lancement à l'ouverture de session activé.
    pub autostart: bool,
    /// « Lancer au démarrage de Windows » ou « … à l'ouverture de session ».
    pub autostart_label: &'a str,
}

fn item(label: &str, action: MenuAction, enabled: bool) -> MenuEntry {
    MenuEntry::Item {
        label: label.to_string(),
        action,
        enabled,
    }
}

fn check(label: &str, action: MenuAction, checked: bool, enabled: bool) -> MenuEntry {
    MenuEntry::Check {
        label: label.to_string(),
        action,
        checked,
        enabled,
    }
}

/// Réglage booléen lu (ou non) : coché s'il est actif, grisé s'il n'a pas
/// été lu.
fn toggle(label: &str, action: MenuAction, value: Option<bool>, writable: bool) -> MenuEntry {
    check(
        label,
        action,
        value == Some(true),
        writable && value.is_some(),
    )
}

/// Libellé d'une durée d'extinction automatique.
fn auto_off_label(seconds: u16) -> String {
    if seconds == 0 {
        tr("Jamais").to_string()
    } else {
        trf("{} min", &[&(seconds / 60)])
    }
}

/// Contenu du menu pour l'état courant (`None` : casque absent).
pub fn tray_menu(state: Option<&HeadsetState>, ctx: &MenuContext) -> Vec<MenuEntry> {
    let writable = ctx.writable && state.is_some();
    let s = state.copied().unwrap_or_default();
    let mode = state.map(|s| s.noise.mode());
    let status = match (ctx.name, state) {
        (Some(n), Some(s)) => trf("{} — batterie {} %", &[&n, &s.battery]),
        (Some(n), None) => trf("{} — connexion…", &[&n]),
        _ => tr("Casque déconnecté — recherche…").to_string(),
    };

    let mut menu = vec![
        item(
            tr("Ouvrir OpenHeadset Control"),
            MenuAction::OpenWindow,
            true,
        ),
        MenuEntry::Info(status),
    ];
    if let Some(problem) = ctx.problem {
        menu.push(MenuEntry::Info(format!("⚠ {problem}")));
    }
    if state.is_none() {
        menu.push(item(
            tr("Reconnecter le casque"),
            MenuAction::ReconnectHeadset,
            ctx.can_reconnect,
        ));
    }
    menu.push(MenuEntry::Separator);
    menu.extend([
        check(
            tr("ANC adaptatif"),
            MenuAction::Mode(NoiseModeRequest::Adaptive),
            matches!(mode, Some(M5NoiseMode::Adaptive)),
            writable,
        ),
        check(
            tr("Personnalisé"),
            MenuAction::Mode(NoiseModeRequest::Custom),
            matches!(mode, Some(M5NoiseMode::Custom { .. })),
            writable,
        ),
        check(
            tr("Désactivé"),
            MenuAction::Mode(NoiseModeRequest::Off),
            matches!(mode, Some(M5NoiseMode::Off)),
            writable,
        ),
        MenuEntry::Submenu {
            label: tr("Curseur ANC ↔ transparence").to_string(),
            enabled: writable,
            entries: LEVEL_PRESETS
                .iter()
                .map(|&(level, name)| {
                    check(
                        &format!("{name} ({level} %)"),
                        MenuAction::Level(level),
                        matches!(mode, Some(M5NoiseMode::Custom { level: l }) if l == level),
                        writable,
                    )
                })
                .collect(),
        },
        MenuEntry::Separator,
        check(
            tr("Anti-vent"),
            MenuAction::AntiWind,
            state.is_some_and(|s| s.noise.anti_wind),
            writable,
        ),
    ]);

    // Comme l'app : Dolby Atmos et profil personnalisé s'excluent ; on ne
    // propose d'activer l'audio spatial qu'hors du profil.
    let mut spatial = toggle(
        tr("Audio spatial"),
        MenuAction::Spatial,
        s.spatial_audio,
        writable,
    );
    if s.audio_mode == Some(AudioMode::Personalization)
        && s.spatial_audio == Some(false)
        && let MenuEntry::Check { enabled, .. } = &mut spatial
    {
        *enabled = false;
    }
    menu.push(spatial);
    menu.push(toggle(
        tr("Speech Clarity (voix)"),
        MenuAction::SpeechClarity,
        s.audio_mode.map(|m| m == AudioMode::SpeechClarity),
        writable,
    ));
    // Profil sélectionnable seulement s'il existe sur le casque.
    menu.push(toggle(
        tr("Personnalisation du son"),
        MenuAction::SoundProfile,
        s.audio_mode
            .filter(|_| s.sound_profile == Some(SoundProfileStatus::Received))
            .map(|m| m == AudioMode::Personalization),
        writable,
    ));
    menu.push(MenuEntry::Info(match state {
        Some(s) => audio_summary(s),
        None => tr("Mode audio : —").to_string(),
    }));
    menu.push(item(tr("Égaliseur…"), MenuAction::OpenEqualizer, true));

    menu.push(MenuEntry::Submenu {
        label: tr("Réglages du casque").to_string(),
        enabled: state.is_some(),
        entries: vec![
            toggle(
                tr("Smart Pause"),
                MenuAction::SmartPause,
                s.smart_pause,
                writable,
            ),
            toggle(
                tr("Réponse automatique aux appels"),
                MenuAction::AutoAnswer,
                s.auto_answer,
                writable,
            ),
            toggle(
                tr("Appels confortables"),
                MenuAction::ComfortCall,
                s.comfort_call,
                writable,
            ),
            MenuEntry::Submenu {
                label: tr("Extinction automatique").to_string(),
                enabled: writable && s.auto_off_seconds.is_some(),
                entries: AUTO_OFF_CHOICES
                    .iter()
                    .map(|&seconds| {
                        toggle(
                            &auto_off_label(seconds),
                            MenuAction::AutoOff(seconds),
                            s.auto_off_seconds.map(|v| v == seconds),
                            writable,
                        )
                    })
                    .collect(),
            },
            MenuEntry::Submenu {
                label: tr("Invites").to_string(),
                enabled: writable && s.prompt_mode.is_some(),
                entries: vec![
                    toggle(
                        tr("Tonalités seules"),
                        MenuAction::Prompts(PromptMode::TonesOnly),
                        s.prompt_mode.map(|m| m == PromptMode::TonesOnly),
                        writable,
                    ),
                    toggle(
                        tr("Tonalités et voix"),
                        MenuAction::Prompts(PromptMode::TonesAndVoice),
                        s.prompt_mode.map(|m| m == PromptMode::TonesAndVoice),
                        writable,
                    ),
                ],
            },
            MenuEntry::Separator,
            toggle(
                tr("Commandes tactiles"),
                MenuAction::TouchControls,
                s.touch_controls,
                writable,
            ),
            toggle(
                tr("Protection de la batterie"),
                MenuAction::BatteryProtection,
                s.battery_protection,
                writable,
            ),
        ],
    });

    let mut scenes: Vec<MenuEntry> = ctx
        .scenes
        .iter()
        .map(|name| item(name, MenuAction::Scene(name.clone()), writable))
        .collect();
    if !scenes.is_empty() {
        scenes.push(MenuEntry::Separator);
    }
    scenes.push(item(
        tr("Recharger le fichier des scènes"),
        MenuAction::ReloadScenes,
        true,
    ));
    menu.push(MenuEntry::Submenu {
        label: tr("Scènes").to_string(),
        enabled: true,
        entries: scenes,
    });

    menu.extend([
        MenuEntry::Separator,
        check(
            ctx.autostart_label,
            MenuAction::Autostart,
            ctx.autostart,
            true,
        ),
        item(tr("Quitter"), MenuAction::Quit, true),
    ]);
    menu
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::NoiseState;

    fn find<'a>(entries: &'a [MenuEntry], label: &str) -> Option<&'a MenuEntry> {
        entries.iter().find_map(|e| match e {
            MenuEntry::Item { label: l, .. } | MenuEntry::Check { label: l, .. } if l == label => {
                Some(e)
            }
            MenuEntry::Submenu { entries, .. } => find(entries, label),
            _ => None,
        })
    }

    fn ctx<'a>(scenes: &'a [String]) -> MenuContext<'a> {
        MenuContext {
            name: Some("M5"),
            writable: true,
            scenes,
            autostart_label: "Lancer au démarrage de Windows",
            ..MenuContext::default()
        }
    }

    #[test]
    fn reconnect_uses_service_availability_instead_of_the_write_guard() {
        for available in [false, true] {
            let context = MenuContext {
                can_reconnect: available,
                ..MenuContext::default()
            };
            let menu = tray_menu(None, &context);
            assert!(
                matches!(find(&menu, "Reconnecter le casque"), Some(MenuEntry::Item { action: MenuAction::ReconnectHeadset, enabled, .. }) if *enabled == available)
            );
            assert!(
                find(
                    &tray_menu(Some(&HeadsetState::default()), &context),
                    "Reconnecter le casque"
                )
                .is_none()
            );
        }
    }

    #[test]
    fn without_headset_settings_are_greyed() {
        let menu = tray_menu(None, &ctx(&[]));
        assert_eq!(menu[1], MenuEntry::Info("M5 — connexion…".to_string()));
        let Some(MenuEntry::Check { enabled, .. }) = find(&menu, "Anti-vent") else {
            panic!("anti-vent absent");
        };
        assert!(!enabled);
        assert!(matches!(
            find(&menu, "Quitter"),
            Some(MenuEntry::Item { enabled: true, .. })
        ));
    }

    #[test]
    fn read_values_are_checked_and_unread_ones_greyed() {
        let state = HeadsetState {
            battery: 70,
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 25,
            },
            smart_pause: Some(true),
            auto_off_seconds: Some(3600),
            ..HeadsetState::default()
        };
        let scenes = ["Travail".to_string()];
        let menu = tray_menu(Some(&state), &ctx(&scenes));
        assert_eq!(menu[1], MenuEntry::Info("M5 — batterie 70 %".to_string()));
        let checked = |label: &str| match find(&menu, label) {
            Some(MenuEntry::Check {
                checked, enabled, ..
            }) => (*checked, *enabled),
            other => panic!("{label} : {other:?}"),
        };
        assert_eq!(checked("Personnalisé"), (true, true));
        assert_eq!(checked("Anti-vent"), (true, true));
        assert_eq!(checked("Smart Pause"), (true, true));
        assert_eq!(checked("60 min"), (true, true));
        // Non lu : grisé, jamais coché.
        assert_eq!(checked("Réponse automatique aux appels"), (false, false));
        assert!(matches!(
            find(&menu, "Travail"),
            Some(MenuEntry::Item {
                action: MenuAction::Scene(_),
                enabled: true,
                ..
            })
        ));
    }

    #[test]
    fn problem_line_follows_the_status() {
        let c = MenuContext {
            problem: Some("connexion perdue"),
            ..ctx(&[])
        };
        let menu = tray_menu(None, &c);
        assert_eq!(menu[2], MenuEntry::Info("⚠ connexion perdue".to_string()));
    }

    #[test]
    fn read_only_model_greys_every_setting() {
        let state = HeadsetState::default();
        let c = MenuContext {
            writable: false,
            ..ctx(&[])
        };
        let menu = tray_menu(Some(&state), &c);
        assert!(matches!(
            find(&menu, "ANC adaptatif"),
            Some(MenuEntry::Check { enabled: false, .. })
        ));
    }
}
