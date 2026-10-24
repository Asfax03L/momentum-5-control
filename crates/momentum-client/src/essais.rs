//! Suite d'essais sur le casque, rejouée avant chaque publication
//! (`ctl essais`) : pour chaque réglage validé de la liste, une autre valeur
//! est écrite et relue, puis la valeur d'origine est remise et relue.
//!
//! Liste fermée : seuls les réglages ci-dessous, déjà validés sur le M5.
//! Jamais : nom, multipoint, connexions, profil personnalisé, langue,
//! priorité du mode audio, ni aucune autre commande. La première anomalie
//! arrête la suite après une tentative de remise à la valeur d'origine.

use momentum_core::device::{AUTO_OFF_CHOICES, AudioMode, PromptMode};
use momentum_core::request::Outcome;
use serde::Serialize;

use crate::{Command, Snapshot};

/// Réglage essayé.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Essai {
    Niveau,
    AntiVent,
    Bande,
    Basses,
    Tactile,
    SmartPause,
    ReponseAuto,
    AppelsConfort,
    Extinction,
    Invites,
    VolumeInvites,
    Voix,
}

/// Tous les essais, dans l'ordre de la suite.
pub const TOUS: [Essai; 12] = [
    Essai::Niveau,
    Essai::AntiVent,
    Essai::Bande,
    Essai::Basses,
    Essai::Tactile,
    Essai::SmartPause,
    Essai::ReponseAuto,
    Essai::AppelsConfort,
    Essai::Extinction,
    Essai::Invites,
    Essai::VolumeInvites,
    Essai::Voix,
];

impl Essai {
    /// Nom de l'essai (option `--liste` de `ctl essais`).
    pub fn key(self) -> &'static str {
        match self {
            Essai::Niveau => "niveau",
            Essai::AntiVent => "anti-vent",
            Essai::Bande => "bande",
            Essai::Basses => "basses",
            Essai::Tactile => "tactile",
            Essai::SmartPause => "smart-pause",
            Essai::ReponseAuto => "reponse-auto",
            Essai::AppelsConfort => "appels-confort",
            Essai::Extinction => "extinction",
            Essai::Invites => "invites",
            Essai::VolumeInvites => "volume-invites",
            Essai::Voix => "voix",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        TOUS.into_iter().find(|e| e.key() == key.trim())
    }

    /// Valeur relue, sous une forme comparable ; `Err` : essai sans objet
    /// (valeur non lue, ou état qui l'interdit).
    pub fn read(self, s: &Snapshot) -> Result<i32, String> {
        let state = s.state.as_ref().ok_or("état du casque non lu")?;
        let unread = || format!("{} non lu", self.key());
        let flag = |v: Option<bool>| v.map(i32::from).ok_or_else(unread);
        match self {
            Essai::Niveau => {
                let n = &state.noise;
                if n.anc_enabled && !n.adaptive {
                    Ok(i32::from(n.level))
                } else {
                    // Écrire le niveau changerait aussi le mode de bruit.
                    Err("mode de bruit autre que Personnalisé".into())
                }
            }
            Essai::AntiVent => Ok(i32::from(state.noise.anti_wind)),
            Essai::Bande => s
                .equalizer
                .as_ref()
                .and_then(|eq| eq.gains.first())
                .map(|&g| i32::from(g))
                .ok_or_else(unread),
            Essai::Basses => s
                .equalizer
                .as_ref()
                .map(|eq| i32::from(eq.bass_boost))
                .ok_or_else(unread),
            Essai::Tactile => flag(state.touch_controls),
            Essai::SmartPause => flag(state.smart_pause),
            Essai::ReponseAuto => flag(state.auto_answer),
            Essai::AppelsConfort => flag(state.comfort_call),
            Essai::Extinction => state.auto_off_seconds.map(i32::from).ok_or_else(unread),
            Essai::Invites => match state.prompt_mode {
                Some(PromptMode::TonesOnly) => Ok(1),
                Some(PromptMode::TonesAndVoice) => Ok(2),
                _ => Err(unread()),
            },
            Essai::VolumeInvites => s
                .info
                .as_ref()
                .and_then(|i| i.prompt_volume)
                .map(i32::from)
                .ok_or_else(unread),
            Essai::Voix => match state.audio_mode {
                Some(AudioMode::Equalizer) => Ok(0),
                Some(AudioMode::SpeechClarity) => Ok(1),
                // Revenir au profil exigerait une commande hors de la liste.
                Some(AudioMode::Personalization) => Err("profil personnalisé sélectionné".into()),
                _ => Err(unread()),
            },
        }
    }

