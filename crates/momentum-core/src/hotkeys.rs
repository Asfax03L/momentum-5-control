//! Raccourcis clavier globaux : analyse de la configuration, en Rust pur.
//!
//! Fichier `raccourcis.txt` : une ligne `action = touches` par raccourci,
//! par exemple `bruit = Ctrl+Alt+N`, ou `scene:Transport = Ctrl+Alt+1`
//! pour une scène. Une valeur vide désactive l'action. La fenêtre réécrit
//! ce fichier (page Réglages) avec [`build_config`].
//! L'enregistrement auprès de Windows (`RegisterHotKey`) est fait par
//! l'application de la barre système.

use crate::device::{AudioMode, HeadsetState, NoiseModeRequest, SoundProfileStatus};
use crate::i18n::{tr, trf};
use crate::protocol::messages::M5NoiseMode;

/// Actions déclenchables au clavier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Mode de bruit suivant : Adaptatif → Personnalisé → Arrêt → …
    CycleNoise,
    /// Profil personnalisé sélectionné ou retour à l'égaliseur.
    ToggleProfile,
    /// Ouvrir la fenêtre principale.
    OpenWindow,
    /// Appliquer la scène de ce nom (voir `scenes.txt`).
    Scene(String),
}

/// Préfixe des actions de scène dans le fichier.
const SCENE_PREFIX: &str = "scene:";

impl HotkeyAction {
    /// Actions fixes (les scènes s'y ajoutent).
    pub const BUILTIN: [HotkeyAction; 3] = [
        HotkeyAction::CycleNoise,
        HotkeyAction::ToggleProfile,
        HotkeyAction::OpenWindow,
    ];

    /// Nom dans le fichier de configuration.
    pub fn key(&self) -> String {
        match self {
            HotkeyAction::CycleNoise => "bruit".to_string(),
            HotkeyAction::ToggleProfile => "profil".to_string(),
            HotkeyAction::OpenWindow => "fenetre".to_string(),
            HotkeyAction::Scene(name) => format!("{SCENE_PREFIX}{name}"),
        }
    }

    pub fn label(&self) -> String {
        match self {
            HotkeyAction::CycleNoise => tr("Mode de bruit suivant").to_string(),
            HotkeyAction::ToggleProfile => tr("Profil personnalisé / égaliseur").to_string(),
            HotkeyAction::OpenWindow => tr("Ouvrir la fenêtre").to_string(),
            HotkeyAction::Scene(name) => trf("Scène « {} »", &[name]),
        }
    }

    /// Action d'après son nom dans le fichier.
    pub fn from_key(key: &str) -> Option<HotkeyAction> {
        let key = key.trim();
        if let Some(name) = key.strip_prefix(SCENE_PREFIX) {
            return crate::scenes::validate_name(name)
                .ok()
                .map(HotkeyAction::Scene);
        }
        Self::BUILTIN.into_iter().find(|a| a.key() == key)
    }
}

/// Combinaison de touches : modificateurs et code de touche virtuelle
/// Windows (`VK_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u32,
}

/// Analyse « Ctrl+Alt+N », « Ctrl+Shift+F9 »… Au moins un modificateur
/// est exigé, pour ne pas voler une touche seule au reste de Windows.
pub fn parse_hotkey(spec: &str) -> Result<Hotkey, String> {
    let mut key = Hotkey {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
        vk: 0,
    };
    for part in spec.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => key.ctrl = true,
            "alt" => key.alt = true,
            "shift" | "maj" => key.shift = true,
            "win" => key.win = true,
            other => {
                if key.vk != 0 {
                    return Err(trf("plusieurs touches dans « {} »", &[&spec]));
                }
                key.vk =
                    virtual_key(other).ok_or_else(|| trf("touche inconnue « {} »", &[&part]))?;
            }
        }
    }
    if key.vk == 0 {
        return Err(trf("aucune touche dans « {} »", &[&spec]));
    }
    if !(key.ctrl || key.alt || key.win) {
        return Err(trf("« {} » : Ctrl, Alt ou Win obligatoire", &[&spec]));
    }
    Ok(key)
}

