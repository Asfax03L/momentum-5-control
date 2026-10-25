//! Éléments d'interface sans dépendance graphique (testables partout) :
//! libellés en français, infobulle et pixels de l'icône de la barre système.

use crate::device::{AudioMode, HeadsetState, NoiseState, PromptMode, SoundProfileStatus};
use crate::protocol::messages::M5NoiseMode;

/// Contrôle du bruit en deux lignes (mode, anti-vent).
pub fn describe_noise(state: &NoiseState) -> String {
    let mode = match state.mode() {
        M5NoiseMode::Off => "ARRÊT".to_string(),
        M5NoiseMode::Adaptive => "Adaptatif".to_string(),
        M5NoiseMode::Custom { level } => format!("Personnalisé, curseur {level} %"),
    };
    format!(
        "Contrôle du bruit : {mode}\nAnti-vent : {}",
        if state.anti_wind {
            "activé"
        } else {
            "désactivé"
        }
    )
}

/// État complet, une ligne par réglage ; « non lu » si le casque a refusé
/// la lecture.
pub fn describe_state(state: &HeadsetState) -> String {
    fn show<T>(lines: &mut Vec<String>, label: &str, value: Option<T>, text: impl Fn(T) -> String) {
        lines.push(format!(
            "{label} : {}",
            value.map_or("non lu".to_string(), text)
        ));
    }
    let on_off = |on: bool| if on { "activé" } else { "désactivé" }.to_string();
    let on_off_f = |on: bool| if on { "activée" } else { "désactivée" }.to_string();
    let mut lines = vec![
        format!("Batterie : {} %", state.battery),
        describe_noise(&state.noise),
    ];
    show(&mut lines, "Audio spatial", state.spatial_audio, on_off);
    show(&mut lines, "Suivi de tête", state.head_tracking, on_off);
    show(&mut lines, "Mode audio", state.audio_mode, |m| m.label());
    show(
        &mut lines,
        "Personnalisation du son",
        state.sound_profile,
        |s| s.label(),
    );
    show(
        &mut lines,
        "Commandes tactiles",
        state.touch_controls,
        |on| if on { "actives" } else { "désactivées" }.to_string(),
    );
    show(
        &mut lines,
        "Protection de la batterie",
        state.battery_protection,
        on_off_f,
    );
    show(&mut lines, "Smart Pause", state.smart_pause, on_off_f);
    show(
        &mut lines,
        "Réponse automatique",
        state.auto_answer,
        on_off_f,
    );
    show(
        &mut lines,
        "Appels confortables",
        state.comfort_call,
        |on| if on { "activés" } else { "désactivés" }.to_string(),
    );
    show(
        &mut lines,
        "Extinction automatique",
        state.auto_off_seconds,
        |s| match s {
            0 => "jamais".to_string(),
            s => format!("{} min", s / 60),
        },
    );
    show(&mut lines, "Invites", state.prompt_mode, |m| {
        match m {
            PromptMode::TonesOnly => "tonalités",
            PromptMode::TonesAndVoice => "tonalités et voix",
            PromptMode::Unknown => "autre",
        }
        .to_string()
    });
    lines.join("\n")
}

/// Crans proposés dans le menu pour le curseur ANC ↔ transparence.
pub const LEVEL_PRESETS: [(u8, &str); 5] = [
    (0, "ANC maximal"),
    (25, "ANC léger"),
    (50, "Neutre"),
    (75, "Transparence légère"),
    (100, "Transparence maximale"),
];

/// Taille de l'icône dessinée (Windows la met à l'échelle).
pub const ICON_SIZE: u32 = 32;

/// Fréquences centrales des 8 bandes du M5 (notification `108B` capturée).
/// Constante du profil M5 (F11) : le casque donne le nombre de bandes et la
/// plage de gain, pas ces fréquences ; un autre casque pourrait différer.
pub const M5_EQ_FREQUENCIES_HZ: [u16; 8] = [90, 325, 1500, 2500, 4500, 8000, 13000, 16000];

/// Gain en dixièmes de dB au format français : « −3,5 dB », « +1 dB », « 0 dB ».
pub fn format_db(tenths: i8) -> String {
    let abs = i16::from(tenths).abs();
    let sign = match tenths {
        0 => "",
        t if t > 0 => "+",
        _ => "−",
    };
    let decimal = if abs % 10 == 0 {
        String::new()
    } else {
        format!(",{}", abs % 10)
    };
    format!("{sign}{}{decimal} dB", abs / 10)
}