    /// Autre valeur, à écrire pour l'essai (dans les limites du casque).
    pub fn other(self, value: i32, s: &Snapshot) -> i32 {
        match self {
            Essai::Niveau => {
                if value <= 50 {
                    value + 25
                } else {
                    value - 25
                }
            }
            Essai::Bande => {
                let max = s.equalizer.as_ref().map_or(0, |eq| i32::from(eq.max_gain));
                // Un dB de plus, ou de moins en haut de la plage.
                if value + 10 <= max {
                    value + 10
                } else {
                    value - 10
                }
            }
            Essai::Extinction => {
                let next = if value == i32::from(AUTO_OFF_CHOICES[1]) {
                    AUTO_OFF_CHOICES[2]
                } else {
                    AUTO_OFF_CHOICES[1]
                };
                i32::from(next)
            }
            Essai::Invites => 3 - value,
            Essai::VolumeInvites => {
                if value + 2 <= 6 {
                    value + 2
                } else {
                    value - 2
                }
            }
            // Interrupteurs.
            _ => 1 - value,
        }
    }

    /// Commande de l'API qui écrit `value`.
    pub fn command(self, value: i32) -> Command {
        let on = value != 0;
        match self {
            Essai::Niveau => Command::SetLevel(value.clamp(0, 100) as u8),
            Essai::AntiVent => Command::SetAntiWind(on),
            Essai::Bande => Command::SetEqBand(0, value.clamp(-128, 127) as i8),
            Essai::Basses => Command::SetBassBoost(on),
            Essai::Tactile => Command::SetTouchControls(on),
            Essai::SmartPause => Command::SetSmartPause(on),
            Essai::ReponseAuto => Command::SetAutoAnswer(on),
            Essai::AppelsConfort => Command::SetComfortCall(on),
            Essai::Extinction => Command::SetAutoOff(value.clamp(0, 3600) as u16),
            Essai::Invites => Command::SetPromptMode(if value == 1 {
                PromptMode::TonesOnly
            } else {
                PromptMode::TonesAndVoice
            }),
            Essai::VolumeInvites => Command::SetPromptVolume(value.clamp(-20, 6) as i8),
            Essai::Voix => Command::SetSpeechClarity(on),
        }
    }
}

/// Accès au service pour la suite (D-Bus, canal nommé, ou essai simulé).
pub trait Service {
    fn snapshot(&mut self) -> Result<Snapshot, String>;
    /// Envoie une demande suivie et attend son résultat (`Done`).
    fn submit(&mut self, command: &Command, session: u64) -> Result<Outcome, String>;
}

/// Issue d'un essai.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Statut {
    Reussi,
    Saute(String),
    Echec(String),
    /// Pas lancé : une anomalie précédente a arrêté la suite.
    NonLance,
}

#[derive(Clone, Debug, Serialize)]
pub struct Resultat {
    pub essai: &'static str,
    pub statut: Statut,
    pub origine: Option<i32>,
    pub essai_valeur: Option<i32>,
    /// Valeur d'origine relue à la fin (`None` : pas besoin, ou inconnue).
    pub remis: Option<bool>,
}

/// Plan de la suite, sans rien envoyer (`--simulation`).
pub fn plan(essais: &[Essai], s: &Snapshot) -> Vec<Resultat> {
    essais
        .iter()
        .map(|&e| match e.read(s) {
            Ok(v) => Resultat {
                essai: e.key(),
                statut: Statut::NonLance,
                origine: Some(v),
                essai_valeur: Some(e.other(v, s)),
                remis: None,
            },
            Err(reason) => Resultat {
                essai: e.key(),
                statut: Statut::Saute(reason),
                origine: None,
                essai_valeur: None,
                remis: None,
            },
        })
        .collect()
}

