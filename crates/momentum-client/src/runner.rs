//! Recherche du bureau (KRunner sous KDE) : d'une saisie comme « anc off »
//! ou « scène Transport » aux réglages proposés, et d'un résultat choisi à
//! la [`Command`] de l'API. Le service l'expose (`org.kde.krunner1`) et
//! envoie la commande par le même chemin que les interfaces : mêmes refus
//! (casque absent, lecture seule, scène inconnue).

use momentum_core::device::NoiseModeRequest;

use crate::Command;

/// Résultat proposé pour une saisie.
#[derive(Clone, Debug, PartialEq)]
pub struct RunnerMatch {
    /// Identifiant rendu à [`command_for`] au choix du résultat.
    pub id: String,
    pub text: String,
    pub subtext: String,
    /// La saisie est l'un des mots-clés : résultat placé en tête.
    pub exact: bool,
    /// 0 à 1, pour l'ordre des résultats.
    pub relevance: f64,
}

/// Réglages proposés : identifiant, texte, mots-clés.
const ACTIONS: &[(&str, &str, &[&str])] = &[
    (
        "mode:off",
        "Réduction de bruit désactivée",
        &["anc off", "sans anc", "bruit off", "reduction de bruit off"],
    ),
    (
        "mode:adaptive",
        "Réduction de bruit adaptative",
        &["anc adaptatif", "adaptatif", "anc auto"],
    ),
    (
        "level:0",
        "Réduction de bruit maximale",
        &["anc", "anc on", "anc max", "reduction de bruit"],
    ),
    (
        "level:100",
        "Transparence maximale",
        &["transparence", "transparent", "anc transparence"],
    ),
    (
        "antivent:1",
        "Anti-vent activé",
        &["anti-vent on", "antivent on", "vent on"],
    ),
    (
        "antivent:0",
        "Anti-vent désactivé",
        &["anti-vent off", "antivent off", "vent off"],
    ),
    (
        "spatial:1",
        "Audio spatial activé",
        &["spatial on", "audio spatial on"],
    ),
    (
        "spatial:0",
        "Audio spatial désactivé",
        &["spatial off", "audio spatial off"],
    ),
];

const SCENE_WORDS: &[&str] = &["scene", "scenes"];
const SUBTEXT: &str = "Casque MOMENTUM 5 (OpenHeadset Control)";

/// Minuscules sans accents ni espaces superflus, pour comparer les saisies.
pub fn normalize(text: &str) -> String {
    let lower: String = text
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '’' | '\'' => ' ',
            c => c,
        })
        .collect();
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Résultats pour une saisie. `scenes` : noms de `scenes.txt` ;
/// `battery` : pourcentage connu (casque connecté), pour l'information.
pub fn matches(query: &str, scenes: &[String], battery: Option<u8>) -> Vec<RunnerMatch> {
    let q = normalize(query);
    if q.chars().count() < 3 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (id, text, keywords) in ACTIONS {
        if let Some(exact) = keyword_match(&q, keywords) {
            out.push(RunnerMatch {
                id: (*id).to_string(),
                text: (*text).to_string(),
                subtext: SUBTEXT.to_string(),
                exact,
                relevance: if exact { 1.0 } else { 0.6 },
            });
        }
    }
    // « scène transport », « scene trans », ou le nom seul.
    let (scene_query, prefixed) = match q.split_once(' ') {
        Some((word, rest)) if SCENE_WORDS.contains(&word) => (rest.to_string(), true),
        _ if SCENE_WORDS.contains(&q.as_str()) => (String::new(), true),
        _ => (q.clone(), false),
    };
    for name in scenes {
        let n = normalize(name);
        let exact = n == scene_query;
        if exact || (n.starts_with(&scene_query) && (prefixed || scene_query.len() >= 3)) {
            out.push(RunnerMatch {
                id: format!("scene:{name}"),
                text: format!("Scène « {name} »"),
                subtext: SUBTEXT.to_string(),
                exact: exact && (prefixed || !scene_query.is_empty()),
                relevance: if exact { 0.95 } else { 0.55 },
            });
        }
    }
    if ["batterie", "casque"].iter().any(|w| w.starts_with(&q)) {
        let text = match battery {
            Some(level) => format!("Batterie du casque : {level} %"),
            None => "Casque non connecté".to_string(),
        };
        out.push(RunnerMatch {
            id: "info:battery".to_string(),
            text,
            subtext: SUBTEXT.to_string(),
            exact: false,
            relevance: 0.5,
        });
    }
    out
}

