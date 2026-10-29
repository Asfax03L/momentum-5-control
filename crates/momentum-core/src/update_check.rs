//! Avis de nouvelle version (Rust pur) : **désactivé par défaut**. Activé
//! dans les Paramètres, l'interface lit au lancement la dernière
//! publication du dépôt sur l'API GitHub (une requête GET, sans autre
//! donnée que l'adresse IP et l'en-tête `User-Agent`), puis la compare à sa
//! propre version. Aucune télémétrie, aucun téléchargement : l'utilisateur
//! ouvre la page de la publication s'il le souhaite.
//!
//! La requête elle-même appartient aux interfaces (WinRT sous Windows,
//! `XMLHttpRequest` de QML sous KDE). Jamais en démonstration ; jamais dans
//! le Flatpak, qui n'a pas accès au réseau et se met à jour par Flatpak.

use crate::i18n::{tr, trf};

/// Dernière publication du dépôt (API GitHub).
pub const LATEST_URL: &str =
    "https://api.github.com/repos/Asfax03L/momentum-5-control/releases/latest";
/// Seules pages ouvertes depuis l'avis.
pub const RELEASES_PREFIX: &str = "https://github.com/Asfax03L/momentum-5-control/releases/";
const LEGACY_RELEASES_PREFIX: &str = "https://github.com/Asfax03L/openheadset-control/releases/";
/// En-tête exigé par l'API GitHub ; rien d'autre n'est envoyé.
pub const USER_AGENT: &str = "OpenHeadset-Control";

/// Publication lue sur GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: (u64, u64, u64),
    pub tag: String,
    pub url: String,
}

/// `MAJEUR.MINEUR.CORRECTIF` d'une étiquette `vX.Y.Z` ou d'une version.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    let mut parts = text.split('.');
    let version = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(version)
}

/// Réponse de l'API GitHub ; `None` si elle est illisible, si c'est un
/// brouillon ou une préversion, ou si la page n'est pas une publication du
/// dépôt.
pub fn parse_latest(json: &str) -> Option<Release> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value.get("draft")?.as_bool()? || value.get("prerelease")?.as_bool()? {
        return None;
    }
    let tag = value.get("tag_name")?.as_str()?.to_string();
    let url = value.get("html_url")?.as_str()?.to_string();
    if ![RELEASES_PREFIX, LEGACY_RELEASES_PREFIX]
        .iter()
        .any(|prefix| url == format!("{prefix}tag/{tag}"))
    {
        return None;
    }
    Some(Release {
        version: parse_version(&tag)?,
        tag,
        url,
    })
}

/// Résultat d'une vérification, prêt à afficher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    Available(Release),
    Failed(String),
}

impl Outcome {
    /// Compare la publication lue à la version en cours (`CARGO_PKG_VERSION`).
    pub fn from_response(current: &str, body: Result<&str, &str>) -> Self {
        match body {
            Err(e) => Outcome::Failed(e.to_string()),
            Ok(json) => match (parse_latest(json), parse_version(current)) {
                (Some(release), Some(current)) if release.version > current => {
                    Outcome::Available(release)
                }
                (Some(_), Some(_)) => Outcome::UpToDate,
                _ => Outcome::Failed(tr("réponse de GitHub illisible").to_string()),
            },
        }
    }

    /// Texte affiché.
    pub fn text(&self, current: &str) -> String {
        match self {
            Outcome::UpToDate => trf("À jour (version {}).", &[&current]),
            Outcome::Available(r) => trf("Nouvelle version disponible : {}.", &[&r.tag]),
            Outcome::Failed(e) => trf("Vérification impossible : {}.", &[e]),
        }
    }
}

fn pref_file() -> Option<std::path::PathBuf> {
    Some(crate::paths::data_dir()?.join("mises-a-jour.txt"))
}

/// Choix lu dans le texte de `mises-a-jour.txt` (`avis = oui|non`).
pub fn parse_pref(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.split('#').next().unwrap_or("");
        line.split_once('=')
            .is_some_and(|(k, v)| k.trim() == "avis" && v.trim() == "oui")
    })
}

/// Avis activé ? Non par défaut (fichier absent ou illisible).
pub fn load_enabled() -> bool {
    pref_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .is_some_and(|text| parse_pref(&text))
}

/// Enregistre le choix.
pub fn store_enabled(enabled: bool) -> std::io::Result<()> {
    let path =
        pref_file().ok_or_else(|| std::io::Error::other(tr("dossier des données introuvable")))?;
    crate::paths::write_user_file(
        &path,
        &format!(
            "# Avis de nouvelle version (lecture de la dernière publication sur GitHub) : oui ou non\navis = {}\n",
            if enabled { "oui" } else { "non" }
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reponse(tag: &str, url: &str, prerelease: bool) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"{url}","draft":false,"prerelease":{prerelease},"assets":[]}}"#
        )
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_version("0.16.0"), Some((0, 16, 0)));
        assert_eq!(parse_version("v1.0"), None);
        assert_eq!(parse_version("v1.0.0-rc1"), None);
        assert!(parse_version("v1.10.0") > parse_version("v1.9.3"));
    }

    #[test]
    fn newer_release_is_announced() {
        let url = format!("{RELEASES_PREFIX}tag/v1.1.0");
        let json = reponse("v1.1.0", &url, false);
        match Outcome::from_response("1.0.0", Ok(&json)) {
            Outcome::Available(r) => {
                assert_eq!(r.tag, "v1.1.0");
                assert_eq!(r.url, url);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            Outcome::from_response("1.1.0", Ok(&json)),
            Outcome::UpToDate
        );
        assert_eq!(
            Outcome::from_response("1.2.0", Ok(&json)),
            Outcome::UpToDate
        );
    }

    #[test]
    fn suspicious_answers_are_refused() {
        // Préversion, page hors du dépôt, réponse illisible.
        let pre = reponse("v2.0.0", &format!("{RELEASES_PREFIX}tag/v2.0.0"), true);
        assert!(parse_latest(&pre).is_none());
        let ailleurs = reponse("v2.0.0", "https://example.com/releases/tag/v2.0.0", false);
        assert!(parse_latest(&ailleurs).is_none());
        assert!(matches!(
            Outcome::from_response("1.0.0", Ok("<html>")),
            Outcome::Failed(_)
        ));
        assert!(matches!(
            Outcome::from_response("1.0.0", Err("HTTP 403")),
            Outcome::Failed(_)
        ));
    }

    #[test]
    fn disabled_unless_asked() {
        assert!(!parse_pref(""));
        assert!(!parse_pref("avis = non\n"));
        assert!(parse_pref("# commentaire\navis = oui\n"));
        assert!(!parse_pref("# avis = oui\n"));
    }

    #[test]
    fn canonical_and_historical_release_urls_remain_strictly_bound_to_the_tag() {
        for prefix in [RELEASES_PREFIX, LEGACY_RELEASES_PREFIX] {
            let json = reponse("v1.3.0", &format!("{prefix}tag/v1.3.0"), false);
            assert!(matches!(
                Outcome::from_response("1.2.1", Ok(&json)),
                Outcome::Available(_)
            ));
            assert!(
                parse_latest(&reponse("v1.3.0", &format!("{prefix}tag/v9.0.0"), false)).is_none()
            );
            assert!(
                parse_latest(&reponse("v1.3.0", &format!("{prefix}../../other"), false)).is_none()
            );
        }
    }
}
