//! Scènes : ensembles nommés de réglages déjà validés, appliqués d'un coup.
//!
//! Fichier `scenes.txt` : un titre `[Nom]` par scène, puis des lignes
//! `réglage = valeur` ; `#` commence un commentaire. Liste fermée de
//! réglages (voir [`KEYS`]) : uniquement des écritures capturées dans l'app
//! puis validées sur le MOMENTUM 5. Priorité du mode audio, langue des
//! invites, profil auditif, connexions et nom en sont exclus.
//!
//! Une scène n'est pas une transaction : ses étapes partent une par une, dans
//! un ordre fixe, et l'application s'arrête à la première erreur. Le rapport
//! dit ce qui a été appliqué, ce qui a échoué et ce qui n'a pas été tenté.

use crate::device::{
    AudioMode, DeviceError, EqState, Headset, HeadsetState, NoiseModeRequest, ProfileSelection,
};
use crate::presets::Preset;
use crate::transport::Transport;

/// Longueur maximale d'un nom de scène.
pub const MAX_NAME_CHARS: usize = 32;

/// Réglages acceptés dans une scène, dans l'ordre d'application.
pub const KEYS: [&str; 8] = [
    "son",
    "spatial",
    "bruit",
    "niveau",
    "anti-vent",
    "tactile",
    "egaliseur",
    "basses",
];

/// Exemples écrits dans `scenes.txt` s'il n'existe pas encore.
pub const DEFAULT_SCENES: &str = "\
# Scènes de momentum-ctl-win : un titre [Nom] puis des lignes « réglage = valeur ».
# Réglages possibles :
#   son       = egaliseur | voix | profil   (voix = Speech Clarity)
#   spatial   = on | off                    (audio spatial Dolby Atmos)
#   bruit     = adaptatif | personnalise | arret
#   niveau    = 0 à 100                     (0 = ANC max, 100 = transparence max)
#   anti-vent = on | off
#   tactile   = on | off                    (commandes tactiles)
#   egaliseur = nom d'un préréglage         (exige son = egaliseur)
#   basses    = on | off                    (exige son = egaliseur)
# Les réglages s'appliquent un par un ; l'application s'arrête à la première erreur.

[Travail]
bruit = adaptatif
anti-vent = off

[Transport]
bruit = personnalise
niveau = 0
anti-vent = on

[Écoute]
son = egaliseur
egaliseur = Neutre
basses = off
bruit = adaptatif
";

/// Mode audio demandé par une scène.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSound {
    Equalizer,
    SpeechClarity,
    Profile,
}

/// Une étape, dans l'ordre d'application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Sound(SceneSound),
    Spatial(bool),
    Noise(NoiseModeRequest),
    Level(u8),
    AntiWind(bool),
    Touch(bool),
    Equalizer(String),
    BassBoost(bool),
}

impl Step {
    /// Libellé en français pour les rapports.
    pub fn label(&self) -> String {
        let on_off = |on: bool| if on { "activé" } else { "désactivé" };
        match self {
            Step::Sound(SceneSound::Equalizer) => "mode audio : égaliseur".to_string(),
            Step::Sound(SceneSound::SpeechClarity) => "mode audio : Speech Clarity".to_string(),
            Step::Sound(SceneSound::Profile) => "mode audio : profil personnalisé".to_string(),
            Step::Spatial(on) => format!("audio spatial {}", on_off(*on)),
            Step::Noise(NoiseModeRequest::Adaptive) => "bruit : adaptatif".to_string(),
            Step::Noise(NoiseModeRequest::Custom) => "bruit : personnalisé".to_string(),
            Step::Noise(NoiseModeRequest::Off) => "bruit : arrêt".to_string(),
            Step::Level(level) => format!("niveau ANC ↔ transparence : {level}"),
            Step::AntiWind(on) => format!("anti-vent {}", on_off(*on)),
            Step::Touch(on) => format!(
                "commandes tactiles {}",
                if *on { "actives" } else { "désactivées" }
            ),
            Step::Equalizer(name) => format!("égaliseur : « {name} »"),
            Step::BassBoost(on) => format!("amplification des basses {}", on_off(*on)),
        }
    }
}

