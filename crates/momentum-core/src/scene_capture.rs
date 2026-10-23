//! Enregistrement local d'une scène capturée, sans commande au service ou au casque.

use std::fs;
use std::path::Path;

use crate::device::{EqState, HeadsetState};
use crate::paths;
use crate::presets::Preset;
use crate::scenes::{self, CaptureError, CapturedScene, Scene};

/// Construit et enregistre une scène à partir du dernier état connu du service.
/// Les deux textes sont préparés avant toute écriture ; les fichiers remplacés
/// gardent leur sauvegarde habituelle `.bak`.
pub fn save_current(
    name: &str,
    state: &HeadsetState,
    eq: Option<&EqState>,
    replace: bool,
) -> Result<Scene, CaptureError> {
    let captured = scenes::from_state(name, state, eq)?;
    let dir = paths::data_dir()
        .ok_or_else(|| CaptureError::File("dossier des données introuvable".into()))?;
    save_in(&dir, &captured, replace)?;
    Ok(captured.scene)
}

fn read_optional(path: &Path) -> Result<Option<String>, CaptureError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Ajoute une section ou remplace seulement ses réglages. Les autres sections,
/// et les lignes de commentaire sont conservées.
pub fn update_scene_text(text: &str, scene: &Scene, replace: bool) -> Result<String, CaptureError> {
    scenes::check_scene(scene).map_err(CaptureError::State)?;
    let (list, errors) = scenes::parse_scenes(text);
    if !errors.is_empty() {
        return Err(CaptureError::File(format!(
            "scenes.txt invalide : {}",
            errors.join(" ; ")
        )));
    }
    let existing = scenes::find(&list, &scene.name);
    if existing.is_some() && !replace {
        return Err(CaptureError::Exists(format!("la scène « {} »", scene.name)));
    }
    let formatted = scenes::format_scenes(std::slice::from_ref(scene));
    let (_, errors) = scenes::parse_scenes(&formatted);
    if !errors.is_empty() {
        return Err(CaptureError::State(errors.join(" ; ")));
    }
    let mut lines = formatted.lines().skip_while(|line| !line.starts_with('['));
    let title = lines
        .next()
        .ok_or_else(|| CaptureError::State("scène vide".into()))?;
    let mut values: Vec<(&str, &str)> = lines.filter_map(|line| line.split_once('=')).collect();
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut start = None;
    let mut end = text.len();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.split('#').next().unwrap_or("").trim();
        if let Some(name) = content.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if start.is_some() {
                end = offset;
                break;
            }
            if existing.is_some_and(|s| s.name == name.trim()) {
                start = Some(offset);
            }
        }
        offset += line.len();
    }
    let Some(start) = start else {
        let mut out = text.to_string();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(newline);
        }
        out.push_str(newline);
        out.push_str(
            &formatted
                .lines()
                .skip_while(|l| !l.starts_with('['))
                .collect::<Vec<_>>()
                .join(newline),
        );
        out.push_str(newline);
        return Ok(out);
    };
    let mut out = text[..start].to_string();
    for (index, line) in text[start..end].split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\r', '\n']);
        if index == 0 {
            out.push_str(title);
            out.push_str(newline);
        } else if body.trim().is_empty() || body.trim_start().starts_with('#') {
            out.push_str(line);
        } else {
            let key = body.split('=').next().unwrap_or("").trim();
            if let Some(index) = values.iter().position(|(k, _)| k.trim() == key) {
                let (key, value) = values.remove(index);
                out.push_str(&format!("{} = {}", key.trim(), value.trim()));
                out.push_str(newline);
            }
        }
    }
    if !out.ends_with('\n') {
        out.push_str(newline);
    }
    for (key, value) in values {
        out.push_str(&format!("{} = {}{newline}", key.trim(), value.trim()));
    }
    out.push_str(&text[end..]);
    Ok(out)
}

