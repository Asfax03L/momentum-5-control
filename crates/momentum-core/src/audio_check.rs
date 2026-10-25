//! « Connexion et audio » : causes probables d'un casque « connecté, mais
//! sans son » (Rust pur, testable sans Windows ni casque).
//!
//! Trois sources distinctes, jamais confondues :
//! - le **pilotage** du casque (GAIA sur RFCOMM) : il prouve que ce PC
//!   commande le casque, pas qu'il lui envoie du son ;
//! - la **sortie audio de Windows** (sortie par défaut, format de mixage,
//!   volume) : le format de mixage décrit le traitement partagé de Windows,
//!   pas le codec radio ni une transmission sans perte ;
//! - le **flux rapporté par le casque** (codec `0880`, fréquence `089A`) :
//!   en Multipoint, l'appareil qui l'envoie n'est pas connu.
//!
//! Aucune commande n'est envoyée au casque et rien n'est changé dans Windows.

use crate::i18n::{tr, trf};
/// Liaison d'une sortie audio de Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputLink {
    Bluetooth,
    Usb,
    /// Autre (carte son, HDMI…) ou non reconnue.
    Other,
}

/// Format de mixage d'une sortie (format partagé de Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixFormat {
    pub channels: u16,
    pub rate_hz: u32,
    pub bits: u16,
    /// Échantillons en virgule flottante (cas habituel du mixeur).
    pub float: bool,
}

/// Sortie audio vue par Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioOutput {
    /// Nom affiché par Windows (« Casque (MOMENTUM 5) »).
    pub name: String,
    /// Sortie de ce casque (et non d'un autre appareil).
    pub headset: bool,
    pub link: OutputLink,
    /// Profil mains libres (HFP) : qualité téléphone, son mono.
    pub hands_free: bool,
    pub mix: Option<MixFormat>,
    pub muted: Option<bool>,
    /// Volume de Windows, en pourcentage.
    pub volume: Option<u8>,
}

/// Lecture des sorties de Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowsAudio {
    /// Sortie par défaut (« multimédia »), si Windows en a une.
    pub default_output: Option<AudioOutput>,
    /// Sorties actives de ce casque (stéréo, mains libres, USB).
    pub headset_outputs: Vec<AudioOutput>,
}

/// Classe une sortie d'après les textes que Windows lui associe (nom,
/// énumérateur, identifiants du périphérique). Repères :
/// - `BTHENUM` : Bluetooth ; `USB\` : USB (le M5 en USB : `VID_3542`) ;
/// - profil mains libres : services Bluetooth `0x111E`/`0x111F` (Hands-Free)
///   dans l'identifiant, ou « Hands-Free » dans le nom (relevé sur ce PC :
///   « MOMENTUM 5 … Hands-Free » ; la stéréo A2DP porte `0x110B`).
///
/// `device_name` : nom du casque lu par le pilotage, s'il est connu.
pub fn classify(texts: &[&str], device_name: Option<&str>) -> (bool, OutputLink, bool) {
    let upper: Vec<String> = texts.iter().map(|t| t.to_uppercase()).collect();
    let any = |needle: &str| upper.iter().any(|t| t.contains(needle));
    let headset = any(tr("MOMENTUM"))
        || any(tr("VID_3542"))
        || device_name
            .map(|n| n.trim().to_uppercase())
            .is_some_and(|n| n.len() >= 3 && any(&n));
    let link = if any(tr("BTHENUM")) {
        OutputLink::Bluetooth
    } else if upper.iter().any(|t| t == "USB" || t.contains("USB\\")) {
        OutputLink::Usb
    } else {
        OutputLink::Other
    };
    let hands_free = any("{0000111E-") || any("{0000111F-") || any(tr("HANDS-FREE"));
    (headset, link, hands_free)
}

/// « 48 kHz », « 44,1 kHz ».
pub fn format_rate(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        trf("{} kHz", &[&(hz / 1000)])
    } else {
        trf("{} kHz", &[&(format!("{:.1}", f64::from(hz) / 1000.0))]).replace('.', ",")
    }
}

/// « 2 canaux, 48 kHz, 32 bits virgule flottante ».
pub fn format_mix(mix: &MixFormat) -> String {
    let rate = format_rate(mix.rate_hz);
    let channels = match mix.channels {
        1 => tr("1 canal (mono)").to_string(),
        n => trf("{} canaux", &[&n]),
    };
    trf(
        "{}, {}, {} bits{}",
        &[
            &channels,
            &rate,
            &mix.bits,
            &(if mix.float { " virgule flottante" } else { "" }),
        ],
    )
}