/// Scène lue dans le fichier. Chaque réglage est facultatif ; au moins un
/// est présent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scene {
    pub name: String,
    pub sound: Option<SceneSound>,
    pub spatial: Option<bool>,
    pub noise: Option<NoiseModeRequest>,
    pub level: Option<u8>,
    pub anti_wind: Option<bool>,
    pub touch: Option<bool>,
    pub equalizer: Option<String>,
    pub bass_boost: Option<bool>,
}

/// Scène construite avec les seules lectures connues ; la courbe reste sur le PC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedScene {
    pub scene: Scene,
    pub preset: Option<Preset>,
}

/// Rien n'est enregistré si le nom ou les lectures ne permettent pas une scène valide.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("nom refusé : {0}")]
    Name(&'static str),
    #[error("état non enregistrable : {0}")]
    State(String),
    #[error("{0} existe déjà ; choisir un autre nom ou autoriser le remplacement")]
    Exists(String),
    #[error("{0}")]
    File(String),
    #[error("enregistrement impossible : {0}")]
    Io(#[from] std::io::Error),
}

/// Capture pure de l'état déjà relu par le service, sans accès au casque.
/// Les réglages latents (curseur hors mode personnalisé, courbe hors mode
/// égaliseur) sont omis pour ne pas changer de mode à la réapplication.
pub fn from_state(
    name: &str,
    state: &HeadsetState,
    eq: Option<&EqState>,
) -> Result<CapturedScene, CaptureError> {
    let name = validate_name(name).map_err(CaptureError::Name)?;
    let sound = match state.audio_mode {
        Some(AudioMode::Equalizer) => Some(SceneSound::Equalizer),
        Some(AudioMode::SpeechClarity) => Some(SceneSound::SpeechClarity),
        Some(AudioMode::Personalization) => Some(SceneSound::Profile),
        Some(AudioMode::Unknown(_)) | None => None,
    };
    let noise = if !state.noise.anc_enabled {
        NoiseModeRequest::Off
    } else if state.noise.adaptive {
        NoiseModeRequest::Adaptive
    } else {
        NoiseModeRequest::Custom
    };
    let mut scene = Scene {
        name: name.clone(),
        sound,
        spatial: state.spatial_audio,
        noise: Some(noise),
        level: (noise == NoiseModeRequest::Custom).then_some(state.noise.level),
        anti_wind: Some(state.noise.anti_wind),
        touch: state.touch_controls,
        ..Scene::default()
    };
    let preset = if sound == Some(SceneSound::Equalizer) {
        eq.map(|eq| {
            let name = crate::presets::validate_name(&name).map_err(CaptureError::Name)?;
            if eq.gains.is_empty() || eq.gains.iter().any(|&g| eq.check_gain(g).is_err()) {
                return Err(CaptureError::State(
                    "courbe d'égaliseur absente ou hors limites".into(),
                ));
            }
            scene.equalizer = Some(name.clone());
            scene.bass_boost = Some(eq.bass_boost);
            Ok(Preset {
                name,
                gains: eq.gains.clone(),
                builtin: false,
            })
        })
        .transpose()?
    } else {
        None
    };
    check_scene(&scene).map_err(CaptureError::State)?;
    // check_scene ne reçoit normalement que des valeurs déjà bornées par le formulaire.
    if scene.level.is_some_and(|level| level > 100) {
        return Err(CaptureError::State("niveau hors limites (0 à 100)".into()));
    }
    Ok(CapturedScene { scene, preset })
}

impl Scene {
    /// Étapes dans l'ordre fixe d'application. Un niveau sans mode de bruit
    /// impose le mode Personnalisé (le curseur n'agit que dans ce mode).
    pub fn steps(&self) -> Vec<Step> {
        let noise = self.noise.or(self.level.map(|_| NoiseModeRequest::Custom));
        [
            self.sound.map(Step::Sound),
            self.spatial.map(Step::Spatial),
            noise.map(Step::Noise),
            self.level.map(Step::Level),
            self.anti_wind.map(Step::AntiWind),
            self.touch.map(Step::Touch),
            self.equalizer.clone().map(Step::Equalizer),
            self.bass_boost.map(Step::BassBoost),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Règles de cohérence entre réglages (voir le module).
    fn check(&self) -> Result<(), &'static str> {
        if self.steps().is_empty() {
            return Err("scène vide");
        }
        if self.level.is_some() && matches!(self.noise, Some(n) if n != NoiseModeRequest::Custom) {
            return Err("« niveau » exige « bruit = personnalise » (ou pas de « bruit »)");
        }
        if (self.equalizer.is_some() || self.bass_boost.is_some())
            && self.sound != Some(SceneSound::Equalizer)
        {
            return Err("« egaliseur » et « basses » exigent « son = egaliseur » dans la scène");
        }
        if self.spatial == Some(true) && self.sound == Some(SceneSound::Profile) {
            return Err("« spatial = on » est incompatible avec « son = profil »");
        }
        Ok(())
    }
}

fn on_off(value: &str) -> Result<bool, &'static str> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("valeur attendue : on ou off"),
    }
}