/// Libellé d'une bande : sa fréquence si l'égaliseur a la forme connue du
/// M5 (8 bandes), sinon un simple numéro.
pub fn band_label(index: usize, band_count: usize) -> String {
    match M5_EQ_FREQUENCIES_HZ.get(index) {
        Some(&hz) if band_count == M5_EQ_FREQUENCIES_HZ.len() => {
            if hz >= 1000 {
                let khz = f32::from(hz) / 1000.0;
                // « 1,5 kHz » mais « 8 kHz » (virgule décimale française).
                let text = if hz % 1000 == 0 {
                    format!("{khz:.0}")
                } else {
                    format!("{khz:.1}").replace('.', ",")
                };
                format!("{text} kHz")
            } else {
                format!("{hz} Hz")
            }
        }
        _ => format!("Bande {}", index + 1),
    }
}

pub fn mode_label(mode: M5NoiseMode) -> String {
    match mode {
        M5NoiseMode::Off => "Contrôle du bruit désactivé".to_string(),
        M5NoiseMode::Adaptive => "ANC adaptatif".to_string(),
        M5NoiseMode::Custom { level } => match LEVEL_PRESETS.iter().find(|(l, _)| *l == level) {
            Some((_, name)) => format!("Personnalisé : {name} ({level} %)"),
            None => format!("Personnalisé : {level} %"),
        },
    }
}

/// Infobulle de l'icône (Windows la tronque à 127 caractères).
///
/// `problem` : dernier incident à signaler (connexion perdue, réglage
/// refusé), affiché en dernière ligne.
pub fn tooltip(
    device_name: Option<&str>,
    state: Option<&HeadsetState>,
    problem: Option<&str>,
) -> String {
    let mut text = match (device_name, state) {
        (Some(name), Some(s)) => {
            let mut t = format!(
                "{name}\nBatterie {} %\n{}",
                s.battery,
                mode_label(s.noise.mode())
            );
            // Le mode audio n'est rappelé que s'il sort de l'ordinaire.
            if let Some(mode) = s.audio_mode
                && mode != AudioMode::Equalizer
            {
                t.push_str(&format!("\n{}", audio_mode_label(mode)));
            }
            t
        }
        (Some(name), None) => format!("{name}\nConnexion…"),
        _ => "Casque Sennheiser\nDéconnecté, recherche en cours…".to_string(),
    };
    if let Some(problem) = problem {
        text.push_str(&format!("\n⚠ {problem}"));
    }
    text.chars().take(127).collect()
}

/// Mode audio en clair (valeur brute affichée si inconnue).
pub fn audio_mode_label(mode: AudioMode) -> String {
    match mode {
        AudioMode::Equalizer => "Mode audio : égaliseur".to_string(),
        AudioMode::SpeechClarity => "Mode audio : Speech Clarity".to_string(),
        AudioMode::Personalization => "Mode audio : profil personnalisé".to_string(),
        AudioMode::Unknown(b) => format!("Mode audio : inconnu ({b})"),
    }
}

/// Ligne d'information du menu : mode audio et état du profil, sans rien
/// déduire de l'un pour l'autre.
pub fn audio_summary(state: &HeadsetState) -> String {
    let profile = match state.sound_profile {
        None => "non lu",
        Some(SoundProfileStatus::NoProfile) => "aucun",
        Some(SoundProfileStatus::TestRunning) => "test en cours",
        Some(SoundProfileStatus::Received) => "reçu",
        Some(SoundProfileStatus::Unknown(_)) => "état inconnu",
    };
    let mode = state
        .audio_mode
        .map_or("Mode audio : non lu".to_string(), audio_mode_label);
    format!("{mode} · profil personnalisé : {profile}")
}

/// Avertissement de la fenêtre d'égaliseur quand le mode audio du casque
/// n'est pas l'égaliseur graphique (rien sinon).
pub fn eq_mode_note(mode: AudioMode) -> Option<String> {
    match mode {
        AudioMode::Equalizer => None,
        AudioMode::SpeechClarity => {
            Some("Mode audio actif : Speech Clarity, et non l'égaliseur graphique.".to_string())
        }
        AudioMode::Personalization => Some(
            "Mode audio actif : profil personnalisé (personnalisation du son), et non l'égaliseur graphique."
                .to_string(),
        ),
        AudioMode::Unknown(b) => Some(format!(
            "Mode audio actif inconnu (valeur {b}) : l'égaliseur graphique n'est peut-être pas appliqué."
        )),
    }
}