/// Description courte d'une sortie : « Bluetooth, stéréo ».
pub fn describe_output(output: &AudioOutput) -> String {
    let link = match output.link {
        OutputLink::Bluetooth => tr("Bluetooth"),
        OutputLink::Usb => tr("USB"),
        OutputLink::Other => tr("autre liaison"),
    };
    let mode = if output.hands_free {
        tr("mains libres (qualité téléphone)")
    } else if output.mix.is_some_and(|m| m.channels == 1) {
        tr("mono")
    } else {
        tr("stéréo")
    };
    format!("{link}, {mode}")
}

/// État de ce PC dans la liste des connexions du casque (`140E`), valeur
/// brute. Seules 1, 2 et 3 ont été observées.
pub fn describe_link_state(state: u8) -> String {
    match state {
        1 => tr("non connecté (1)").to_string(),
        2 => tr("connecté (2)").to_string(),
        3 => tr("connecté, audio actif (3, observé pendant une lecture)").to_string(),
        n => trf("valeur {} non caractérisée", &[&n]),
    }
}

/// Ce que le pilotage sait de la liaison.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControlLink {
    /// Session de pilotage ouverte.
    pub connected: bool,
    /// État brut de ce PC dans la liste du casque, s'il y figure.
    pub this_pc_state: Option<u8>,
    /// Autres appareils connectés au casque (Multipoint).
    pub others_connected: usize,
}

/// Gravité d'un constat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Cause probable d'une absence de son.
    Problem,
    /// À savoir, sans être forcément une cause.
    Note,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

fn problem(text: impl Into<String>) -> Finding {
    Finding {
        level: Level::Problem,
        text: text.into(),
    }
}

fn note(text: impl Into<String>) -> Finding {
    Finding {
        level: Level::Note,
        text: text.into(),
    }
}