/// Vérifie un nom de scène.
pub fn validate_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("nom de scène vide");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("nom de scène trop long (32 caractères au plus)");
    }
    if name.contains(['[', ']', '=', '#', '\n', '\r']) {
        return Err("caractères [ ] = # interdits dans un nom de scène");
    }
    Ok(name.to_string())
}

/// Ajoute un réglage à la scène en cours.
fn set_key(scene: &mut Scene, key: &str, value: &str) -> Result<(), String> {
    fn once<T>(slot: &mut Option<T>, value: T, key: &str) -> Result<(), String> {
        if slot.is_some() {
            return Err(format!("« {key} » apparaît deux fois"));
        }
        *slot = Some(value);
        Ok(())
    }
    let value = value.trim();
    match key {
        "son" => {
            let sound = match value {
                "egaliseur" => SceneSound::Equalizer,
                "voix" => SceneSound::SpeechClarity,
                "profil" => SceneSound::Profile,
                _ => return Err("son : egaliseur, voix ou profil".to_string()),
            };
            once(&mut scene.sound, sound, key)
        }
        "spatial" => once(&mut scene.spatial, on_off(value)?, key),
        "bruit" => {
            let mode = match value {
                "adaptatif" => NoiseModeRequest::Adaptive,
                "personnalise" => NoiseModeRequest::Custom,
                "arret" => NoiseModeRequest::Off,
                _ => return Err("bruit : adaptatif, personnalise ou arret".to_string()),
            };
            once(&mut scene.noise, mode, key)
        }
        "niveau" => match value.parse::<u8>() {
            Ok(level) if level <= 100 => once(&mut scene.level, level, key),
            _ => Err("niveau : nombre entier de 0 à 100".to_string()),
        },
        "anti-vent" => once(&mut scene.anti_wind, on_off(value)?, key),
        "tactile" => once(&mut scene.touch, on_off(value)?, key),
        "egaliseur" => {
            if value.is_empty() {
                return Err("egaliseur : nom de préréglage attendu".to_string());
            }
            once(&mut scene.equalizer, value.to_string(), key)
        }
        "basses" => once(&mut scene.bass_boost, on_off(value)?, key),
        _ => Err(format!(
            "réglage inconnu « {key} » (possibles : {})",
            KEYS.join(", ")
        )),
    }
}

