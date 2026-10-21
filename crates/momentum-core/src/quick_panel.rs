//! Contenu du panneau rapide (clic gauche sur l'icône), choisi par
//! l'utilisateur. Fichier `panneau.txt` : lignes `réglage = valeur`.
//!
//! ```text
//! bruit = oui           # boutons du contrôle du bruit
//! niveau = oui          # curseur ANC ↔ transparence
//! profil = oui          # interrupteur du profil personnalisé
//! scenes = Travail, Transport
//! ```
//!
//! Sans ligne `scenes`, le panneau montre les premières scènes du fichier
//! (comportement d'origine). Ces préférences ne touchent jamais le casque.

/// Nombre de scènes que le panneau peut montrer (largeur du panneau).
pub const MAX_PANEL_SCENES: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelPrefs {
    pub noise: bool,
    pub level: bool,
    pub profile: bool,
    /// Scènes choisies, dans l'ordre ; `None` : les premières du fichier.
    pub scenes: Option<Vec<String>>,
}

impl Default for PanelPrefs {
    fn default() -> Self {
        Self {
            noise: true,
            level: true,
            profile: true,
            scenes: None,
        }
    }
}

fn yes_no(value: &str) -> Option<bool> {
    match value.trim() {
        "oui" => Some(true),
        "non" => Some(false),
        _ => None,
    }
}

impl PanelPrefs {
    /// Lit `panneau.txt` ; une ligne illisible garde la valeur par défaut.
    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "bruit" => prefs.noise = yes_no(value).unwrap_or(prefs.noise),
                "niveau" => prefs.level = yes_no(value).unwrap_or(prefs.level),
                "profil" => prefs.profile = yes_no(value).unwrap_or(prefs.profile),
                "scenes" => {
                    prefs.scenes = Some(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|n| !n.is_empty())
                            .map(str::to_string)
                            .take(MAX_PANEL_SCENES)
                            .collect(),
                    );
                }
                _ => {}
            }
        }
        prefs
    }

    /// Texte de `panneau.txt`.
    pub fn to_text(&self) -> String {
        let yn = |b: bool| if b { "oui" } else { "non" };
        let mut out = format!(
            "# Panneau rapide de momentum-tray (fenêtre, page Scènes).\n\
             bruit = {}\nniveau = {}\nprofil = {}\n",
            yn(self.noise),
            yn(self.level),
            yn(self.profile)
        );
        if let Some(scenes) = &self.scenes {
            out.push_str(&format!("scenes = {}\n", scenes.join(", ")));
        }
        out
    }

    /// Scènes à montrer, parmi celles du fichier (`all`, dans leur ordre) :
    /// les favorites qui existent encore, ou les premières.
    pub fn shown_scenes(&self, all: &[String]) -> Vec<String> {
        match &self.scenes {
            None => all.iter().take(MAX_PANEL_SCENES).cloned().collect(),
            Some(chosen) => all
                .iter()
                .filter(|n| chosen.iter().any(|c| same(c, n)))
                .take(MAX_PANEL_SCENES)
                .cloned()
                .collect(),
        }
    }

    /// Montre ou retire une scène du panneau. Refusé au-delà de
    /// [`MAX_PANEL_SCENES`] ; `all` sert à partir des scènes montrées
    /// jusqu'ici quand aucun choix n'a encore été fait.
    pub fn set_scene(&mut self, all: &[String], name: &str, shown: bool) -> Result<(), String> {
        let mut chosen = self.shown_scenes(all);
        chosen.retain(|n| !same(n, name));
        if shown {
            if chosen.len() >= MAX_PANEL_SCENES {
                return Err(format!(
                    "le panneau montre au plus {MAX_PANEL_SCENES} scènes : retirez-en une d'abord"
                ));
            }
            chosen.push(name.to_string());
        }
        self.scenes = Some(chosen);
        Ok(())
    }

    /// Suit un renommage (`Some`) ou une suppression (`None`) de scène.
    pub fn rename_scene(&mut self, old: &str, new: Option<&str>) {
        if let Some(scenes) = &mut self.scenes {
            match new {
                Some(new) => {
                    for n in scenes.iter_mut().filter(|n| same(n, old)) {
                        *n = new.to_string();
                    }
                }
                None => scenes.retain(|n| !same(n, old)),
            }
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_keep_the_original_panel() {
        let p = PanelPrefs::parse("");
        assert_eq!(p, PanelPrefs::default());
        let all = names(&["A", "B", "C", "D", "E"]);
        assert_eq!(p.shown_scenes(&all), names(&["A", "B", "C", "D"]));
    }

    #[test]
    fn prefs_round_trip_and_follow_the_file_order() {
        let mut p = PanelPrefs {
            noise: true,
            level: false,
            profile: true,
            scenes: Some(names(&["Transport", "Travail"])),
        };
        let again = PanelPrefs::parse(&p.to_text());
        assert_eq!(again, p);
        let all = names(&["Travail", "Écoute", "Transport"]);
        // Ordre du fichier des scènes ; une scène disparue est ignorée.
        assert_eq!(p.shown_scenes(&all), names(&["Travail", "Transport"]));
        p.rename_scene("travail", Some("Bureau"));
        p.rename_scene("Transport", None);
        assert_eq!(p.scenes, Some(names(&["Bureau"])));
    }

    #[test]
    fn at_most_four_scenes_are_shown() {
        let all = names(&["A", "B", "C", "D", "E"]);
        let mut p = PanelPrefs::default();
        assert!(
            p.set_scene(&all, "E", true)
                .unwrap_err()
                .contains("au plus 4")
        );
        p.set_scene(&all, "B", false).unwrap();
        p.set_scene(&all, "E", true).unwrap();
        assert_eq!(p.shown_scenes(&all), names(&["A", "C", "D", "E"]));
        // Valeurs illisibles : défaut conservé.
        let p = PanelPrefs::parse("bruit = peut-être\nniveau = non\n");
        assert!(p.noise && !p.level);
    }
}
