//! Thème choisi par l'utilisateur : clair ou sombre, dans `apparence.txt`
//! (même fichier que l'ancienne icône Slint). Sans choix enregistré, les
//! interfaces suivent le thème du système.

use crate::paths;

/// Fichier du thème choisi.
pub fn file() -> Option<std::path::PathBuf> {
    Some(paths::data_dir()?.join("apparence.txt"))
}

/// Thème lu : `Some(true)` sombre, `Some(false)` clair, `None` : aucun
/// choix (le système décide).
pub fn parse(text: &str) -> Option<bool> {
    text.lines().find_map(|line| {
        let line = line.split('#').next().unwrap_or("");
        let (key, value) = line.split_once('=')?;
        if key.trim() != "theme" {
            return None;
        }
        match value.trim() {
            "sombre" => Some(true),
            "clair" => Some(false),
            _ => None,
        }
    })
}

/// Thème enregistré, s'il y en a un.
pub fn load() -> Option<bool> {
    file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| parse(&text))
}

/// Texte de `apparence.txt` pour ce choix.
pub fn to_text(dark: bool) -> String {
    format!(
        "# Apparence d'OpenHeadset Control : theme = sombre ou clair\ntheme = {}\n",
        if dark { "sombre" } else { "clair" }
    )
}

/// Enregistre le choix (avec sauvegarde de l'ancien fichier).
pub fn store(dark: bool) -> std::io::Result<()> {
    let path = file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    paths::write_user_file(&path, &to_text(dark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_round_trip_and_absence() {
        assert_eq!(parse(&to_text(true)), Some(true));
        assert_eq!(parse(&to_text(false)), Some(false));
        assert_eq!(parse(""), None);
        assert_eq!(parse("theme = violet\n"), None);
        assert_eq!(parse("# theme = clair\n"), None);
        assert_eq!(parse("theme=clair # choisi le 21/10\n"), Some(false));
    }
}