/// Texte d'un raccourci, dans la forme acceptée par [`parse_hotkey`].
pub fn format_hotkey(key: &Hotkey) -> String {
    let mut parts: Vec<String> = [
        (key.ctrl, "Ctrl"),
        (key.alt, "Alt"),
        (key.shift, "Shift"),
        (key.win, "Win"),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name.to_string())
    .collect();
    parts.push(match key.vk {
        0x70..=0x87 => format!("F{}", key.vk - 0x70 + 1),
        vk => char::from_u32(vk).map_or(format!("#{vk}"), |c| c.to_string()),
    });
    parts.join("+")
}

/// Lettres, chiffres et F1 à F24.
fn virtual_key(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    match bytes {
        [c] if c.is_ascii_uppercase() || c.is_ascii_digit() => Some(u32::from(*c)),
        [b'F', rest @ ..] if !rest.is_empty() => {
            let n: u32 = std::str::from_utf8(rest).ok()?.parse().ok()?;
            (1..=24).contains(&n).then_some(0x70 + n - 1)
        }
        _ => None,
    }
}

/// Configuration écrite au premier lancement.
pub const DEFAULT_CONFIG: &str = "\
# Raccourcis clavier globaux d'OpenHeadset Control : « action = touches ».
# Touches : Ctrl, Alt, Shift, Win et une lettre, un chiffre ou F1 à F24.
# Scène : « scene:Nom = touches ». Laisser vide pour désactiver.
# Modifiable aussi dans la fenêtre (page Réglages).
bruit = Ctrl+Alt+N
profil = Ctrl+Alt+P
fenetre = Ctrl+Alt+M
";

/// Lit la configuration : raccourcis valides, et erreurs à signaler.
pub fn parse_config(text: &str) -> (Vec<(HotkeyAction, Hotkey)>, Vec<String>) {
    let mut keys = Vec::new();
    let mut errors = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, spec)) = line.split_once('=') else {
            errors.push(format!("ligne ignorée : « {line} »"));
            continue;
        };
        let Some(action) = HotkeyAction::from_key(name) else {
            errors.push(trf("action inconnue : « {} »", &[&(name.trim())]));
            continue;
        };
        if spec.trim().is_empty() {
            keys.retain(|(a, _)| *a != action);
            continue;
        }
        match parse_hotkey(spec) {
            Ok(key) => {
                keys.retain(|(a, _)| *a != action);
                keys.push((action, key));
            }
            Err(e) => errors.push(e),
        }
    }
    (keys, errors)
}

/// Construit `raccourcis.txt` depuis l'éditeur de la fenêtre : `(action,
/// touches)` avec des touches vides pour une action désactivée. Refuse une
/// combinaison invalide ou utilisée deux fois.
pub fn build_config(entries: &[(HotkeyAction, String)]) -> Result<String, String> {
    let mut used: Vec<(Hotkey, String)> = Vec::new();
    let mut out = String::from(
        "# Raccourcis clavier globaux d'OpenHeadset Control : « action = touches ».\n\
         # Touches : Ctrl, Alt, Shift, Win et une lettre, un chiffre ou F1 à F24.\n\
         # Scène : « scene:Nom = touches ». Laisser vide pour désactiver.\n\
         # Modifiable aussi dans la fenêtre (page Réglages).\n",
    );
    for (action, spec) in entries {
        let spec = spec.trim();
        if spec.is_empty() {
            // Action fixe désactivée : ligne vide (sinon la valeur par
            // défaut reviendrait). Scène sans raccourci : rien à écrire.
            if !matches!(action, HotkeyAction::Scene(_)) {
                out.push_str(&format!("{} =\n", action.key()));
            }
            continue;
        }
        let key = parse_hotkey(spec).map_err(|e| format!("{} : {e}", action.label()))?;
        if let Some((_, other)) = used.iter().find(|(k, _)| *k == key) {
            return Err(trf(
                "{} et {} utilisent tous deux {}",
                &[&other, &(action.label()), &(format_hotkey(&key))],
            ));
        }
        used.push((key, action.label()));
        out.push_str(&format!("{} = {}\n", action.key(), format_hotkey(&key)));
    }
    Ok(out)
}