/// Lit le fichier des scènes. Une scène qui contient une erreur est écartée
/// en entier ; les erreurs sont rendues avec leur numéro de ligne.
pub fn parse_scenes(text: &str) -> (Vec<Scene>, Vec<String>) {
    let mut scenes: Vec<Scene> = Vec::new();
    let mut errors = Vec::new();
    // Scène en cours, avec son numéro de ligne et son état (valide ou non).
    let mut current: Option<(usize, Scene, bool)> = None;

    let finish = |current: Option<(usize, Scene, bool)>,
                  scenes: &mut Vec<Scene>,
                  errors: &mut Vec<String>| {
        let Some((line, scene, valid)) = current else {
            return;
        };
        if !valid {
            return;
        }
        if let Err(e) = scene.check() {
            errors.push(format!("ligne {line} : scène « {} » : {e}", scene.name));
        } else if scenes
            .iter()
            .any(|s| s.name.to_lowercase() == scene.name.to_lowercase())
        {
            errors.push(format!(
                "ligne {line} : scène « {} » déjà définie plus haut",
                scene.name
            ));
        } else {
            scenes.push(scene);
        }
    };

    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(title) = line.strip_prefix('[') {
            finish(current.take(), &mut scenes, &mut errors);
            let parsed = title
                .strip_suffix(']')
                .ok_or("titre de scène attendu sous la forme [Nom]")
                .and_then(validate_name);
            current = Some(match parsed {
                Ok(name) => (
                    number,
                    Scene {
                        name,
                        ..Scene::default()
                    },
                    true,
                ),
                Err(e) => {
                    errors.push(format!("ligne {number} : {e}"));
                    (number, Scene::default(), false)
                }
            });
            continue;
        }
        let Some((_, scene, valid)) = current.as_mut() else {
            errors.push(format!(
                "ligne {number} : réglage hors d'une scène (ajouter un titre [Nom] avant)"
            ));
            continue;
        };
        let result = match line.split_once('=') {
            Some((key, value)) => set_key(scene, key.trim(), value),
            None => Err("ligne attendue : réglage = valeur".to_string()),
        };
        if let Err(e) = result {
            if *valid {
                errors.push(format!(
                    "ligne {number} : {e} (scène « {} » ignorée)",
                    scene.name
                ));
            }
            *valid = false;
        }
    }
    finish(current.take(), &mut scenes, &mut errors);
    (scenes, errors)
}

/// Vérifie une scène modifiée dans l'éditeur : nom et cohérence.
pub fn check_scene(scene: &Scene) -> Result<(), String> {
    let name = validate_name(&scene.name).map_err(str::to_string)?;
    if name != scene.name {
        return Err("espaces en trop au début ou à la fin du nom".to_string());
    }
    scene
        .check()
        .map_err(|e| format!("scène « {} » : {e}", scene.name))
}

/// Vérifie toutes les scènes avant l'écriture : chacune valide, noms
/// distincts (sans tenir compte de la casse).
pub fn check_scenes(scenes: &[Scene]) -> Result<(), String> {
    for (i, scene) in scenes.iter().enumerate() {
        check_scene(scene)?;
        if scenes[..i]
            .iter()
            .any(|s| s.name.to_lowercase() == scene.name.to_lowercase())
        {
            return Err(format!("deux scènes s'appellent « {} »", scene.name));
        }
    }
    Ok(())
}

/// Texte de `scenes.txt` pour ces scènes : le mode d'emploi des exemples,
/// puis une section par scène, réglages dans l'ordre de [`KEYS`]. Relu par
/// [`parse_scenes`], il redonne les mêmes scènes.
pub fn format_scenes(scenes: &[Scene]) -> String {
    let mut out: String = DEFAULT_SCENES
        .lines()
        .take_while(|l| l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    out.push_str("# Fichier écrit par l'éditeur de scènes (fenêtre, page Scènes).\n");
    let on_off = |b: bool| if b { "on" } else { "off" };
    for scene in scenes {
        out.push_str(&format!("\n[{}]\n", scene.name));
        let mut line = |key: &str, value: &str| out.push_str(&format!("{key} = {value}\n"));
        if let Some(sound) = scene.sound {
            line(
                "son",
                match sound {
                    SceneSound::Equalizer => "egaliseur",
                    SceneSound::SpeechClarity => "voix",
                    SceneSound::Profile => "profil",
                },
            );
        }
        if let Some(on) = scene.spatial {
            line("spatial", on_off(on));
        }
        if let Some(mode) = scene.noise {
            line(
                "bruit",
                match mode {
                    NoiseModeRequest::Adaptive => "adaptatif",
                    NoiseModeRequest::Custom => "personnalise",
                    NoiseModeRequest::Off => "arret",
                },
            );
        }
        if let Some(level) = scene.level {
            line("niveau", &level.to_string());
        }
        if let Some(on) = scene.anti_wind {
            line("anti-vent", on_off(on));
        }
        if let Some(on) = scene.touch {
            line("tactile", on_off(on));
        }
        if let Some(name) = &scene.equalizer {
            line("egaliseur", name);
        }
        if let Some(on) = scene.bass_boost {
            line("basses", on_off(on));
        }
    }
    out
}

/// Cherche une scène par son nom, sans tenir compte de la casse.
pub fn find<'a>(scenes: &'a [Scene], name: &str) -> Option<&'a Scene> {
    let name = name.trim().to_lowercase();
    scenes.iter().find(|s| s.name.to_lowercase() == name)
}

