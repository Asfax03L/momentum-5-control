//! Langue des interfaces : français (langue de référence, celle du code) ou
//! anglais. Chaque texte affiché est écrit en français dans le code et
//! passe par [`tr`] (texte fixe) ou [`trf`] (texte avec valeurs, `{}`) ; la
//! traduction anglaise vient du catalogue `i18n/en.txt`, où chaque entrée
//! est un paragraphe de deux lignes : le texte français, puis l'anglais.
//! Un texte absent du catalogue reste en français (jamais vide) ; un essai
//! vérifie que tous les textes du code y figurent.
//!
//! Langue choisie : variable `OPENHEADSET_LANG` (essais, scripts), sinon
//! réglage de l'utilisateur (`langue.txt` : `langue = auto`, `fr` ou `en`),
//! sinon langue du système (`LC_ALL`, `LC_MESSAGES`, `LANG` ; langue
//! d'affichage sous Windows) : français si elle commence par « fr »,
//! anglais sinon. Le choix vaut pour toute la durée du programme.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Langue d'affichage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// Code court (`fr`, `en`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// `fr…` : français ; toute autre langue connue : anglais.
    /// `fr`, `fr_FR.UTF-8`… : français ; `en`, `de_DE`… : anglais ;
    /// `auto`, `C`, `POSIX`, vide : pas de choix (`None`).
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let tag = tag.trim().to_ascii_lowercase();
        let base = tag.split(['.', '@']).next().unwrap_or("");
        if base.is_empty() || base == "c" || base == "posix" || base == "auto" {
            return None;
        }
        Some(if base.starts_with("fr") {
            Lang::Fr
        } else {
            Lang::En
        })
    }
}

static LANG: OnceLock<Lang> = OnceLock::new();
static CATALOGUE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

const EN: &str = include_str!("i18n/en.txt");

/// Entrées du catalogue anglais : (français, anglais).
pub fn catalogue_entries() -> Vec<(&'static str, &'static str)> {
    EN.split("\n\n")
        .filter_map(|block| {
            let block = block.trim_matches('\n');
            if block.is_empty() || block.starts_with('#') {
                return None;
            }
            let (fr, en) = block.split_once('\n')?;
            Some((fr, en))
        })
        .collect()
}

fn catalogue() -> &'static HashMap<&'static str, &'static str> {
    CATALOGUE.get_or_init(|| catalogue_entries().into_iter().collect())
}

/// Fixe la langue (une seule fois, au démarrage) ; sans appel, [`language`]
/// la détecte.
pub fn set_language(lang: Lang) {
    let _ = LANG.set(lang);
}

/// Langue en vigueur.
pub fn language() -> Lang {
    *LANG.get_or_init(detect)
}

fn detect() -> Lang {
    // Choix explicite d'abord (essais, scripts), puis réglage de
    // l'utilisateur, puis langue du système.
    let env = |var| std::env::var(var).ok().as_deref().and_then(Lang::from_tag);
    env("OPENHEADSET_LANG")
        .or_else(load_pref)
        .or_else(|| ["LC_ALL", "LC_MESSAGES", "LANG"].into_iter().find_map(env))
        .or_else(system_language)
        .unwrap_or(Lang::Fr)
}

/// `langue.txt` : choix de l'utilisateur.
pub fn pref_file() -> Option<std::path::PathBuf> {
    Some(crate::paths::data_dir()?.join("langue.txt"))
}

/// Choix lu dans un texte de `langue.txt` (`None` : `auto` ou absent).
pub fn parse_pref(text: &str) -> Option<Lang> {
    text.lines().find_map(|line| {
        let line = line.split('#').next().unwrap_or("");
        let (key, value) = line.split_once('=')?;
        (key.trim() == "langue").then(|| Lang::from_tag(value))?
    })
}

/// Choix enregistré, s'il y en a un.
pub fn load_pref() -> Option<Lang> {
    pref_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| parse_pref(&text))
}

/// Enregistre le choix (`None` : langue du système) ; pris en compte au
/// prochain lancement.
pub fn store_pref(lang: Option<Lang>) -> std::io::Result<()> {
    let path =
        pref_file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    let value = lang.map_or("auto", Lang::code);
    crate::paths::write_user_file(
        &path,
        &format!("# Langue d'OpenHeadset Control : langue = auto, fr ou en\nlangue = {value}\n"),
    )
}

#[cfg(windows)]
fn system_language() -> Option<Lang> {
    // Langue d'affichage de l'utilisateur ; identifiant principal 0x0C :
    // français (toutes variantes).
    // SAFETY: simple lecture d'un réglage du système.
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    Some(if id & 0x3FF == 0x0C {
        Lang::Fr
    } else {
        Lang::En
    })
}

#[cfg(not(windows))]
fn system_language() -> Option<Lang> {
    None
}