fn update_preset_text(text: &str, preset: &Preset, replace: bool) -> Result<String, CaptureError> {
    let matches = |line: &str| {
        line.split('#')
            .next()
            .unwrap_or("")
            .split_once('=')
            .is_some_and(|(name, _)| name.trim().to_lowercase() == preset.name.to_lowercase())
    };
    let count = text.lines().filter(|line| matches(line)).count();
    if count > 0 && !replace {
        return Err(CaptureError::Exists(format!(
            "le préréglage « {} »",
            preset.name
        )));
    }
    if count > 1 {
        return Err(CaptureError::File(
            "plusieurs préréglages portent ce nom dans prereglages.txt".into(),
        ));
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let value = format!(
        "{} = {}",
        preset.name,
        preset
            .gains
            .iter()
            .map(i8::to_string)
            .collect::<Vec<_>>()
            .join(";")
    );
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        if matches(line) {
            out.push_str(&value);
            out.push_str(newline);
            if let Some((_, comment)) = line.trim_end_matches(['\r', '\n']).split_once('#') {
                out.push('#');
                out.push_str(comment);
                out.push_str(newline);
            }
        } else {
            out.push_str(line);
        }
    }
    if count == 0 {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push_str(newline);
        }
        out.push_str(&value);
        out.push_str(newline);
    }
    Ok(out)
}