/// Résultat de l'application d'une scène.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SceneReport {
    pub scene: String,
    pub applied: Vec<String>,
    /// Étape en échec et raison.
    pub failed: Option<(String, String)>,
    /// Étapes non tentées après l'échec.
    pub skipped: Vec<String>,
}

impl SceneReport {
    /// Toutes les étapes ont été acquittées par le casque. Ce n'est pas une
    /// comparaison avec une relecture : l'appelant relit l'état ensuite.
    pub fn is_complete(&self) -> bool {
        self.failed.is_none()
    }

    /// Phrase courte pour une notification ou un bandeau.
    pub fn summary(&self) -> String {
        match &self.failed {
            None => format!("Scène « {} » appliquée.", self.scene),
            Some((step, reason)) => format!(
                "Scène « {} » appliquée en partie : {} sur {} réglages, échec sur « {step} » ({reason}).",
                self.scene,
                self.applied.len(),
                self.applied.len() + 1 + self.skipped.len(),
            ),
        }
    }

    /// Rapport détaillé, une ligne par étape.
    pub fn details(&self) -> String {
        let mut out = vec![self.summary()];
        out.extend(self.applied.iter().map(|s| format!("  ✓ {s}")));
        if let Some((step, reason)) = &self.failed {
            out.push(format!("  ✗ {step} : {reason}"));
        }
        out.extend(self.skipped.iter().map(|s| format!("  – {s} (non tenté)")));
        out.join("\n")
    }
}

/// Précision ajoutée au rapport quand le profil était déjà sélectionné.
pub const ALREADY_SELECTED_NOTE: &str = "déjà sélectionné à la lecture, rien n'a été écrit";

/// Applique une étape. `sound` est le mode audio fixé par la scène, s'il y
/// en a un. `Ok(Some(précision))` : étape déjà en place, sans écriture.
fn apply_step<T: Transport>(
    headset: &mut Headset<T>,
    step: &Step,
    sound: Option<SceneSound>,
    presets: &[Preset],
) -> Result<Option<&'static str>, String> {
    let err = |e: DeviceError| e.to_string();
    let written = match step {
        Step::Sound(SceneSound::Equalizer) => headset.set_speech_clarity(false).map_err(err),
        Step::Sound(SceneSound::SpeechClarity) => headset.set_speech_clarity(true).map_err(err),
        Step::Sound(SceneSound::Profile) => match headset.select_sound_profile() {
            Ok(ProfileSelection::AlreadySelectedAtRead) => return Ok(Some(ALREADY_SELECTED_NOTE)),
            Ok(ProfileSelection::WriteAcknowledged) => Ok(()),
            Err(e) => Err(err(e)),
        },
        Step::Spatial(true) => {
            // Le profil personnalisé exclut Dolby Atmos (comme dans l'app) :
            // on n'envoie rien plutôt que de laisser le casque trancher.
            if sound.is_none() && headset.audio_mode().map_err(err)? == AudioMode::Personalization {
                return Err(
                    "le profil personnalisé est sélectionné ; ajouter « son = egaliseur » ou « son = voix »"
                        .to_string(),
                );
            }
            headset.set_spatial_audio(true).map_err(err)
        }
        Step::Spatial(false) => headset.set_spatial_audio(false).map_err(err),
        Step::Noise(mode) => headset.set_mode(*mode).map_err(err),
        Step::Level(level) => headset.set_level(*level).map_err(err),
        Step::AntiWind(on) => headset.set_anti_wind(*on).map_err(err),
        Step::Touch(on) => headset.set_touch_controls(*on).map_err(err),
        Step::Equalizer(name) => {
            let preset = presets
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("préréglage « {name} » introuvable"))?;
            let mut eq = headset.read_eq().map_err(err)?;
            headset.set_eq_gains(&mut eq, &preset.gains).map_err(err)
        }
        Step::BassBoost(on) => headset.set_bass_boost(*on).map_err(err),
    };
    written.map(|()| None)
}

