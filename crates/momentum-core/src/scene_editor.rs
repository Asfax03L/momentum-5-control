//! Éditeur de scènes dans la fenêtre, en Rust pur (même comportement dans
//! WinUI et le client KDE ; repris de l'ancienne icône Slint).
//!
//! L'éditeur travaille sur un brouillon de `scenes.txt` : rien n'est écrit
//! avant « Enregistrer », et rien n'est jamais envoyé au casque. Il refuse
//! d'enregistrer si le fichier contient des erreurs (rien à perdre) ou s'il
//! a été modifié ailleurs depuis son ouverture. Les renommages et
//! suppressions sont rendus à l'interface, qui fait suivre les raccourcis et
//! le panneau rapide.

use crate::presets::Preset;
use crate::scenes::{self, Scene};
use crate::viewmodel::{self, SceneForm};
use crate::{hotkeys, paths};

/// Changement d'une scène enregistrée : ancien nom, nouveau nom (`None` :
/// supprimée).
pub type SceneChange = (String, Option<String>);

#[derive(Clone, Debug, Default)]
pub struct SceneEditor {
    /// Brouillon : nom dans le fichier (`None` : nouvelle), scène.
    draft: Vec<(Option<String>, Scene)>,
    selected: usize,
    dirty: bool,
    /// Texte du fichier à l'ouverture : une modification ailleurs bloque
    /// l'enregistrement.
    loaded_text: Option<String>,
    file_errors: Vec<String>,
    form_error: Option<String>,
    /// Préréglages proposés à la scène choisie.
    eq_choices: Vec<String>,
    presets: Vec<Preset>,
}

impl SceneEditor {
    /// Ouvre l'éditeur sur `text` (contenu de `scenes.txt`, `None` s'il
    /// manque) ; `presets` : préréglages que les scènes peuvent nommer.
    pub fn load(text: Option<String>, presets: Vec<Preset>) -> Self {
        let (list, errors) = scenes::parse_scenes(text.as_deref().unwrap_or(""));
        let mut editor = Self {
            draft: list
                .into_iter()
                .map(|s| (Some(s.name.clone()), s))
                .collect(),
            loaded_text: text,
            file_errors: errors,
            presets,
            ..Self::default()
        };
        editor.refresh_choices();
        editor
    }

    fn refresh_choices(&mut self) {
        let scene = self.draft.get(self.selected).map(|(_, s)| s);
        self.eq_choices = viewmodel::equalizer_choices(scene, &self.presets);
    }