/// Texte fixe dans la langue en vigueur.
pub fn tr(fr: &'static str) -> &'static str {
    match language() {
        Lang::Fr => fr,
        Lang::En => catalogue().get(fr).copied().unwrap_or(fr),
    }
}

/// Texte quelconque (venu du QML, par exemple) dans la langue en vigueur :
/// même catalogue que [`tr`], le texte lui-même s'il n'y figure pas.
pub fn translate(fr: &str) -> String {
    match language() {
        Lang::Fr => fr.to_string(),
        Lang::En => catalogue().get(fr).copied().unwrap_or(fr).to_string(),
    }
}

/// Texte avec valeurs : chaque `{}` reçoit la valeur suivante, dans
/// l'ordre ; le texte anglais garde le même nombre de `{}`.
pub fn trf(fr: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    fill(tr(fr), args)
}

fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut values = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(v) = values.next() {
            out.push_str(&v.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_preferences() {
        assert_eq!(Lang::from_tag("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("fr-CA"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_tag("de_DE@euro"), Some(Lang::En));
        for none in ["", "C", "C.UTF-8", "POSIX", "auto"] {
            assert_eq!(Lang::from_tag(none), None, "{none}");
        }
        assert_eq!(parse_pref("langue = en\n"), Some(Lang::En));
        assert_eq!(parse_pref("# langue = en\nlangue = auto\n"), None);
        assert_eq!(parse_pref(""), None);
    }

    #[test]
    fn values_fill_the_placeholders_in_order() {
        assert_eq!(fill("Batterie : {} %", &[&80]), "Batterie : 80 %");
        assert_eq!(fill("{} sur {}", &[&1, &"deux"]), "1 sur deux");
        assert_eq!(fill("sans valeur", &[&1]), "sans valeur");
        assert_eq!(fill("{} et {}", &[&1]), "1 et ");
    }

    #[test]
    fn catalogue_is_well_formed() {
        let entries = catalogue_entries();
        let mut seen = std::collections::HashSet::new();
        for (fr, en) in &entries {
            assert!(seen.insert(*fr), "entrée en double : {fr}");
            assert!(!en.trim().is_empty(), "traduction vide : {fr}");
            assert!(!en.contains('\n'), "plus de deux lignes : {fr}");
            assert_eq!(
                fr.matches("{}").count(),
                en.matches("{}").count(),
                "valeurs différentes : {fr} / {en}"
            );
        }
    }

    /// Textes passés à `tr("…")` ou `trf("…"` dans les fichiers donnés.
    pub(crate) fn texts_in(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        for marker in ["tr(\"", "trf(\""] {
            let mut rest = source;
            while let Some(i) = rest.find(marker) {
                // « str(" » ou « attr(" » ne sont pas des appels de traduction.
                let before = rest[..i].chars().next_back();
                rest = &rest[i + marker.len()..];
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let mut text = String::new();
                let mut chars = rest.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => text.push('\n'),
                            Some(other) => text.push(other),
                            None => break,
                        },
                        c => text.push(c),
                    }
                }
                found.push(text);
            }
        }
        found
    }

    #[test]
    fn every_choice_list_is_translated() {
        use crate::viewmodel as v;
        let known: std::collections::HashSet<&str> =
            catalogue_entries().into_iter().map(|(fr, _)| fr).collect();
        let lists: [&[&str]; 7] = [
            &v::NOISE_MODES,
            &v::AUTO_OFF_LABELS,
            &v::PROMPT_MODES,
            &v::AUDIO_MODES,
            &v::SCENE_SWITCH_CHOICES,
            &v::SCENE_SOUND_CHOICES,
            &v::SCENE_NOISE_CHOICES,
        ];
        let mut missing: Vec<&str> = lists.iter().flat_map(|l| l.iter().copied()).collect();
        missing.extend(v::BATTERY_SPANS.iter().map(|(label, _)| *label));
        missing.retain(|t| !known.contains(t));
        assert!(missing.is_empty(), "choix sans traduction : {missing:?}");
    }

    #[test]
    fn every_text_of_the_interfaces_is_translated() {
        // Tous les fichiers Rust et QML du dépôt (hors target) : chaque texte
        // passé à tr/trf a sa traduction anglaise.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let known: std::collections::HashSet<&str> =
            catalogue_entries().into_iter().map(|(fr, _)| fr).collect();
        let mut missing = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if path.is_dir() {
                    if !matches!(name.as_ref(), "target" | ".git" | "node_modules" | "fuzz") {
                        stack.push(path);
                    }
                } else if name.ends_with(".rs") || name.ends_with(".qml") {
                    if path.ends_with("i18n.rs") {
                        continue;
                    }
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    for t in texts_in(&text) {
                        if !known.contains(t.as_str()) {
                            missing.push(format!("{} : {t}", path.display()));
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "textes sans traduction :\n{}",
            missing.join("\n")
        );
    }
}