/// Lignes de l'éditeur des raccourcis : actions fixes puis une par scène,
/// avec les touches lues dans `config` (vide : pas de raccourci).
pub fn editor_rows(config: &str, scenes: &[String]) -> Vec<(HotkeyAction, String)> {
    let (keys, _) = parse_config(config);
    HotkeyAction::BUILTIN
        .iter()
        .cloned()
        .chain(scenes.iter().map(|name| HotkeyAction::Scene(name.clone())))
        .map(|action| {
            let spec = keys
                .iter()
                .find(|(a, _)| *a == action)
                .map(|(_, k)| format_hotkey(k))
                .unwrap_or_default();
            (action, spec)
        })
        .collect()
}

/// Suit un renommage (`Some(nouveau)`) ou une suppression (`None`) de scène
/// dans le texte de `raccourcis.txt` ; les autres lignes restent telles
/// quelles.
pub fn rename_scene(text: &str, old: &str, new: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let target =
            line.split_once('=')
                .and_then(|(name, spec)| match HotkeyAction::from_key(name) {
                    Some(HotkeyAction::Scene(n)) if n.to_lowercase() == old.to_lowercase() => {
                        Some(spec)
                    }
                    _ => None,
                });
        match (target, new) {
            (Some(spec), Some(new)) => {
                out.push_str(&format!("{SCENE_PREFIX}{new} ={spec}\n"));
            }
            (Some(_), None) => {}
            (None, _) => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// Mode de bruit suivant pour le raccourci « bruit ».
pub fn next_noise_mode(current: M5NoiseMode) -> NoiseModeRequest {
    match current {
        M5NoiseMode::Adaptive => NoiseModeRequest::Custom,
        M5NoiseMode::Custom { .. } => NoiseModeRequest::Off,
        M5NoiseMode::Off => NoiseModeRequest::Adaptive,
    }
}

/// Raccourci « profil » : `Some(true)` sélectionne le profil personnalisé
/// (s'il a été reçu par le casque), `Some(false)` revient à l'égaliseur,
/// `None` : rien à faire.
pub fn profile_toggle(state: &HeadsetState) -> Option<bool> {
    if state.audio_mode == Some(AudioMode::Personalization) {
        Some(false)
    } else if state.sound_profile == Some(SoundProfileStatus::Received) {
        Some(true)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_rows_list_builtins_then_scenes_with_their_keys() {
        let config = "bruit = Ctrl+Alt+N\nprofil =\nscene:Travail = Ctrl+Alt+1\n";
        let rows = editor_rows(config, &["Travail".to_string(), "Écoute".to_string()]);
        let specs: Vec<(String, String)> = rows.iter().map(|(a, s)| (a.key(), s.clone())).collect();
        assert_eq!(
            specs,
            [
                ("bruit".to_string(), "Ctrl+Alt+N".to_string()),
                ("profil".to_string(), String::new()),
                ("fenetre".to_string(), String::new()),
                ("scene:Travail".to_string(), "Ctrl+Alt+1".to_string()),
                ("scene:Écoute".to_string(), String::new()),
            ]
        );
        // Ce que l'éditeur écrit se relit à l'identique.
        let text = build_config(&rows).unwrap();
        assert_eq!(
            editor_rows(&text, &["Travail".to_string(), "Écoute".to_string()]),
            rows
        );
    }

    #[test]
    fn profile_toggle_needs_a_received_profile() {
        let mut s = HeadsetState::default();
        assert_eq!(profile_toggle(&s), None);
        s.sound_profile = Some(SoundProfileStatus::Received);
        assert_eq!(profile_toggle(&s), Some(true));
        s.audio_mode = Some(AudioMode::Personalization);
        assert_eq!(profile_toggle(&s), Some(false));
    }

    #[test]
    fn parses_combinations() {
        let k = parse_hotkey("Ctrl+Alt+N").unwrap();
        assert!(k.ctrl && k.alt && !k.shift && !k.win);
        assert_eq!(k.vk, u32::from(b'N'));
        assert_eq!(parse_hotkey(" ctrl + shift + f9 ").unwrap().vk, 0x78);
        assert_eq!(parse_hotkey("Win+1").unwrap().vk, u32::from(b'1'));
        assert!(parse_hotkey("N").is_err());
        assert!(parse_hotkey("Shift+N").is_err());
        assert!(parse_hotkey("Ctrl+Alt").is_err());
        assert!(parse_hotkey("Ctrl+A+B").is_err());
        assert!(parse_hotkey("Ctrl+F25").is_err());
        assert!(parse_hotkey("Ctrl+Échap").is_err());
    }

    #[test]
    fn default_config_is_valid() {
        let (keys, errors) = parse_config(DEFAULT_CONFIG);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn config_overrides_disables_and_reports() {
        let (keys, errors) = parse_config(
            "bruit = Ctrl+Alt+N\nbruit = Ctrl+Alt+B\nprofil =\nvolume = Ctrl+V\nfenetre = M\n",
        );
        assert_eq!(
            keys,
            [(
                HotkeyAction::CycleNoise,
                parse_hotkey("Ctrl+Alt+B").unwrap()
            )]
        );
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn scene_hotkeys_and_editor_round_trip() {
        let (keys, errors) = parse_config(
            "scene:Transport = Ctrl+Alt+1\nscene:[x] = Ctrl+Alt+2\nbruit = Ctrl+Alt+N\n",
        );
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(keys[0].0, HotkeyAction::Scene("Transport".to_string()));
        assert_eq!(format_hotkey(&keys[0].1), "Ctrl+Alt+1");
        assert_eq!(
            format_hotkey(&parse_hotkey("win+shift+f9").unwrap()),
            "Shift+Win+F9"
        );

        let text = build_config(&[
            (HotkeyAction::CycleNoise, "ctrl+alt+n".to_string()),
            (HotkeyAction::ToggleProfile, String::new()),
            (
                HotkeyAction::Scene("Travail".to_string()),
                "Ctrl+Alt+1".to_string(),
            ),
            (HotkeyAction::Scene("Jeu".to_string()), String::new()),
        ])
        .unwrap();
        let (keys, errors) = parse_config(&text);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(keys.len(), 2);
        assert!(text.contains("profil =\n"));
        // Scène sans raccourci : aucune ligne (lisible par les versions
        // précédentes de l'icône).
        assert!(!text.contains("scene:Jeu"));

        let err = build_config(&[
            (HotkeyAction::CycleNoise, "Ctrl+Alt+1".to_string()),
            (
                HotkeyAction::Scene("Jeu".to_string()),
                "ctrl+alt+1".to_string(),
            ),
        ])
        .unwrap_err();
        assert!(err.contains("utilisent tous deux Ctrl+Alt+1"), "{err}");
        assert!(build_config(&[(HotkeyAction::OpenWindow, "M".to_string())]).is_err());
    }

    #[test]
    fn noise_cycle() {
        assert_eq!(
            next_noise_mode(M5NoiseMode::Adaptive),
            NoiseModeRequest::Custom
        );
        assert_eq!(
            next_noise_mode(M5NoiseMode::Custom { level: 50 }),
            NoiseModeRequest::Off
        );
        assert_eq!(
            next_noise_mode(M5NoiseMode::Off),
            NoiseModeRequest::Adaptive
        );
    }

    #[test]
    fn scene_renames_and_removals_follow_in_the_config() {
        let text = "bruit = Ctrl+Alt+N\nscene:Travail = Ctrl+Alt+1\nscene:Transport = Ctrl+Alt+2\n";
        let renamed = rename_scene(text, "travail", Some("Bureau"));
        assert_eq!(
            renamed,
            "bruit = Ctrl+Alt+N\nscene:Bureau = Ctrl+Alt+1\nscene:Transport = Ctrl+Alt+2\n"
        );
        let (keys, errors) = parse_config(&renamed);
        assert!(errors.is_empty());
        assert!(
            keys.iter()
                .any(|(a, _)| *a == HotkeyAction::Scene("Bureau".into()))
        );
        let removed = rename_scene(text, "Transport", None);
        assert_eq!(removed, "bruit = Ctrl+Alt+N\nscene:Travail = Ctrl+Alt+1\n");
    }
}