/// `Some(true)` : la saisie est un mot-clé ; `Some(false)` : le début d'un
/// mot-clé (3 lettres au moins).
fn keyword_match(q: &str, keywords: &[&str]) -> Option<bool> {
    if keywords.contains(&q) {
        Some(true)
    } else if keywords.iter().any(|k| k.starts_with(q)) {
        Some(false)
    } else {
        None
    }
}

/// Commande de l'API pour un résultat choisi ; `None` : identifiant
/// inconnu, ou résultat d'information seule (batterie).
pub fn command_for(id: &str) -> Option<Command> {
    let (kind, value) = id.split_once(':')?;
    Some(match (kind, value) {
        ("mode", "off") => Command::SetMode(NoiseModeRequest::Off),
        ("mode", "adaptive") => Command::SetMode(NoiseModeRequest::Adaptive),
        ("level", "0") => Command::SetLevel(0),
        ("level", "100") => Command::SetLevel(100),
        ("antivent", on) => Command::SetAntiWind(parse_switch(on)?),
        ("spatial", on) => Command::SetSpatial(parse_switch(on)?),
        ("scene", name) if !name.is_empty() => Command::ApplyScene(name.to_string()),
        _ => return None,
    })
}

fn parse_switch(value: &str) -> Option<bool> {
    match value {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenes() -> Vec<String> {
        vec!["Transport".into(), "Travail".into(), "Écoute".into()]
    }

    fn ids(query: &str) -> Vec<String> {
        matches(query, &scenes(), Some(80))
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    #[test]
    fn roadmap_examples() {
        let anc_off = matches("anc off", &scenes(), None);
        assert_eq!(anc_off[0].id, "mode:off");
        assert!(anc_off[0].exact);
        assert_eq!(
            command_for(&anc_off[0].id),
            Some(Command::SetMode(NoiseModeRequest::Off))
        );

        let scene = matches("scène Transport", &scenes(), None);
        assert_eq!(scene.len(), 1);
        assert_eq!(scene[0].id, "scene:Transport");
        assert!(scene[0].exact);
        assert_eq!(
            command_for(&scene[0].id),
            Some(Command::ApplyScene("Transport".into()))
        );
    }

    #[test]
    fn accents_case_and_prefixes() {
        assert_eq!(ids("SCENE ecoute"), ["scene:Écoute"]);
        assert_eq!(ids("scène tra"), ["scene:Transport", "scene:Travail"]);
        assert!(ids("transp").contains(&"level:100".to_string()));
        assert!(ids("Anti-Vent OFF").contains(&"antivent:0".to_string()));
        // Le nom seul d'une scène suffit.
        assert_eq!(ids("transport"), ["scene:Transport"]);
    }

    #[test]
    fn short_or_unrelated_queries_give_nothing() {
        assert!(ids("an").is_empty());
        assert!(ids("firefox").is_empty());
        assert!(ids("").is_empty());
    }

    #[test]
    fn battery_is_information_only() {
        let m = matches("batt", &scenes(), Some(80));
        assert_eq!(m[0].text, "Batterie du casque : 80 %");
        assert_eq!(command_for("info:battery"), None);
        assert_eq!(
            matches("batterie", &[], None)[0].text,
            "Casque non connecté"
        );
    }

    #[test]
    fn unknown_ids_send_nothing() {
        for id in [
            "",
            "mode",
            "mode:custom",
            "level:50",
            "antivent:2",
            "scene:",
            "x:y",
        ] {
            assert_eq!(command_for(id), None, "{id}");
        }
        // Une scène inconnue part au service, qui la refuse comme pour les
        // autres interfaces.
        assert_eq!(
            command_for("scene:Inconnue"),
            Some(Command::ApplyScene("Inconnue".into()))
        );
    }
}