/// Applique une scène étape par étape et s'arrête à la première erreur.
/// L'appelant relit ensuite l'état du casque (et, si la session est
/// désynchronisée, la rouvre).
pub fn apply_scene<T: Transport>(
    headset: &mut Headset<T>,
    scene: &Scene,
    presets: &[Preset],
) -> SceneReport {
    let mut report = SceneReport {
        scene: scene.name.clone(),
        applied: Vec::new(),
        failed: None,
        skipped: Vec::new(),
    };
    for step in scene.steps() {
        if report.failed.is_some() {
            report.skipped.push(step.label());
            continue;
        }
        match apply_step(headset, &step, scene.sound, presets) {
            Ok(None) => report.applied.push(step.label()),
            Ok(Some(note)) => report.applied.push(format!("{} ({note})", step.label())),
            Err(reason) => report.failed = Some((step.label(), reason)),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::from_hex;
    use crate::presets::builtin_presets;
    use crate::transport::MockTransport;
    use std::time::Duration;

    const T: Duration = Duration::from_millis(50);

    fn h(s: &str) -> Vec<u8> {
        from_hex(s).unwrap()
    }

    #[test]
    fn default_scenes_are_valid() {
        let (scenes, errors) = parse_scenes(DEFAULT_SCENES);
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<&str> = scenes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Travail", "Transport", "Écoute"]);
    }

    #[test]
    fn steps_follow_a_fixed_order_whatever_the_file_order() {
        let (scenes, errors) = parse_scenes(
            "[Jeu]\nbasses = on\nanti-vent = off\nniveau = 40\nson = egaliseur\nspatial = on\n",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            scenes[0].steps(),
            [
                Step::Sound(SceneSound::Equalizer),
                Step::Spatial(true),
                // Le niveau impose le mode Personnalisé.
                Step::Noise(NoiseModeRequest::Custom),
                Step::Level(40),
                Step::AntiWind(false),
                Step::BassBoost(true),
            ]
        );
    }

    #[test]
    fn invalid_scenes_are_dropped_with_their_line() {
        let text = "\
bruit = arret
[Ok]
tactile = off
[Inconnu]
volume = 10
[Doublon]
bruit = arret
bruit = adaptatif
[Niveau]
bruit = arret
niveau = 10
[Eq]
egaliseur = Neutre
[Atmos]
son = profil
spatial = on
[Vide]
[ok]
tactile = on
[Mauvais
";
        let (scenes, errors) = parse_scenes(text);
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].name, "Ok");
        let joined = errors.join("\n");
        for expected in [
            "ligne 1 : réglage hors d'une scène",
            "ligne 5 : réglage inconnu « volume »",
            "ligne 8 : « bruit » apparaît deux fois",
            "ligne 9 : scène « Niveau » : « niveau » exige",
            "ligne 12 : scène « Eq » : « egaliseur » et « basses » exigent",
            "ligne 14 : scène « Atmos » : « spatial = on » est incompatible",
            "ligne 17 : scène « Vide » : scène vide",
            "ligne 18 : scène « ok » déjà définie",
            "ligne 20 : titre de scène attendu",
        ] {
            assert!(
                joined.contains(expected),
                "manque « {expected} » dans\n{joined}"
            );
        }
        assert_eq!(errors.len(), 9, "{joined}");
        assert!(find(&scenes, " OK ").is_some());
    }

    #[test]
    fn application_stops_at_the_first_error_and_reports_the_rest() {
        let (scenes, _) =
            parse_scenes("[Transport]\nbruit = personnalise\nniveau = 0\nanti-vent = on\n");
        // Mode Personnalisé (séquence de l'app), puis le niveau est refusé
        // par le casque : l'anti-vent n'est pas envoyé.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 1A 00 03 00"),
                &h("FF 03 00 00 04 95 1B 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 1A 04 01"),
                &h("FF 03 00 00 04 95 1B 04"),
            )
            .expect(
                &h("FF 03 00 01 04 95 1A 02 00"),
                &h("FF 03 00 01 04 95 1B 82 05"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let report = apply_scene(&mut headset, &scenes[0], &[]);
        assert!(!report.is_complete());
        assert_eq!(report.applied, ["bruit : personnalisé"]);
        assert_eq!(
            report.failed.as_ref().map(|f| f.0.as_str()),
            Some("niveau ANC ↔ transparence : 0")
        );
        assert_eq!(report.skipped, ["anti-vent activé"]);
        assert!(report.summary().contains("1 sur 3 réglages"));
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn spatial_is_not_sent_while_the_profile_is_selected() {
        let (scenes, _) = parse_scenes("[Film]\nspatial = on\n");
        // Mode audio lu : profil personnalisé (03) ; rien n'est écrit.
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 08 04"),
            &h("FF 03 00 02 04 95 09 04 00 03"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        let report = apply_scene(&mut headset, &scenes[0], &[]);
        assert!(report.failed.unwrap().1.contains("profil personnalisé"));
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn unknown_preset_fails_before_any_band_is_written() {
        let (scenes, _) = parse_scenes("[Eq]\nson = egaliseur\negaliseur = Inexistant\n");
        let mock = MockTransport::new().expect(
            &h("FF 03 00 02 04 95 08 03 00 01"),
            &h("FF 03 00 00 04 95 09 03"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        let report = apply_scene(&mut headset, &scenes[0], &builtin_presets(8));
        assert_eq!(report.applied, ["mode audio : égaliseur"]);
        assert!(report.failed.unwrap().1.contains("introuvable"));
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn formatted_scenes_read_back_identically() {
        let (scenes, errors) = parse_scenes(DEFAULT_SCENES);
        assert!(errors.is_empty());
        let mut all = scenes.clone();
        all.push(Scene {
            name: "Tout".into(),
            sound: Some(SceneSound::Equalizer),
            spatial: Some(false),
            noise: Some(NoiseModeRequest::Custom),
            level: Some(35),
            anti_wind: Some(true),
            touch: Some(false),
            equalizer: Some("Mon préréglage".into()),
            bass_boost: Some(true),
        });
        let text = format_scenes(&all);
        assert!(text.starts_with("# Scènes de momentum-ctl-win"));
        let (again, errors) = parse_scenes(&text);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(again, all);
    }

    #[test]
    fn editor_checks_names_and_rules() {
        let ok = Scene {
            name: "Calme".into(),
            noise: Some(NoiseModeRequest::Adaptive),
            ..Scene::default()
        };
        assert!(check_scenes(std::slice::from_ref(&ok)).is_ok());
        let twice = [
            ok.clone(),
            Scene {
                name: "CALME".into(),
                ..ok.clone()
            },
        ];
        assert!(check_scenes(&twice).unwrap_err().contains("deux scènes"));
        let empty = Scene {
            name: "Vide".into(),
            ..Scene::default()
        };
        assert!(check_scene(&empty).unwrap_err().contains("scène vide"));
        let bad_name = Scene {
            name: "[x]".into(),
            ..ok.clone()
        };
        assert!(check_scene(&bad_name).is_err());
        let eq_without_sound = Scene {
            name: "Eq".into(),
            equalizer: Some("Neutre".into()),
            ..Scene::default()
        };
        assert!(
            check_scene(&eq_without_sound)
                .unwrap_err()
                .contains("son = egaliseur")
        );
    }

    #[test]
    fn an_already_selected_profile_is_reported_as_such() {
        // 2001 → profil reçu, 3605 → spatial coupé, 0804 → profil déjà actif :
        // aucune écriture, et le rapport le dit.
        let mock = MockTransport::new()
            .expect(
                &from_hex("FF 03 00 00 04 95 20 01").unwrap(),
                &from_hex("FF 03 00 01 04 95 21 01 02").unwrap(),
            )
            .expect(
                &from_hex("FF 03 00 00 04 95 36 05").unwrap(),
                &from_hex("FF 03 00 01 04 95 37 05 00").unwrap(),
            )
            .expect(
                &from_hex("FF 03 00 00 04 95 08 04").unwrap(),
                &from_hex("FF 03 00 02 04 95 09 04 00 03").unwrap(),
            );
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        let scene = Scene {
            name: "Profil".into(),
            sound: Some(SceneSound::Profile),
            ..Scene::default()
        };
        let report = apply_scene(&mut headset, &scene, &[]);
        assert!(report.is_complete());
        assert_eq!(
            report.applied,
            [format!(
                "mode audio : profil personnalisé ({ALREADY_SELECTED_NOTE})"
            )]
        );
        assert!(headset.client().transport().is_exhausted());
    }
}