fn save_in(dir: &Path, captured: &CapturedScene, replace: bool) -> Result<(), CaptureError> {
    let scene_path = dir.join("scenes.txt");
    let preset_path = dir.join("prereglages.txt");
    let before = read_optional(&scene_path)?;
    let text = update_scene_text(
        before.as_deref().unwrap_or(scenes::DEFAULT_SCENES),
        &captured.scene,
        replace,
    )?;
    let preset_before = if captured.preset.is_some() {
        read_optional(&preset_path)?
    } else {
        None
    };
    let preset_text = captured
        .preset
        .as_ref()
        .map(|preset| update_preset_text(preset_before.as_deref().unwrap_or(""), preset, replace))
        .transpose()?;
    if let Some(preset_text) = &preset_text {
        paths::write_if_unchanged(&preset_path, preset_before.as_deref(), preset_text)
            .map_err(CaptureError::File)?;
    }
    if let Err(error) = paths::write_if_unchanged(&scene_path, before.as_deref(), &text) {
        // Une erreur sur le deuxième fichier ne doit pas modifier la courbe
        // existante. Une modification concurrente n'est jamais restaurée par-dessus.
        if let Some(preset_text) = &preset_text {
            let restored = match read_optional(&preset_path) {
                Ok(current) if current.as_deref() == Some(preset_text) => match &preset_before {
                    Some(old) => paths::write_if_unchanged(&preset_path, Some(preset_text), old),
                    None => fs::remove_file(&preset_path).map_err(|e| e.to_string()),
                },
                _ => Err("prereglages.txt a changé entre-temps".into()),
            };
            if let Err(restore_error) = restored {
                return Err(CaptureError::File(format!(
                    "{error} ; préréglage non restauré : {restore_error}"
                )));
            }
        }
        return Err(CaptureError::File(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{AudioMode, NoiseModeRequest, NoiseState};
    use crate::presets;
    use crate::scenes::SceneSound;

    fn state() -> HeadsetState {
        HeadsetState {
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 35,
            },
            audio_mode: Some(AudioMode::Equalizer),
            spatial_audio: Some(true),
            touch_controls: Some(false),
            ..HeadsetState::default()
        }
    }

    fn eq() -> EqState {
        EqState {
            min_gain: -60,
            max_gain: 60,
            gains: vec![10, -20, 0, 5, 0, 15, -5, 0],
            bass_boost: true,
        }
    }

    #[test]
    fn state_scene_text_and_preset_round_trip() {
        let captured = scenes::from_state("Transport démo", &state(), Some(&eq())).unwrap();
        let (again, errors) = scenes::parse_scenes(&scenes::format_scenes(std::slice::from_ref(
            &captured.scene,
        )));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(again.as_slice(), std::slice::from_ref(&captured.scene));
        let scene = &again[0];
        assert_eq!(scene.sound, Some(SceneSound::Equalizer));
        assert_eq!(scene.noise, Some(NoiseModeRequest::Custom));
        assert_eq!(scene.level, Some(35));
        assert_eq!(scene.anti_wind, Some(true));
        assert_eq!(scene.touch, Some(false));
        assert_eq!(scene.spatial, Some(true));
        assert_eq!(scene.bass_boost, Some(true));
        assert_eq!(scene.equalizer.as_deref(), Some("Transport démo"));
        let preset = captured.preset.unwrap();
        let text = presets::serialize_user_presets(std::slice::from_ref(&preset));
        assert_eq!(presets::parse_user_presets(&text), [preset]);
    }

    #[test]
    fn unknown_and_inactive_settings_are_not_invented() {
        let mut s = HeadsetState::default();
        s.noise.adaptive = true;
        s.noise.anc_enabled = true;
        let captured = scenes::from_state("Démo", &s, Some(&eq())).unwrap();
        assert_eq!(captured.scene.sound, None);
        assert_eq!(captured.scene.spatial, None);
        assert_eq!(captured.scene.touch, None);
        assert_eq!(captured.scene.equalizer, None);
        assert_eq!(captured.scene.bass_boost, None);
        assert_eq!(captured.scene.level, None);
        assert_eq!(captured.preset, None);
        s.audio_mode = Some(AudioMode::Equalizer);
        let captured = scenes::from_state("Démo", &s, None).unwrap();
        assert_eq!(captured.scene.equalizer, None);
        assert_eq!(captured.scene.bass_boost, None);
        s.noise.anc_enabled = false;
        assert_eq!(
            scenes::from_state("Démo", &s, None).unwrap().scene.level,
            None
        );
        for mode in [
            AudioMode::SpeechClarity,
            AudioMode::Personalization,
            AudioMode::Unknown(99),
        ] {
            s.audio_mode = Some(mode);
            assert!(
                scenes::from_state("Démo", &s, Some(&eq()))
                    .unwrap()
                    .preset
                    .is_none()
            );
        }
    }

    #[test]
    fn inconsistent_state_and_unusable_curve_are_refused() {
        let mut s = state();
        s.audio_mode = Some(AudioMode::Personalization);
        assert!(scenes::from_state("Démo", &s, None).is_err());
        s = state();
        s.noise.level = 101;
        assert!(scenes::from_state("Démo", &s, None).is_err());
        let mut curve = eq();
        curve.gains[0] = 61;
        assert!(scenes::from_state("Démo", &state(), Some(&curve)).is_err());
        curve.gains.clear();
        assert!(scenes::from_state("Démo", &state(), Some(&curve)).is_err());
        assert!(scenes::from_state("Neutre", &state(), Some(&eq())).is_err());
        assert!(scenes::from_state("[Démo]", &state(), None).is_err());
    }

    #[test]
    fn replacement_keeps_comments_and_other_scenes_byte_for_byte() {
        let prefix = "# Début\r\n[Autre]\r\n# intact\r\nbruit=arret\r\n\r\n";
        let suffix = "[Suite]\r\ntactile=on\r\n# Fin sans saut";
        let text = format!(
            "{prefix}[Démo]\r\n# titre\r\n# Avant réglage\r\nson=voix\r\n# explication\r\nspatial=off\r\n\r\n{suffix}"
        );
        let scene = scenes::from_state("Démo", &state(), Some(&eq()))
            .unwrap()
            .scene;
        assert!(matches!(
            update_scene_text(&text, &scene, false),
            Err(CaptureError::Exists(_))
        ));
        let updated = update_scene_text(&text, &scene, true).unwrap();
        assert!(updated.starts_with(prefix) && updated.ends_with(suffix));
        for comment in ["# titre", "# Avant réglage", "# explication"] {
            assert!(updated.contains(comment));
        }
        let (list, errors) = scenes::parse_scenes(&updated);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(list[1], scene);
        let mut partial = scenes::from_state("Démo", &HeadsetState::default(), None)
            .unwrap()
            .scene;
        partial.name = "démo".into();
        assert!(update_scene_text(&text, &partial, false).is_err());
        let updated = update_scene_text(&text, &partial, true).unwrap();
        assert!(updated.contains("# explication"));
        assert!(!updated.contains("son="));
    }

    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "scene-capture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn files_keep_unrelated_content_and_duplicate_does_not_write() {
        let dir = Temp::new();
        let original = "# Scènes préservées\n[Autre]\ntactile=off\n# fin";
        let presets = "# Courbes\nAutre courbe = 0;0;0;0;0;0;0;0\n# inchangé\n";
        fs::write(dir.0.join("scenes.txt"), original).unwrap();
        fs::write(dir.0.join("prereglages.txt"), presets).unwrap();
        let captured = scenes::from_state("Démo", &state(), Some(&eq())).unwrap();
        save_in(&dir.0, &captured, false).unwrap();
        let scenes_text = fs::read_to_string(dir.0.join("scenes.txt")).unwrap();
        let preset_text = fs::read_to_string(dir.0.join("prereglages.txt")).unwrap();
        assert!(scenes_text.starts_with(original));
        assert!(preset_text.starts_with(presets));
        assert_eq!(
            presets::parse_user_presets(&preset_text)[1],
            captured.preset.clone().unwrap()
        );
        assert!(save_in(&dir.0, &captured, false).is_err());
        assert_eq!(
            fs::read_to_string(dir.0.join("scenes.txt")).unwrap(),
            scenes_text
        );
        assert_eq!(
            fs::read_to_string(dir.0.join("prereglages.txt")).unwrap(),
            preset_text
        );
        save_in(&dir.0, &captured, true).unwrap();
        assert_eq!(
            presets::parse_user_presets(
                &fs::read_to_string(dir.0.join("prereglages.txt")).unwrap()
            )
            .len(),
            2
        );
    }

    #[test]
    fn preset_collision_invalid_file_and_second_write_failure_preserve_data() {
        let dir = Temp::new();
        let captured = scenes::from_state("Démo", &state(), Some(&eq())).unwrap();
        let old_presets = "# Courbe originale\nDémo=0;0;0;0;0;0;0;0\n# conserver\n";
        fs::write(dir.0.join("prereglages.txt"), old_presets).unwrap();
        assert!(save_in(&dir.0, &captured, false).is_err());
        assert!(!dir.0.join("scenes.txt").exists());
        fs::write(dir.0.join("scenes.txt"), "[Invalide]\ninconnu=on\n").unwrap();
        assert!(save_in(&dir.0, &captured, true).is_err());
        assert_eq!(
            fs::read_to_string(dir.0.join("prereglages.txt")).unwrap(),
            old_presets
        );
        let old_scenes = "# Commentaire\n[Autre]\ntactile=off\n";
        fs::write(dir.0.join("scenes.txt"), old_scenes).unwrap();
        // Le fichier de sauvegarde est volontairement un dossier : la deuxième
        // écriture échoue sans dépendre des permissions de l'utilisateur du test.
        fs::create_dir(dir.0.join("scenes.txt.bak")).unwrap();
        assert!(save_in(&dir.0, &captured, true).is_err());
        assert_eq!(
            fs::read_to_string(dir.0.join("scenes.txt")).unwrap(),
            old_scenes
        );
        assert_eq!(
            fs::read_to_string(dir.0.join("prereglages.txt")).unwrap(),
            old_presets
        );
    }
}
