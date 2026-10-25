//! Diagnostic expurgé, à copier dans un rapport de bogue (Rust pur).
//!
//! Contenu : versions, modèle et micrologiciel, réglages lus ou non (sans
//! leurs valeurs personnelles), incidents récents. Jamais : nom du casque ou
//! des appareils, adresse Bluetooth, numéro de série, chemin personnel, profil
//! auditif. Le texte est montré avant toute copie ; rien n'est envoyé.
//! Le rapport de bogue en un clic ouvre le formulaire GitHub prérempli dans
//! le navigateur : l'utilisateur relit et envoie lui-même.

use crate::device::{DeviceInfo, HeadsetState};

/// Remplacement des éléments masqués.
pub const MASK: &str = "‹masqué›";

/// Masque les adresses Bluetooth (`AA:BB:CC:DD:EE:FF` ou avec `-`, quelle
/// que soit la casse).
pub fn mask_addresses(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_address = |at: usize| -> bool {
        if at + 17 > chars.len() {
            return false;
        }
        let sep = chars[at + 2];
        if sep != ':' && sep != '-' {
            return false;
        }
        (0..6).all(|g| {
            let i = at + g * 3;
            chars[i].is_ascii_hexdigit()
                && chars[i + 1].is_ascii_hexdigit()
                && (g == 5 || chars[i + 2] == sep)
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        // Pas au milieu d'un mot hexadécimal plus long.
        let starts_word = i == 0 || !chars[i - 1].is_ascii_hexdigit();
        if starts_word && is_address(i) {
            out.push_str(MASK);
            i += 17;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Masque les adresses, puis chaque texte de `secrets` (noms d'appareils,
/// dossier de l'utilisateur…), sans tenir compte de la casse.
pub fn redact(text: &str, secrets: &[&str]) -> String {
    let mut out = mask_addresses(text);
    for secret in secrets.iter().map(|s| s.trim()).filter(|s| s.len() >= 3) {
        out = mask_secret(&out, &secret.to_lowercase());
    }
    out
}

/// Remplace chaque occurrence de `needle` (déjà en minuscules) dans `text`.
///
/// La comparaison se fait sur le texte en minuscules, dont les positions ne
/// valent pas toujours celles de `text` : certains caractères changent de
/// longueur en UTF-8 (`İ` → `i̇`, `ẞ` → `ß`). Chaque caractère d'origine
/// garde donc la position de sa forme en minuscules ; une occurrence qui ne
/// commence ou ne finit pas sur un caractère entier masque tout le texte, par
/// prudence.
fn mask_secret(text: &str, needle: &str) -> String {
    // (position dans `lower`, position dans `text`) au début de chaque
    // caractère, plus la fin des deux textes.
    let mut lower = String::with_capacity(text.len());
    let mut starts = Vec::with_capacity(text.len() + 1);
    for (at, c) in text.char_indices() {
        starts.push((lower.len(), at));
        lower.extend(c.to_lowercase());
    }
    starts.push((lower.len(), text.len()));
    let original = |at: usize| -> Option<usize> {
        starts
            .binary_search_by_key(&at, |&(l, _)| l)
            .ok()
            .map(|i| starts[i].1)
    };
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for (start, _) in lower.match_indices(needle) {
        let (Some(from), Some(to)) = (original(start), original(start + needle.len())) else {
            return MASK.to_string();
        };
        result.push_str(&text[last..from]);
        result.push_str(MASK);
        last = to;
    }
    result.push_str(&text[last..]);
    result
}

/// Données rassemblées par l'application.
pub struct DiagnosticInput<'a> {
    pub app_version: &'a str,
    /// Système, si connu (« Windows 10.0.26300 », « Bazzite 44 »).
    pub os_version: Option<&'a str>,
    pub connected: bool,
    /// Raison de la lecture seule (modèle ou micrologiciel), s'il y a lieu.
    pub read_only: Option<&'a str>,
    pub info: Option<&'a DeviceInfo>,
    pub state: Option<&'a HeadsetState>,
    pub eq_bands: Option<usize>,
    pub paired_devices: Option<usize>,
    pub battery_entries: usize,
    /// Incidents récents : (secondes écoulées, message).
    pub incidents: &'a [(u64, String)],
    /// Textes à masquer en plus des adresses (noms, dossier personnel).
    pub secrets: &'a [&'a str],
}

/// Nom du système d'après le contenu de `/etc/os-release` (`PRETTY_NAME`,
/// sinon `NAME`), par exemple « Bazzite 44 ».
pub fn os_release_name(text: &str) -> Option<String> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim_matches('"').to_string())
    };
    field("PRETTY_NAME=").or_else(|| field("NAME="))
}

/// « lues : nom, langue ; non lues : priorité » (accord neutre, quel que
/// soit le genre de chaque lecture).
fn read_list(items: &[(&str, bool)]) -> String {
    let names = |read: bool| {
        items
            .iter()
            .filter(|(_, r)| *r == read)
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let (done, missing) = (names(true), names(false));
    match (done.is_empty(), missing.is_empty()) {
        (_, true) => format!("lues : {done}"),
        (true, false) => format!("aucune lue ; non lues : {missing}"),
        (false, false) => format!("lues : {done} ; non lues : {missing}"),
    }
}

/// Texte du diagnostic, déjà expurgé.
pub fn build(input: &DiagnosticInput) -> String {
    let mut lines = vec![
        "Diagnostic momentum-ctl-win (expurgé : ni nom, ni adresse, ni numéro de série, ni profil auditif)".to_string(),
        format!("Application : {}", input.app_version),
        format!(
            "Système : {}",
            input.os_version.unwrap_or("version inconnue")
        ),
        format!(
            "Casque : {}",
            if input.connected {
                "connecté"
            } else {
                "non connecté"
            }
        ),
    ];
    if let Some(reason) = input.read_only {
        lines.push(format!("Lecture seule : {reason}"));
    }
    if let Some(info) = input.info {
        lines.push(format!(
            "Modèle : {}",
            info.model.as_deref().unwrap_or("non lu")
        ));
        lines.push(format!(
            "Micrologiciel : {}",
            info.firmware.as_deref().unwrap_or("non lu")
        ));
        lines.push(format!(
            "Révision matérielle : {}",
            info.hardware_revision
                .map_or("non lue".to_string(), |r| r.to_string())
        ));
        lines.push(format!(
            "Lectures annexes — {}",
            read_list(&[
                ("nom", info.name.is_some()),
                ("langue", info.prompt_language.is_some()),
                ("volume des invites", info.prompt_volume.is_some()),
                ("priorité", info.audio_priority.is_some()),
                ("Multipoint", info.multipoint.is_some()),
            ]),
        ));
    }
    if let Some(s) = input.state {
        lines.push(format!(
            "Réglages — {}",
            read_list(&[
                ("spatial", s.spatial_audio.is_some()),
                ("suivi de tête", s.head_tracking.is_some()),
                ("mode audio", s.audio_mode.is_some()),
                ("profil", s.sound_profile.is_some()),
                ("tactile", s.touch_controls.is_some()),
                ("protection batterie", s.battery_protection.is_some()),
                ("Smart Pause", s.smart_pause.is_some()),
                ("réponse auto", s.auto_answer.is_some()),
                ("appels confortables", s.comfort_call.is_some()),
                ("extinction", s.auto_off_seconds.is_some()),
                ("invites", s.prompt_mode.is_some()),
                ("codec", s.codec.is_some()),
                ("fréquence", s.sample_rate_hz.is_some()),
            ]),
        ));
    }
    if let Some(bands) = input.eq_bands {
        lines.push(format!("Égaliseur : {bands} bandes"));
    }
    if let Some(count) = input.paired_devices {
        lines.push(format!("Appareils appairés au casque : {count}"));
    }
    lines.push(format!(
        "Historique de la batterie : {} lignes",
        input.battery_entries
    ));
    if input.incidents.is_empty() {
        lines.push("Incidents récents : aucun".to_string());
    } else {
        lines.push("Incidents récents :".to_string());
        for (age, message) in input.incidents {
            lines.push(format!("  il y a {} min : {message}", age / 60));
        }
    }
    redact(&lines.join("\n"), input.secrets)
}

/// Formulaire « Bogue » du dépôt (`.github/ISSUE_TEMPLATE/bogue.yml`).
pub const ISSUE_FORM: &str =
    "https://github.com/Asfax03L/openheadset-control/issues/new?template=bogue.yml";
/// Choix « Interface » du formulaire, au mot près.
pub const ISSUE_INTERFACE_WINUI: &str = "WinUI 3 (Windows)";
pub const ISSUE_INTERFACE_KDE: &str = "Client KDE (Linux)";
/// Longueur d'adresse sûre pour les navigateurs et GitHub.
const MAX_ISSUE_URL: usize = 7000;
/// Fin d'un diagnostic raccourci pour tenir dans l'adresse.
const TRUNCATED: &str =
    "\n[… diagnostic raccourci : « Copier le diagnostic » donne le texte complet]";

/// Encodage d'un paramètre d'adresse (RFC 3986 : seuls les caractères non
/// réservés restent tels quels).
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Adresse du formulaire « Bogue » prérempli : version, interface,
/// système, diagnostic (déjà expurgé). Diagnostic raccourci si l'adresse
/// dépasse la longueur sûre. Rien n'est envoyé : la page s'ouvre dans le
/// navigateur, l'utilisateur relit et envoie.
pub fn issue_url(version: &str, interface: &str, system: Option<&str>, diagnostic: &str) -> String {
    let head = format!(
        "{ISSUE_FORM}&version={}&interface={}&systeme={}&diagnostic=",
        percent_encode(version),
        percent_encode(interface),
        percent_encode(system.unwrap_or_default()),
    );
    let full = percent_encode(diagnostic);
    if head.len() + full.len() <= MAX_ISSUE_URL {
        return head + &full;
    }
    let room = MAX_ISSUE_URL.saturating_sub(head.len() + percent_encode(TRUNCATED).len());
    // Coupe sur une fin de ligne, sinon sur un caractère entier.
    let mut cut = String::new();
    for line in diagnostic.split_inclusive('\n') {
        if percent_encode(&cut).len() + percent_encode(line).len() > room {
            break;
        }
        cut.push_str(line);
    }
    if cut.is_empty() {
        for c in diagnostic.chars() {
            if percent_encode(&cut).len() + c.len_utf8() * 3 > room {
                break;
            }
            cut.push(c);
        }
    }
    head + &percent_encode(cut.trim_end()) + &percent_encode(TRUNCATED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_url_prefills_the_bug_form() {
        let url = issue_url(
            "0.16.0",
            ISSUE_INTERFACE_WINUI,
            Some("Windows 11"),
            "Casque : connecté\nA&B",
        );
        assert!(url.starts_with(ISSUE_FORM));
        assert!(url.contains("&version=0.16.0&"));
        assert!(url.contains("&interface=WinUI%203%20%28Windows%29&"));
        assert!(url.contains("&systeme=Windows%2011&"));
        assert!(url.ends_with("&diagnostic=Casque%20%3A%20connect%C3%A9%0AA%26B"));
    }

    #[test]
    fn long_diagnostic_is_cut_on_a_line() {
        let long: String = (0..2000).map(|i| format!("ligne {i} é\n")).collect();
        let url = issue_url("0.16.0", ISSUE_INTERFACE_KDE, None, &long);
        assert!(url.len() <= MAX_ISSUE_URL, "{}", url.len());
        assert!(url.ends_with(&percent_encode(TRUNCATED)));
        assert!(url.contains(&percent_encode("ligne 0 é\nligne 1 é")));
        // Aucune ligne coupée en deux.
        let shown = url.trim_end_matches(&percent_encode(TRUNCATED));
        assert!(shown.ends_with("%C3%A9"), "{}", &shown[shown.len() - 30..]);
    }

    #[test]
    fn interfaces_match_the_bug_form() {
        let form = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.github/ISSUE_TEMPLATE/bogue.yml"),
        )
        .unwrap();
        for id in [
            "id: version",
            "id: interface",
            "id: systeme",
            "id: diagnostic",
        ] {
            assert!(form.contains(id), "{id} absent du formulaire");
        }
        for choice in [ISSUE_INTERFACE_WINUI, ISSUE_INTERFACE_KDE] {
            assert!(
                form.contains(&format!("- {choice}\n")),
                "{choice} absent du formulaire"
            );
        }
    }

    #[test]
    fn os_release_pretty_name() {
        let text = "NAME=\"Bazzite\"\nPRETTY_NAME=\"Bazzite 44 (FROM Fedora Kinoite)\"\n";
        assert_eq!(
            os_release_name(text).as_deref(),
            Some("Bazzite 44 (FROM Fedora Kinoite)")
        );
        assert_eq!(os_release_name("NAME=Fedora\n").as_deref(), Some("Fedora"));
        assert_eq!(os_release_name(""), None);
    }

    #[test]
    fn read_lists_agree_whatever_the_gender() {
        assert_eq!(
            read_list(&[("nom", true), ("langue", true)]),
            "lues : nom, langue"
        );
        assert_eq!(
            read_list(&[("nom", true), ("priorité", false)]),
            "lues : nom ; non lues : priorité"
        );
        assert_eq!(
            read_list(&[("codec", false)]),
            "aucune lue ; non lues : codec"
        );
    }

    #[test]
    fn addresses_are_masked_in_any_form() {
        assert_eq!(
            mask_addresses("service=Bluetooth#Bluetooth02:11:22:33:44:55-02:AA:BB:CC:DD:EE#RFCOMM"),
            format!("service=Bluetooth#Bluetooth{MASK}-{MASK}#RFCOMM")
        );
        assert_eq!(mask_addresses("AA-BB-CC-DD-EE-FF"), MASK);
        // Pas une adresse : séparateurs mêlés, trop court, mot plus long.
        for kept in [
            "AA:BB-CC:DD:EE:FF",
            "AA:BB:CC:DD:EE",
            "1AA:BB:CC:DD:EE:FF",
            "12:30",
        ] {
            assert_eq!(mask_addresses(kept), kept);
        }
    }

    #[test]
    fn secrets_are_masked_whatever_the_case() {
        assert_eq!(
            redact("Casque Écoute Léo, puis ÉCOUTE LÉO", &["écoute léo"]),
            format!("Casque {MASK}, puis {MASK}")
        );
        // Texte inchangé sans occurrence ; secrets trop courts ignorés.
        assert_eq!(redact("rien à cacher", &["absent", "à"]), "rien à cacher");
    }

    #[test]
    fn unicode_whose_length_changes_in_lowercase_never_panics() {
        // `İ` s'allonge en minuscules, `ẞ` raccourcit : la longueur totale
        // est la même, les positions non.
        assert_eq!(redact("İẞX", &["ẞX"]), format!("İ{MASK}"));
        assert_eq!(redact("ẞX İ ẞx", &["ßx"]), format!("{MASK} İ {MASK}"));
        // Occurrence qui commence au milieu d'un caractère en minuscules
        // (`İ` → `i` + point) : tout le texte est masqué par prudence.
        assert_eq!(redact("aİbc", &["\u{307}bc"]), MASK);
    }

    #[test]
    fn diagnostic_hides_names_paths_and_addresses() {
        let info = DeviceInfo {
            name: Some("MOMENTUM 5 Exemple".into()),
            model: Some("M5AEBT Black".into()),
            firmware: Some("6.21.5".into()),
            ..DeviceInfo::default()
        };
        let incidents = [(
            600,
            r"Connexion perdue : MOMENTUM 5 Exemple (02:AA:BB:CC:DD:EE), journal C:\Users\leo\AppData".to_string(),
        )];
        let text = build(&DiagnosticInput {
            app_version: "0.4.0",
            os_version: Some("Windows 10.0.26300"),
            connected: true,
            read_only: None,
            info: Some(&info),
            state: None,
            eq_bands: Some(8),
            paired_devices: Some(3),
            battery_entries: 12,
            incidents: &incidents,
            secrets: &["momentum 5 exemple", r"C:\Users\leo"],
        });
        for hidden in ["Exemple", "02:AA", r"C:\Users\leo"] {
            assert!(!text.contains(hidden), "{hidden} visible dans\n{text}");
        }
        for shown in [
            "M5AEBT Black",
            "6.21.5",
            "Lectures annexes — lues : nom",
            "il y a 10 min",
            "Égaliseur : 8 bandes",
        ] {
            assert!(text.contains(shown), "{shown} absent de\n{text}");
        }
    }
}