/// Couleur du fond de l'icône selon l'état.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Disconnected,
    Off,
    Anc,
    Adaptive,
    Transparency,
}

impl IconKind {
    pub fn from_state(state: Option<&HeadsetState>) -> Self {
        match state.map(|s| s.noise.mode()) {
            None => IconKind::Disconnected,
            Some(M5NoiseMode::Off) => IconKind::Off,
            Some(M5NoiseMode::Adaptive) => IconKind::Adaptive,
            // Au-dessus du neutre, le curseur penche vers la transparence.
            Some(M5NoiseMode::Custom { level }) if level > 50 => IconKind::Transparency,
            Some(M5NoiseMode::Custom { .. }) => IconKind::Anc,
        }
    }

    fn rgba(self) -> [u8; 4] {
        match self {
            IconKind::Disconnected => [128, 128, 128, 140],
            IconKind::Off => [100, 116, 139, 255],
            IconKind::Anc => [59, 130, 246, 255],
            IconKind::Adaptive => [20, 184, 166, 255],
            IconKind::Transparency => [245, 158, 11, 255],
        }
    }
}

/// Dessine le logo (casque blanc sur un carré arrondi de la couleur de
/// l'état) en RGBA, `size` × `size`.
///
/// Dessin par « distance signée » : chaque pixel est couvert selon sa
/// distance au bord de la forme, ce qui donne un anticrénelage gratuit.
pub fn icon_rgba(kind: IconKind, size: u32) -> Vec<u8> {
    let [r, g, b, a] = kind.rgba();
    let scale = size as f32 / 32.0;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            // Coordonnées ramenées à une grille 32 × 32, au centre du pixel.
            let px = (x as f32 + 0.5) / scale;
            let py = (y as f32 + 0.5) / scale;
            let tile =
                (0.5 - rounded_box(px - 16.0, py - 16.0, 15.0, 15.0, 7.0) * scale).clamp(0.0, 1.0);
            let white = (0.5 - headphones_distance(px, py) * scale).clamp(0.0, 1.0);
            let mix = |c: u8| (f32::from(c) + (255.0 - f32::from(c)) * white).round() as u8;
            let alpha = (f32::from(a) * tile).round() as u8;
            out.extend_from_slice(&[mix(r), mix(g), mix(b), alpha]);
        }
    }
    out
}

/// Distance signée (négative à l'intérieur) au casque, sur une grille 32.
fn headphones_distance(x: f32, y: f32) -> f32 {
    // Arceau : demi-anneau de rayon 8,5, épaisseur 3,2, centré en (16, 17).
    let (cx, cy) = (16.0, 17.0);
    let band = if y <= cy {
        (((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - 8.5).abs() - 1.6
    } else {
        f32::MAX
    };
    let cup_left = rounded_box(x - 8.3, y - 21.0, 2.6, 4.2, 1.8);
    let cup_right = rounded_box(x - 23.7, y - 21.0, 2.6, 4.2, 1.8);
    band.min(cup_left).min(cup_right)
}

/// Couleur de la pastille de batterie : rouge à 15 % et moins, orange à
/// 30 % et moins, vert au-dessus.
fn battery_color(level: u8) -> [u8; 3] {
    match level {
        0..=15 => [239, 68, 68],
        16..=30 => [245, 158, 11],
        _ => [34, 197, 94],
    }
}

/// Pastille de batterie (superposée à l'icône de la barre des tâches
/// Windows) : pile horizontale à contour sombre, remplie de `level` %
/// (0 à 100), en RGBA, `size` × `size`.
pub fn battery_badge_rgba(level: u8, size: u32) -> Vec<u8> {
    let level = level.min(100);
    let [r, g, b] = battery_color(level);
    let scale = size as f32 / 32.0;
    // Corps de 2 à 27 en x (centre 14,5), de 7 à 25 en y (centre 16) ;
    // borne de 27 à 30. Intérieur de 5 à 24 en x.
    let fill_right = 5.0 + 19.0 * f32::from(level) / 100.0;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let px = (x as f32 + 0.5) / scale;
            let py = (y as f32 + 0.5) / scale;
            let cover = |d: f32| (0.5 - d * scale).clamp(0.0, 1.0);
            let body = cover(rounded_box(px - 14.5, py - 16.0, 12.5, 9.0, 3.0));
            let tip = cover(rounded_box(px - 28.5, py - 16.0, 1.5, 4.0, 1.0));
            let inner = cover(rounded_box(px - 14.5, py - 16.0, 9.5, 6.0, 1.5));
            let fill = if px <= fill_right { inner } else { 0.0 };
            // Contour et vide : gris très sombre ; remplissage : couleur.
            let shape = body.max(tip);
            let mix = |c: u8| (30.0 + (f32::from(c) - 30.0) * fill).round() as u8;
            let alpha = (255.0 * shape).round() as u8;
            out.extend_from_slice(&[mix(r), mix(g), mix(b), alpha]);
        }
    }
    out
}