    pub fn names(&self) -> Vec<String> {
        self.draft.iter().map(|(_, s)| s.name.clone()).collect()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn is_empty(&self) -> bool {
        self.draft.is_empty()
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Préréglages proposés (« inchangé » en premier).
    pub fn eq_choices(&self) -> &[String] {
        &self.eq_choices
    }

    /// Formulaire de la scène choisie.
    pub fn form(&self) -> SceneForm {
        self.draft
            .get(self.selected)
            .map(|(_, s)| viewmodel::scene_form(s, &self.eq_choices))
            .unwrap_or_default()
    }

    /// Étapes prévues de la scène choisie.
    pub fn preview(&self) -> String {
        self.draft
            .get(self.selected)
            .map_or(String::new(), |(_, s)| viewmodel::scene_preview(s))
    }

    /// Ce qui empêche d'enregistrer, s'il y a lieu.
    pub fn problem(&self) -> Option<String> {
        if !self.file_errors.is_empty() {
            return Some(format!(
                "scenes.txt contient des erreurs : l'enregistrement est désactivé pour ne \
                 rien perdre. Corrigez le fichier, puis rechargez. {}",
                self.file_errors.join(" ; ")
            ));
        }
        self.form_error
            .clone()
            .or_else(|| scenes::check_scenes(&self.scenes()).err())
    }

    pub fn can_save(&self) -> bool {
        self.dirty && self.problem().is_none()
    }

    /// Nom de la scène choisie telle qu'enregistrée (pour l'appliquer),
    /// seulement sans modification en cours.
    pub fn saved_selection(&self) -> Option<String> {
        if self.dirty {
            return None;
        }
        self.draft.get(self.selected).and_then(|(o, _)| o.clone())
    }

    fn scenes(&self) -> Vec<Scene> {
        self.draft.iter().map(|(_, s)| s.clone()).collect()
    }

    pub fn select(&mut self, index: usize) {
        if index < self.draft.len() {
            self.form_error = None;
            self.selected = index;
            self.refresh_choices();
        }
    }

    /// Le formulaire a changé : la scène suit s'il est valide.
    pub fn set_form(&mut self, form: &SceneForm) {
        self.dirty = true;
        match viewmodel::scene_from_form(form, &self.eq_choices) {
            Ok(scene) => {
                if let Some((_, slot)) = self.draft.get_mut(self.selected) {
                    *slot = scene;
                }
                self.form_error = None;
            }
            Err(e) => self.form_error = Some(e),
        }
    }

    /// Nom libre : `base`, puis `base 2`, `base 3`…
    fn free_name(&self, base: &str) -> String {
        let taken = |n: &str| {
            self.draft
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

    fn after_list_change(&mut self, select: usize) {
        self.dirty = true;
        self.form_error = None;
        self.selected = select;
        self.refresh_choices();
    }

    pub fn new_scene(&mut self) {
        let scene = Scene {
            name: self.free_name("Nouvelle scène"),
            noise: Some(crate::device::NoiseModeRequest::Adaptive),
            ..Scene::default()
        };
        self.draft.push((None, scene));
        self.after_list_change(self.draft.len() - 1);
    }

    pub fn duplicate(&mut self) {
        let Some((_, scene)) = self.draft.get(self.selected) else {
            return;
        };
        let mut copy = scene.clone();
        copy.name = self.free_name(&format!("{} (copie)", scene.name));
        self.draft.insert(self.selected + 1, (None, copy));
        self.after_list_change(self.selected + 1);
    }

    pub fn delete(&mut self) {
        if self.selected < self.draft.len() {
            self.draft.remove(self.selected);
            let next = self.selected.min(self.draft.len().saturating_sub(1));
            self.after_list_change(next);
        }
    }

    /// Monte (`-1`) ou descend (`1`) la scène choisie.
    pub fn move_by(&mut self, delta: isize) {
        let from = self.selected;
        if let Some(to) = from.checked_add_signed(delta)
            && to < self.draft.len()
            && from < self.draft.len()
        {
            self.draft.swap(from, to);
            self.after_list_change(to);
        }
    }

    /// Prépare l'enregistrement : texte à écrire dans `scenes.txt` et
    /// renommages ou suppressions à faire suivre. `current_text` : contenu
    /// actuel du fichier (refus s'il a changé depuis l'ouverture).
    pub fn save_plan(
        &self,
        current_text: Option<&str>,
    ) -> Result<(String, Vec<SceneChange>), String> {
        if let Some(problem) = self.problem() {
            return Err(problem);
        }
        if current_text != self.loaded_text.as_deref() {
            return Err(
                "scenes.txt a été modifié ailleurs depuis l'ouverture de l'éditeur ; \
                 « Recharger » le relit."
                    .to_string(),
            );
        }
        let mut changes: Vec<SceneChange> = self
            .draft
            .iter()
            .filter_map(|(origin, s)| {
                origin
                    .as_ref()
                    .filter(|o| **o != s.name)
                    .map(|o| (o.clone(), Some(s.name.clone())))
            })
            .collect();
        let (saved, _) = scenes::parse_scenes(self.loaded_text.as_deref().unwrap_or(""));
        for scene in saved {
            let kept = self
                .draft
                .iter()
                .any(|(origin, _)| origin.as_deref() == Some(scene.name.as_str()));
            if !kept {
                changes.push((scene.name, None));
            }
        }
        Ok((scenes::format_scenes(&self.scenes()), changes))
    }
}

/// Enregistre le brouillon dans `scenes.txt` (avec sauvegarde), puis fait
/// suivre renommages et suppressions dans `raccourcis.txt` et
/// `panneau.txt`. Renvoie le message à afficher et si les raccourcis ont
/// changé (à réenregistrer). Rien n'est envoyé au casque.
pub fn save(editor: &SceneEditor) -> Result<(String, bool), String> {
    let path = paths::scenes_file().ok_or("dossier des données introuvable")?;
    let mut changes = Vec::new();
    // Lecture du fichier actuel et remplacement d'un bloc (A03).
    paths::update_user_file(&path, |current| {
        let (text, found) = editor.save_plan(current)?;
        changes = found;
        Ok(Some(text))
    })?;
    let mut message = "Scènes enregistrées (rien n'a été envoyé au casque).".to_string();
    let mut hotkeys_changed = false;
    if !changes.is_empty() {
        if let Some(p) = paths::panel_file() {
            let _ = paths::update_user_file(&p, |current| {
                let Some(text) = current else { return Ok(None) };
                let mut panel = crate::quick_panel::PanelPrefs::parse(text);
                for (old, new) in &changes {
                    panel.rename_scene(old, new.as_deref());
                }
                Ok(Some(panel.to_text()))
            });
        }
        match paths::hotkey_config() {
            // Illisible (autre encodage) : gardé tel quel, sans réécriture (A05).
            Err(e) => message.push_str(&format!(
                " Raccourcis non mis à jour (fichier illisible : {e})."
            )),
            Ok(_) => {
                let updated = paths::hotkeys_file()
                    .ok_or_else(|| "dossier des données introuvable".to_string())
                    .and_then(|p| {
                        paths::update_user_file(&p, |current| {
                            let Some(before) = current else {
                                return Ok(None);
                            };
                            let mut keys = before.to_string();
                            for (old, new) in &changes {
                                keys = hotkeys::rename_scene(&keys, old, new.as_deref());
                            }
                            Ok((keys != before).then_some(keys))
                        })
                    });
                match updated {
                    Ok(true) => {
                        hotkeys_changed = true;
                        message.push_str(" Raccourcis mis à jour.");
                    }
                    Ok(false) => {}
                    Err(e) => message.push_str(&format!(" Raccourcis non mis à jour : {e}.")),
                }
            }
        }
    }
    Ok((message, hotkeys_changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str =
        "[Travail]\nbruit = adaptatif\n\n[Transport]\nbruit = personnalise\nniveau = 0\n";

    fn editor() -> SceneEditor {
        SceneEditor::load(Some(TEXT.to_string()), crate::presets::builtin_presets(8))
    }

    #[test]
    fn loads_scenes_without_changes() {
        let e = editor();
        assert_eq!(e.names(), ["Travail", "Transport"]);
        assert!(!e.dirty() && !e.can_save());
        assert_eq!(e.saved_selection().as_deref(), Some("Travail"));
        assert!(e.preview().contains("adaptatif"));
    }

    #[test]
    fn rename_delete_and_new_are_reported_on_save() {
        let mut e = editor();
        let mut form = e.form();
        form.name = "Bureau".to_string();
        e.set_form(&form);
        e.select(1);
        e.delete();
        e.new_scene();
        assert_eq!(e.names(), ["Bureau", "Nouvelle scène"]);
        assert!(e.can_save());
        assert_eq!(e.saved_selection(), None);
        let (text, changes) = e.save_plan(Some(TEXT)).unwrap();
        assert!(text.contains("[Bureau]") && text.contains("[Nouvelle scène]"));
        assert!(!text.contains("[Transport]"));
        assert_eq!(
            changes,
            [
                ("Travail".to_string(), Some("Bureau".to_string())),
                ("Transport".to_string(), None),
            ]
        );
    }

    #[test]
    fn invalid_form_and_outside_change_block_saving() {
        let mut e = editor();
        let mut form = e.form();
        form.level = "150".to_string();
        e.set_form(&form);
        assert!(!e.can_save());
        assert!(e.problem().unwrap().contains("niveau"));
        // Un niveau exige le mode personnalisé (vérification des scènes).
        form.level = "40".to_string();
        form.noise = 2;
        e.set_form(&form);
        assert!(e.can_save(), "{:?}", e.problem());
        assert!(e.save_plan(Some("[Autre]\nbruit = arret\n")).is_err());
    }

    #[test]
    fn duplicate_names_get_a_number_and_order_can_change() {
        let mut e = editor();
        e.duplicate();
        assert_eq!(e.names(), ["Travail", "Travail (copie)", "Transport"]);
        // La copie est choisie : on revient sur l'original pour le recopier.
        e.select(0);
        e.duplicate();
        assert_eq!(e.names()[1], "Travail (copie) 2");
        e.move_by(1);
        assert_eq!(e.names()[2], "Travail (copie) 2");
        assert_eq!(e.selected(), 2);
    }

    #[test]
    fn file_errors_disable_saving() {
        let mut e = SceneEditor::load(Some("[Bad]\ninconnu = 1\n".to_string()), Vec::new());
        e.new_scene();
        assert!(!e.can_save());
        assert!(e.problem().unwrap().contains("erreurs"));
    }
}