/// Écrit `value` puis vérifie la relecture.
fn write_and_check(service: &mut impl Service, e: Essai, value: i32) -> Result<(), String> {
    let session = service.snapshot()?.session;
    let outcome = service.submit(&e.command(value), session)?;
    if !outcome.is_success() {
        return Err(outcome.describe());
    }
    match e.read(&service.snapshot()?) {
        Ok(read) if read == value => Ok(()),
        Ok(read) => Err(format!("relu {read} au lieu de {value}")),
        Err(reason) => Err(format!("relecture impossible : {reason}")),
    }
}

/// Lance la suite ; s'arrête à la première anomalie, après une tentative
/// de remise à la valeur d'origine.
pub fn run(service: &mut impl Service, essais: &[Essai]) -> Vec<Resultat> {
    let mut out = Vec::new();
    let mut stopped = false;
    for &e in essais {
        let mut r = Resultat {
            essai: e.key(),
            statut: Statut::NonLance,
            origine: None,
            essai_valeur: None,
            remis: None,
        };
        if stopped {
            out.push(r);
            continue;
        }
        let snapshot = match service.snapshot() {
            Ok(s) => s,
            Err(reason) => {
                r.statut = Statut::Echec(reason);
                stopped = true;
                out.push(r);
                continue;
            }
        };
        let origin = match e.read(&snapshot) {
            Ok(v) => v,
            Err(reason) => {
                r.statut = Statut::Saute(reason);
                out.push(r);
                continue;
            }
        };
        let other = e.other(origin, &snapshot);
        r.origine = Some(origin);
        r.essai_valeur = Some(other);
        let result = write_and_check(service, e, other)
            .map_err(|why| format!("écriture de {other} : {why}"))
            .and_then(|()| {
                write_and_check(service, e, origin)
                    .map_err(|why| format!("remise à {origin} : {why}"))
            });
        match result {
            Ok(()) => {
                r.statut = Statut::Reussi;
                r.remis = Some(true);
            }
            Err(reason) => {
                // Tentative de remise, quel que soit l'état relu.
                let restored = write_and_check(service, e, origin).is_ok();
                r.statut = Statut::Echec(reason);
                r.remis = Some(restored);
                stopped = true;
            }
        }
        out.push(r);
    }
    out
}

/// Contexte du compte rendu : rien qui désigne l'utilisateur ou l'appareil.
pub struct Contexte<'a> {
    /// Date et heure UTC, ISO 8601.
    pub date: &'a str,
    pub version: &'a str,
    pub systeme: &'a str,
    pub simulation: bool,
}

fn statut_texte(s: &Statut) -> String {
    match s {
        Statut::Reussi => "réussi (écrit, relu, remis, relu)".into(),
        Statut::Saute(r) => format!("sauté : {r}"),
        Statut::Echec(r) => format!("**échec** : {r}"),
        Statut::NonLance => "non lancé".into(),
    }
}