/// Distance signée à un rectangle arrondi centré à l'origine.
fn rounded_box(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = x.abs() - half_w + radius;
    let qy = y.abs() - half_h + radius;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

/// Position du panneau rapide (coin haut-gauche, pixels physiques) : centré
/// sur le clic, au-dessus si la place le permet (barre des tâches en bas),
/// sinon au-dessous, et toujours dans la zone de travail `(gauche, haut,
/// droite, bas)` de l'écran, à 8 pixels du bord.
pub fn panel_position(
    click: (i32, i32),
    panel: (i32, i32),
    work_area: (i32, i32, i32, i32),
) -> (i32, i32) {
    const MARGIN: i32 = 8;
    let (left, top, right, bottom) = work_area;
    let (width, height) = panel;
    let x = (click.0 - width / 2).clamp(left + MARGIN, (right - width - MARGIN).max(left + MARGIN));
    let above = click.1 - height - MARGIN;
    let y = if above >= top + MARGIN {
        above
    } else {
        click.1 + MARGIN
    };
    let y = y.clamp(top + MARGIN, (bottom - height - MARGIN).max(top + MARGIN));
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::NoiseState;

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    #[test]
    fn battery_badge_fill_and_colour_follow_the_level() {
        for size in [16, 32] {
            assert_eq!(
                battery_badge_rgba(50, size).len(),
                (size * size * 4) as usize
            );
        }
        let full = battery_badge_rgba(100, 32);
        let low = battery_badge_rgba(10, 32);
        let mid = battery_badge_rgba(25, 32);
        // Au centre, à droite de l'intérieur : rempli à 100 %, vide à 10 %.
        assert_eq!(pixel(&full, 32, 22, 16), [34, 197, 94, 255]);
        assert_eq!(pixel(&low, 32, 22, 16), [30, 30, 30, 255]);
        // Tout à gauche de l'intérieur : la couleur du niveau.
        assert_eq!(pixel(&low, 32, 6, 16), [239, 68, 68, 255]);
        assert_eq!(pixel(&mid, 32, 6, 16), [245, 158, 11, 255]);
        // Hors de la pile : transparent ; borne : opaque.
        assert_eq!(pixel(&full, 32, 16, 1)[3], 0);
        assert_eq!(pixel(&full, 32, 29, 16)[3], 255);
        // Au-delà de 100 : comme 100.
        assert_eq!(battery_badge_rgba(250, 16), battery_badge_rgba(100, 16));
    }

    #[test]
    fn panel_sits_next_to_the_tray_icon_inside_the_work_area() {
        let screen = (0, 0, 1920, 1032);
        // Barre des tâches en bas, icône près de l'horloge : au-dessus, sans
        // déborder à droite.
        assert_eq!(
            panel_position((1850, 1060), (360, 320), screen),
            (1552, 704)
        );
        // Barre des tâches en haut : au-dessous.
        let top_bar = (0, 48, 1920, 1080);
        assert_eq!(panel_position((1850, 20), (360, 320), top_bar), (1552, 56));
        // Clic au milieu : centré.
        assert_eq!(panel_position((960, 1060), (360, 320), screen), (780, 704));
    }

    fn state(anc: bool, adaptive: bool, level: u8) -> HeadsetState {
        HeadsetState {
            battery: 80,
            noise: NoiseState {
                anc_enabled: anc,
                adaptive,
                anti_wind: true,
                level,
            },
            spatial_audio: Some(true),
            ..HeadsetState::default()
        }
    }

    #[test]
    fn labels_and_tooltip() {
        let s = state(true, false, 0);
        let s = HeadsetState {
            audio_mode: Some(AudioMode::Equalizer),
            ..s
        };
        assert_eq!(
            tooltip(Some("MOMENTUM 5"), Some(&s), None),
            "MOMENTUM 5\nBatterie 80 %\nPersonnalisé : ANC maximal (0 %)"
        );
        let speech = HeadsetState {
            audio_mode: Some(AudioMode::SpeechClarity),
            ..s
        };
        assert!(tooltip(Some("M"), Some(&speech), None).ends_with("Mode audio : Speech Clarity"));
        assert!(tooltip(None, None, Some("connexion perdue")).ends_with("⚠ connexion perdue"));
        assert_eq!(
            mode_label(M5NoiseMode::Custom { level: 42 }),
            "Personnalisé : 42 %"
        );
        assert!(tooltip(None, None, None).contains("Déconnecté"));
        assert!(
            tooltip(Some(&"x".repeat(300)), None, Some("y"))
                .chars()
                .count()
                <= 127
        );
    }

    #[test]
    fn audio_mode_and_profile_stay_separate() {
        let s = HeadsetState {
            audio_mode: Some(AudioMode::Equalizer),
            sound_profile: Some(SoundProfileStatus::Received),
            ..HeadsetState::default()
        };
        assert_eq!(
            audio_summary(&s),
            "Mode audio : égaliseur · profil personnalisé : reçu"
        );
        assert_eq!(eq_mode_note(AudioMode::Equalizer), None);
        assert!(
            eq_mode_note(AudioMode::Unknown(9))
                .unwrap()
                .contains("valeur 9")
        );
        let unread = HeadsetState {
            audio_mode: Some(AudioMode::Unknown(5)),
            ..HeadsetState::default()
        };
        assert_eq!(
            audio_summary(&unread),
            "Mode audio : inconnu (5) · profil personnalisé : non lu"
        );
        assert_eq!(
            audio_summary(&HeadsetState::default()),
            "Mode audio : non lu · profil personnalisé : non lu"
        );
    }

    #[test]
    fn french_gain_format() {
        assert_eq!(format_db(-35), "−3,5 dB");
        assert_eq!(format_db(30), "+3 dB");
        assert_eq!(format_db(6), "+0,6 dB");
        assert_eq!(format_db(0), "0 dB");
        assert_eq!(format_db(-128), "−12,8 dB");
    }

    #[test]
    fn band_labels() {
        let labels: Vec<String> = (0..8).map(|i| band_label(i, 8)).collect();
        assert_eq!(
            labels,
            [
                "90 Hz", "325 Hz", "1,5 kHz", "2,5 kHz", "4,5 kHz", "8 kHz", "13 kHz", "16 kHz"
            ]
        );
        assert_eq!(band_label(2, 5), "Bande 3");
    }

    #[test]
    fn icon_kind_follows_the_mode() {
        assert_eq!(IconKind::from_state(None), IconKind::Disconnected);
        assert_eq!(
            IconKind::from_state(Some(&state(false, false, 0))),
            IconKind::Off
        );
        assert_eq!(
            IconKind::from_state(Some(&state(true, true, 0))),
            IconKind::Adaptive
        );
        assert_eq!(
            IconKind::from_state(Some(&state(true, false, 50))),
            IconKind::Anc
        );
        assert_eq!(
            IconKind::from_state(Some(&state(true, false, 75))),
            IconKind::Transparency
        );
    }

    #[test]
    fn icon_pixels() {
        let px = icon_rgba(IconKind::Anc, 32);
        assert_eq!(px.len(), 32 * 32 * 4);
        let pixel = |x: usize, y: usize| &px[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(pixel(8, 21), [255, 255, 255, 255], "écouteur gauche blanc");
        assert_eq!(
            pixel(16, 22),
            [59, 130, 246, 255],
            "fond bleu entre les écouteurs"
        );
        assert_eq!(pixel(0, 0)[3], 0, "coin arrondi transparent");
        assert_eq!(pixel(16, 8)[..3], [255, 255, 255], "haut de l'arceau blanc");
    }
}