/// Constats, du plus probable au moins probable. `windows` vaut `None` si
/// Windows n'a pas pu être interrogé ; `codec` est le dernier codec rapporté
/// par le casque (`0xFF` : pas de lecture).
pub fn findings(
    control: &ControlLink,
    windows: Option<&WindowsAudio>,
    codec: Option<u8>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    if !control.connected {
        out.push(note(
            tr("Le casque n'est pas piloté par ce PC : les informations du casque ne sont pas disponibles. Le son peut fonctionner sans pilotage."),
        ));
    }
    let Some(windows) = windows else {
        out.push(note(tr(
            "Les sorties audio de Windows n'ont pas pu être lues.",
        )));
        return out;
    };
    match &windows.default_output {
        None => out.push(problem(tr(
            "Windows n'a aucune sortie audio par défaut : le son ne part nulle part.",
        ))),
        Some(output) if !output.headset => {
            out.push(problem(trf("Le son de Windows part vers « {} », pas vers le casque. Choisissez le casque dans les paramètres de son de Windows.", &[&output.name])));
        }
        Some(output) => {
            if output.muted == Some(true) {
                out.push(problem(tr(
                    "La sortie du casque est coupée (muet) dans Windows.",
                )));
            } else if output.volume == Some(0) {
                out.push(problem(tr(
                    "Le volume de Windows est à 0 sur la sortie du casque.",
                )));
            }
            if output.hands_free {
                out.push(problem(
                    tr("La sortie choisie est le profil mains libres : son mono de qualité téléphone. Choisissez la sortie stéréo du casque (sans « Hands-Free »), sauf pendant un appel."),
                ));
            }
        }
    }
    if control.connected && windows.headset_outputs.is_empty() {
        out.push(problem(
            tr("Le casque est piloté, mais Windows n'a aucune sortie audio active vers lui : le profil audio Bluetooth n'est pas connecté (ou le câble USB est débranché)."),
        ));
    }
    if codec == Some(0xFF) {
        out.push(note(tr(
            "Le casque indique qu'aucun flux audio n'est en cours (dernière valeur reçue).",
        )));
    }
    if control.others_connected > 0 {
        out.push(note(
            tr("Un autre appareil est connecté au casque (Multipoint) : le casque joue le son d'un appareil à la fois, et une lecture sur l'autre peut prendre la main. L'appareil qui envoie le flux n'est pas connu."),
        ));
    }
    if !out.iter().any(|f| f.level == Level::Problem) {
        out.push(note(
            tr("Aucune cause évidente côté PC : la sortie par défaut est le casque, sans muet. Vérifiez le volume de l'application et le volume du casque."),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str, headset: bool, hands_free: bool) -> AudioOutput {
        AudioOutput {
            name: name.into(),
            headset,
            link: OutputLink::Bluetooth,
            hands_free,
            mix: Some(MixFormat {
                channels: 2,
                rate_hz: 48_000,
                bits: 32,
                float: true,
            }),
            muted: Some(false),
            volume: Some(40),
        }
    }

    fn connected() -> ControlLink {
        ControlLink {
            connected: true,
            this_pc_state: Some(2),
            others_connected: 0,
        }
    }

    fn problems(f: &[Finding]) -> Vec<&str> {
        f.iter()
            .filter(|f| f.level == Level::Problem)
            .map(|f| f.text.as_str())
            .collect()
    }

    #[test]
    fn outputs_are_classified_from_windows_texts() {
        // Textes relevés sur ce PC (adresses retirées).
        let a2dp = [
            "Casque (MOMENTUM 5 Exemple)",
            "BTHENUM",
            r"{1}.BTHENUM\{0000110B-0000-1000-8000-‹adr›}_LOCALMFG&0002",
        ];
        assert_eq!(classify(&a2dp, None), (true, OutputLink::Bluetooth, false));
        // Déchargement Intel : énumérateur INTELAUDIO, identifiant BTHENUM.
        let hfp = [
            "Casque (Technologie Intel® Smart Sound pour audio Bluetooth®)",
            "INTELAUDIO",
            r"{1}.BTHENUM\{0000111E-0000-1000-8000-‹adr›}_HCIBYPASS",
            "MOMENTUM 5 Exemple Hands-Free",
        ];
        assert_eq!(classify(&hfp, None), (true, OutputLink::Bluetooth, true));
        let usb = [
            "Casque (MOMENTUM 5)",
            "USB",
            r"{1}.USB\VID_3542&PID_1004&MI_01\7&0&0001",
        ];
        assert_eq!(classify(&usb, None), (true, OutputLink::Usb, false));
        let speakers = ["Haut-parleurs (Realtek(R) Audio)", "HDAUDIO"];
        assert_eq!(
            classify(&speakers, Some("MOMENTUM 5 Exemple")),
            (false, OutputLink::Other, false)
        );
        // Casque renommé : reconnu par le nom lu par le pilotage.
        let renamed = ["Casque (Mon casque)", "BTHENUM"];
        assert!(classify(&renamed, Some("Mon casque")).0);
    }

    #[test]
    fn mix_format_is_described_without_claiming_a_codec() {
        let mix = MixFormat {
            channels: 2,
            rate_hz: 44_100,
            bits: 32,
            float: true,
        };
        assert_eq!(
            format_mix(&mix),
            "2 canaux, 44,1 kHz, 32 bits virgule flottante"
        );
        let mono = MixFormat {
            channels: 1,
            rate_hz: 16_000,
            bits: 16,
            float: false,
        };
        assert_eq!(format_mix(&mono), "1 canal (mono), 16 kHz, 16 bits");
    }

    #[test]
    fn link_states_keep_unknown_values_unknown() {
        assert_eq!(describe_link_state(2), "connecté (2)");
        assert!(describe_link_state(3).contains("audio actif"));
        assert_eq!(describe_link_state(7), "valeur 7 non caractérisée");
    }

    #[test]
    fn sound_going_elsewhere_is_the_first_cause() {
        let windows = WindowsAudio {
            default_output: Some(AudioOutput {
                link: OutputLink::Other,
                ..output("Haut-parleurs", false, false)
            }),
            headset_outputs: vec![output("Casque (MOMENTUM 5)", true, false)],
        };
        let f = findings(&connected(), Some(&windows), Some(2));
        assert_eq!(problems(&f).len(), 1);
        assert!(problems(&f)[0].contains("« Haut-parleurs »"));
    }

    #[test]
    fn hands_free_mute_and_missing_outputs_are_reported() {
        let mut hfp = output("Casque (MOMENTUM 5 Hands-Free)", true, true);
        hfp.muted = Some(true);
        let windows = WindowsAudio {
            default_output: Some(hfp.clone()),
            headset_outputs: vec![hfp],
        };
        let f = findings(&connected(), Some(&windows), None);
        let p = problems(&f);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].contains("muet"));
        assert!(p[1].contains("mains libres"));

        let none = WindowsAudio {
            default_output: Some(AudioOutput {
                link: OutputLink::Other,
                ..output("Haut-parleurs", false, false)
            }),
            headset_outputs: Vec::new(),
        };
        let f = findings(&connected(), Some(&none), None);
        assert!(
            problems(&f)
                .iter()
                .any(|t| t.contains("aucune sortie audio active"))
        );
    }

    #[test]
    fn a_healthy_route_gives_notes_only() {
        let good = output("Casque (MOMENTUM 5)", true, false);
        let windows = WindowsAudio {
            default_output: Some(good.clone()),
            headset_outputs: vec![good],
        };
        let control = ControlLink {
            others_connected: 1,
            ..connected()
        };
        let f = findings(&control, Some(&windows), Some(0xFF));
        assert!(problems(&f).is_empty());
        let texts: Vec<&str> = f.iter().map(|f| f.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("aucun flux audio")));
        assert!(texts.iter().any(|t| t.contains("Multipoint")));
        assert!(texts.iter().any(|t| t.contains("Aucune cause évidente")));
    }

    #[test]
    fn unreadable_windows_and_no_control_are_notes() {
        let f = findings(&ControlLink::default(), None, None);
        assert!(problems(&f).is_empty());
        assert_eq!(f.len(), 2);
    }
}