/// Compte rendu Markdown daté : version, système, modèle et micrologiciel
/// relus, résultat de chaque essai. Ni nom du casque, ni adresse, ni numéro
/// de série.
pub fn report(ctx: &Contexte, s: &Snapshot, results: &[Resultat]) -> String {
    let info = s.info.as_ref();
    let model = info.and_then(|i| i.model.as_deref()).unwrap_or("non lu");
    let firmware = info.and_then(|i| i.firmware.as_deref()).unwrap_or("non lu");
    let failed = results.iter().any(|r| matches!(r.statut, Statut::Echec(_)));
    let mut out = format!(
        "# Essais sur le casque — {}\n\n\
         - OpenHeadset Control {}, {}\n\
         - Modèle : {model} ; micrologiciel : {firmware}\n\
         - {}\n\n\
         | Essai | Origine | Essai | Résultat |\n|---|---|---|---|\n",
        ctx.date,
        ctx.version,
        ctx.systeme,
        if ctx.simulation {
            "Simulation : plan seulement, rien n'a été envoyé"
        } else if failed {
            "Résultat : **anomalie**, suite arrêtée"
        } else {
            "Résultat : tous les essais lancés ont réussi"
        },
    );
    for r in results {
        let v = |x: Option<i32>| x.map_or_else(|| "—".to_string(), |v| v.to_string());
        let statut = if ctx.simulation && r.statut == Statut::NonLance {
            "prévu".to_string()
        } else {
            statut_texte(&r.statut)
        };
        out.push_str(&format!(
            "| {} | {} | {} | {statut} |\n",
            r.essai,
            v(r.origine),
            v(r.essai_valeur)
        ));
        if r.remis == Some(false) {
            out.push_str(&format!(
                "|  |  |  | ⚠ valeur d'origine non relue : vérifier « {} » |\n",
                r.essai
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use momentum_core::device::{DeviceInfo, EqState, HeadsetState, NoiseState};

    fn snapshot() -> Snapshot {
        Snapshot {
            session: 3,
            state: Some(HeadsetState {
                battery: 50,
                noise: NoiseState {
                    anc_enabled: true,
                    adaptive: false,
                    level: 0,
                    anti_wind: true,
                },
                audio_mode: Some(AudioMode::Equalizer),
                touch_controls: Some(true),
                smart_pause: Some(true),
                auto_answer: Some(false),
                comfort_call: Some(true),
                auto_off_seconds: Some(3600),
                prompt_mode: Some(PromptMode::TonesAndVoice),
                ..HeadsetState::default()
            }),
            equalizer: Some(EqState {
                min_gain: -60,
                max_gain: 60,
                gains: vec![60, 0, 0, 0, 0, 0, 0, 0],
                bass_boost: false,
            }),
            info: Some(DeviceInfo {
                name: Some("Casque démo".into()),
                model: Some("M5AEBT".into()),
                firmware: Some("6.21.5".into()),
                prompt_volume: Some(6),
                ..DeviceInfo::default()
            }),
            ..Snapshot::default()
        }
    }

    /// Casque simulé : applique chaque commande à son état, sauf celles
    /// listées dans `refuse` (refusées) et `ignore` (acquittées, non appliquées).
    struct Fake {
        snapshot: Snapshot,
        sent: Vec<Command>,
        ignore: Option<Essai>,
        refuse: Option<Essai>,
    }

    impl Service for Fake {
        fn snapshot(&mut self) -> Result<Snapshot, String> {
            Ok(self.snapshot.clone())
        }
        fn submit(&mut self, command: &Command, session: u64) -> Result<Outcome, String> {
            assert_eq!(session, 3);
            self.sent.push(command.clone());
            let essai = TOUS
                .into_iter()
                .find(|e| std::mem::discriminant(&e.command(0)) == std::mem::discriminant(command))
                .expect("commande hors de la liste");
            if self.refuse == Some(essai) {
                return Ok(Outcome::Refused("refus simulé".into()));
            }
            if self.ignore == Some(essai) {
                return Ok(Outcome::Applied);
            }
            let s = self.snapshot.state.as_mut().unwrap();
            let eq = self.snapshot.equalizer.as_mut().unwrap();
            let info = self.snapshot.info.as_mut().unwrap();
            match command.clone() {
                Command::SetLevel(v) => s.noise.level = v,
                Command::SetAntiWind(v) => s.noise.anti_wind = v,
                Command::SetEqBand(0, g) => eq.gains[0] = g,
                Command::SetBassBoost(v) => eq.bass_boost = v,
                Command::SetTouchControls(v) => s.touch_controls = Some(v),
                Command::SetSmartPause(v) => s.smart_pause = Some(v),
                Command::SetAutoAnswer(v) => s.auto_answer = Some(v),
                Command::SetComfortCall(v) => s.comfort_call = Some(v),
                Command::SetAutoOff(v) => s.auto_off_seconds = Some(v),
                Command::SetPromptMode(m) => s.prompt_mode = Some(m),
                Command::SetPromptVolume(v) => info.prompt_volume = Some(v),
                Command::SetSpeechClarity(on) => {
                    s.audio_mode = Some(if on {
                        AudioMode::SpeechClarity
                    } else {
                        AudioMode::Equalizer
                    })
                }
                other => panic!("commande inattendue {other:?}"),
            }
            Ok(Outcome::Applied)
        }
    }

    fn fake() -> Fake {
        Fake {
            snapshot: snapshot(),
            sent: Vec::new(),
            ignore: None,
            refuse: None,
        }
    }

    #[test]
    fn the_whole_suite_writes_reads_and_restores_every_setting() {
        let mut f = fake();
        let before = f.snapshot.clone();
        let results = run(&mut f, &TOUS);
        assert!(
            results.iter().all(|r| r.statut == Statut::Reussi),
            "{results:?}"
        );
        // Deux écritures par essai, et l'état final est l'état d'origine.
        assert_eq!(f.sent.len(), 2 * TOUS.len());
        assert_eq!(f.snapshot, before);
        // Bande au maximum (+6 dB) : l'essai descend d'un dB.
        assert_eq!(results[2].essai_valeur, Some(50));
        // Volume des invites au maximum (+6) : l'essai descend.
        assert_eq!(results[10].essai_valeur, Some(4));
    }

    #[test]
    fn only_listed_commands_are_ever_sent() {
        let mut f = fake();
        run(&mut f, &TOUS);
        for c in &f.sent {
            assert!(!matches!(
                c,
                Command::SetName(_)
                    | Command::SetMultipoint(_)
                    | Command::ToggleConnection { .. }
                    | Command::SetSoundProfile(_)
                    | Command::ReconnectHeadset
                    | Command::ApplyScene(_)
                    | Command::ExportProfile(_)
            ));
        }
    }

    #[test]
    fn an_anomaly_stops_the_suite_after_a_restore_attempt() {
        let mut f = fake();
        f.ignore = Some(Essai::Basses);
        let results = run(&mut f, &TOUS);
        assert!(
            matches!(results[3].statut, Statut::Echec(ref r) if r.contains("relu 0 au lieu de 1"))
        );
        assert_eq!(results[3].remis, Some(true));
        assert!(results[4..].iter().all(|r| r.statut == Statut::NonLance));
        let mut f = fake();
        f.refuse = Some(Essai::AntiVent);
        let results = run(&mut f, &[Essai::AntiVent, Essai::Tactile]);
        assert!(matches!(results[0].statut, Statut::Echec(ref r) if r.contains("refus simulé")));
        assert_eq!(results[1].statut, Statut::NonLance);
    }

    #[test]
    fn settings_that_would_need_other_commands_are_skipped() {
        let mut f = fake();
        let s = f.snapshot.state.as_mut().unwrap();
        s.noise.adaptive = true;
        s.audio_mode = Some(AudioMode::Personalization);
        let results = run(&mut f, &[Essai::Niveau, Essai::Voix]);
        assert!(results.iter().all(|r| matches!(r.statut, Statut::Saute(_))));
        assert!(f.sent.is_empty());
    }

    #[test]
    fn simulation_and_report_send_nothing_and_name_nothing() {
        let s = snapshot();
        let results = plan(&TOUS, &s);
        let text = report(
            &Contexte {
                date: "2026-10-23T20:00:00Z",
                version: "0.11.0",
                systeme: "linux",
                simulation: true,
            },
            &s,
            &results,
        );
        assert!(text.contains("M5AEBT") && text.contains("6.21.5"));
        assert!(text.contains("| niveau | 0 | 25 | prévu |"));
        assert!(!text.contains("Casque démo"));
        assert_eq!(
            Essai::from_key("volume-invites"),
            Some(Essai::VolumeInvites)
        );
        assert_eq!(Essai::from_key("nom"), None);
    }
}
