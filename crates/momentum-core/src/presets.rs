//! Préréglages d'égaliseur : ceux de Smart Control Plus connus par capture,
//! et les préréglages personnels enregistrés sur le PC.
//!
//! Un préréglage n'existe pas dans le casque : l'app (et ce projet)
//! l'applique bande par bande (`1001`). Seule la courbe compte.
//!
//! Fichier des préréglages personnels : une ligne par préréglage,
//! `nom = g1;g2;…` en dixièmes de dB ; `#` commence un commentaire.

use crate::device::EqState;
use crate::i18n::{tr, trf};

/// Topologie documentée, pas une mesure des fréquences par GAIA.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EqTopology {
    pub model: String,
    pub frequencies_hz: Vec<u16>,
    pub min_gain: i8,
    pub max_gain: i8,
}

impl EqTopology {
    /// Réservé au M5 identifié par le service (ou à sa démonstration).
    /// Sans contrat connu à huit bandes, aucune association n'est proposée.
    pub fn momentum5(eq: &EqState) -> Option<Self> {
        (eq.gains.len() == crate::ui::M5_EQ_FREQUENCIES_HZ.len() && eq.min_gain <= eq.max_gain)
            .then(|| Self {
                model: "M5AEBT".into(),
                frequencies_hz: crate::ui::M5_EQ_FREQUENCIES_HZ.to_vec(),
                min_gain: eq.min_gain,
                max_gain: eq.max_gain,
            })
    }

    pub fn check_gains(&self, gains: &[i8]) -> Result<(), &'static str> {
        if gains.len() != self.frequencies_hz.len()
            || gains
                .iter()
                .any(|g| *g < self.min_gain || *g > self.max_gain)
        {
            return Err(tr(
                "Courbe incompatible avec les bandes ou les limites de cet égaliseur.",
            ));
        }
        Ok(())
    }
}

/// Métadonnée en commentaire : les anciennes versions lisent encore les
/// lignes de gains. Une entrée non qualifiée reste conservée, mais refusée.
#[derive(serde::Serialize, serde::Deserialize)]
struct PresetTopology {
    name: String,
    topology: EqTopology,
}
/// Préréglage : nom et gains en dixièmes de dB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub gains: Vec<i8>,
    /// Préréglage de l'app (non modifiable, non supprimable).
    pub builtin: bool,
    pub topology: Option<EqTopology>,
}

impl Preset {
    pub fn check_topology(&self, current: &EqTopology) -> Result<(), &'static str> {
        let saved = self.topology.as_ref().ok_or(tr(
            "Préréglage ancien sans topologie : associez-le explicitement à cet égaliseur avant de l'appliquer.",
        ))?;
        if saved != current {
            return Err(tr(
                "La topologie de ce préréglage ne correspond pas à cet égaliseur.",
            ));
        }
        current.check_gains(&self.gains)
    }
}

/// Comparaison A/B de deux courbes d'égaliseur : on mémorise la courbe
/// actuelle comme A, on modifie (B), puis on bascule de l'une à l'autre.
/// Les deux courbes sont gardées ; seul le casque change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbCompare {
    a: Option<Vec<i8>>,
    b: Option<Vec<i8>>,
    on_a: bool,
    topology: Option<EqTopology>,
}

impl AbCompare {
    pub fn remember_for(
        &mut self,
        topology: &EqTopology,
        gains: &[i8],
    ) -> Result<(), &'static str> {
        topology.check_gains(gains)?;
        self.remember(gains);
        self.topology = Some(topology.clone());
        Ok(())
    }

    pub fn toggle_for(
        &mut self,
        topology: &EqTopology,
        current: &[i8],
    ) -> Result<Option<Vec<i8>>, &'static str> {
        if self.topology.as_ref() != Some(topology) {
            *self = Self::default();
            return Err(tr(
                "Comparaison A/B annulée : l'égaliseur ou la session a changé.",
            ));
        }
        topology.check_gains(current)?;
        Ok(self.toggle(current))
    }
    /// Mémorise `gains` comme courbe A (la suite des réglages sera B).
    pub fn remember(&mut self, gains: &[i8]) {
        self.a = Some(gains.to_vec());
        self.b = None;
        self.on_a = false;
        self.topology = None;
    }

    /// Une courbe A est mémorisée.
    pub fn ready(&self) -> bool {
        self.a.is_some()
    }

    /// Le casque est sur la courbe A.
    pub fn on_a(&self) -> bool {
        self.on_a
    }

    /// Bascule : renvoie la courbe à appliquer. `current` est la courbe du
    /// casque, gardée comme B quand on passe sur A.
    pub fn toggle(&mut self, current: &[i8]) -> Option<Vec<i8>> {
        let a = self.a.clone()?;
        if self.on_a {
            self.on_a = false;
            Some(self.b.clone().unwrap_or_else(|| current.to_vec()))
        } else {
            self.b = Some(current.to_vec());
            self.on_a = true;
            Some(a)
        }
    }

    /// Libellé du bouton de bascule.
    pub fn label(&self) -> &'static str {
        if self.on_a {
            crate::i18n::tr("Écouter B")
        } else {
            crate::i18n::tr("Écouter A")
        }
    }
}

/// Longueur maximale d'un nom de préréglage.
pub const MAX_NAME_CHARS: usize = 32;

/// Préréglages de l'app dont la courbe est connue (égaliseur à 8 bandes) :
/// « Neutre » (tout à 0) et « Audiophile » (notification `1082` relevée
/// dans la capture du 09/10/2026). Les autres sont à relever dans l'app.
pub fn builtin_presets(band_count: usize) -> Vec<Preset> {
    if band_count != 8 {
        return Vec::new();
    }
    vec![
        Preset {
            name: "Neutre".to_string(),
            gains: vec![0; 8],
            builtin: true,
            topology: EqTopology::momentum5(&EqState {
                min_gain: -60,
                max_gain: 60,
                gains: vec![0; 8],
                ..EqState::default()
            }),
        },
        Preset {
            name: "Audiophile".to_string(),
            gains: vec![-35, -10, 10, 6, -10, -5, -5, 30],
            builtin: true,
            topology: EqTopology::momentum5(&EqState {
                min_gain: -60,
                max_gain: 60,
                gains: vec![0; 8],
                ..EqState::default()
            }),
        },
    ]
}

/// Lit les préréglages personnels ; les lignes invalides sont ignorées.
pub fn parse_user_presets(text: &str) -> Vec<Preset> {
    let mut presets: Vec<Preset> = Vec::new();
    let topology: Vec<PresetTopology> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("# topologie = "))
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, gains)) = line.split_once('=') else {
            continue;
        };
        let Ok(name) = validate_name(name) else {
            continue;
        };
        let gains: Result<Vec<i8>, _> = gains.split(';').map(|g| g.trim().parse()).collect();
        if let Ok(gains) = gains
            && !gains.is_empty()
        {
            presets.retain(|p| p.name != name);
            presets.push(Preset {
                name,
                gains,
                builtin: false,
                topology: None,
            });
        }
    }
    for p in &mut presets {
        p.topology = topology
            .iter()
            .rev()
            .find(|t| t.name == p.name)
            .map(|t| t.topology.clone());
    }
    presets
}

/// Écrit les préréglages personnels (les préréglages de l'app sont omis).
pub fn serialize_user_presets(presets: &[Preset]) -> String {
    let mut out = String::from(
        "# Préréglages d'égaliseur de momentum-ctl-win : nom = gains en dixièmes de dB\n",
    );
    for p in presets.iter().filter(|p| !p.builtin) {
        if let Some(topology) = &p.topology {
            let metadata = PresetTopology {
                name: p.name.clone(),
                topology: topology.clone(),
            };
            if let Ok(json) = serde_json::to_string(&metadata) {
                out.push_str(&format!("# topologie = {json}\n"));
            }
        }
        let gains: Vec<String> = p.gains.iter().map(i8::to_string).collect();
        out.push_str(&format!("{} = {}\n", p.name, gains.join(";")));
    }
    out
}

/// Nom accepté : 1 à 32 caractères après nettoyage, sans `=`, `#` ni
/// retour à la ligne, différent des préréglages de l'app.
pub fn validate_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(tr("nom vide"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(tr("nom trop long (32 caractères au plus)"));
    }
    if name.contains(['=', '#', '\n', '\r']) {
        return Err(tr("caractères = et # interdits"));
    }
    if builtin_presets(8)
        .iter()
        .any(|p| p.name.eq_ignore_ascii_case(name))
    {
        return Err(tr("nom réservé à un préréglage de l'app"));
    }
    Ok(name.to_string())
}

/// Ajoute ou remplace un préréglage personnel (même nom).
pub fn upsert(presets: &mut Vec<Preset>, name: &str, gains: &[i8]) -> Result<(), &'static str> {
    let name = validate_name(name)?;
    presets.retain(|p| p.builtin || p.name != name);
    presets.push(Preset {
        name,
        gains: gains.to_vec(),
        builtin: false,
        topology: None,
    });
    Ok(())
}

/// Enregistrement explicite de la topologie, sans écriture au casque.
pub fn upsert_for(
    presets: &mut Vec<Preset>,
    name: &str,
    gains: &[i8],
    topology: &EqTopology,
) -> Result<(), &'static str> {
    topology.check_gains(gains)?;
    upsert(presets, name, gains)?;
    if let Some(p) = presets.last_mut() {
        p.topology = Some(topology.clone());
    }
    Ok(())
}

/// Association demandée par l'utilisateur. Les anciens gains sont conservés,
/// y compris ceux qu'on ne peut pas associer à la topologie courante.
pub fn bind_legacy(presets: &mut [Preset], topology: &EqTopology) -> usize {
    let mut count = 0;
    for p in presets
        .iter_mut()
        .filter(|p| !p.builtin && p.topology.is_none())
    {
        if topology.check_gains(&p.gains).is_ok() {
            p.topology = Some(topology.clone());
            count += 1;
        }
    }
    count
}

/// Supprime un préréglage personnel ; ceux de l'app restent.
pub fn remove(presets: &mut Vec<Preset>, name: &str) -> bool {
    let before = presets.len();
    presets.retain(|p| p.builtin || p.name != name);
    presets.len() != before
}

/// Gains en dB pour la saisie : « -3,5 ; -1 ; 0 ; … » (virgule décimale,
/// point-virgule entre les bandes).
pub fn format_gains_db(gains: &[i8]) -> String {
    gains
        .iter()
        .map(|&g| {
            if g % 10 == 0 {
                (g / 10).to_string()
            } else {
                format!("{:.1}", f32::from(g) / 10.0).replace('.', ",")
            }
        })
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Lit des gains saisis en dB (virgule ou point décimal, séparés par des
/// points-virgules) et les convertit en dixièmes de dB. Le nombre de bandes
/// et les bornes du casque sont vérifiés ; au plus une décimale.
pub fn parse_gains_db(text: &str, bands: usize, min: i8, max: i8) -> Result<Vec<i8>, String> {
    let values: Vec<&str> = text
        .split(';')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    if values.len() != bands {
        return Err(trf(
            "{} valeurs saisies pour {} bandes (séparer les gains par « ; »)",
            &[&(values.len()), &bands],
        ));
    }
    values
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let normalized = raw.replace(',', ".");
            let tenths = normalized
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v * 10.0)
                .filter(|t| (t - t.round()).abs() < 1e-6)
                .ok_or_else(|| {
                    format!(
                        "bande {} : « {raw} » n'est pas un gain en dB (une décimale au plus)",
                        i + 1
                    )
                })?;
            let (lo, hi) = (f64::from(min), f64::from(max));
            if tenths < lo || tenths > hi {
                return Err(trf(
                    "bande {} : {} dB hors de la plage du casque ({} à {} dB)",
                    &[
                        &(i + 1),
                        &raw,
                        &(format_gains_db(&[min])),
                        &(format_gains_db(&[max])),
                    ],
                ));
            }
            // Borné par [min, max] ci-dessus : la conversion est exacte.
            Ok(tenths.round() as i8)
        })
        .collect()
}

/// Version du format de fichier des courbes.
pub const CURVE_VERSION: u32 = 1;

/// Courbe lue dans un fichier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Curve {
    pub gains: Vec<i8>,
    /// Amplification des basses, si le fichier la précise.
    pub bass_boost: Option<bool>,
}

/// Fichier de courbe : version, modèle, nombre de bandes, gains en dB et
/// basses. Les fréquences et le type des filtres ne sont pas écrits : ils
/// sont fixés par le casque.
pub fn format_curve(model: Option<&str>, gains: &[i8], bass_boost: bool) -> String {
    let mut out = format!(
        "# Courbe d'égaliseur de momentum-ctl-win (gains en dB, une valeur par bande).\nversion = {CURVE_VERSION}\n"
    );
    if let Some(model) = model {
        out.push_str(&format!("modele = {model}\n"));
    }
    out.push_str(&format!(
        "bandes = {}\ngains = {}\nbasses = {}\n",
        gains.len(),
        format_gains_db(gains),
        if bass_boost { "on" } else { "off" }
    ));
    out
}

/// Courbe au format GraphicEQ d'Equalizer APO (Windows) : une paire
/// « fréquence gain » par bande du MOMENTUM 5, pour reproduire la courbe
/// côté PC. Equalizer APO interpole entre les points ; la forme exacte des
/// filtres du casque n'est pas documentée, la correspondance est donc
/// approchée. L'amplification des basses n'est pas reproduite. Refusée si
/// l'égaliseur n'a pas les 8 bandes connues.
pub fn format_graphic_eq(gains: &[i8]) -> Result<String, String> {
    let freqs = crate::ui::M5_EQ_FREQUENCIES_HZ;
    if gains.len() != freqs.len() {
        return Err(trf(
            "export Equalizer APO : {} bandes, {} attendues",
            &[&gains.len(), &freqs.len()],
        ));
    }
    let points: Vec<String> = freqs
        .iter()
        .zip(gains)
        .map(|(hz, &g)| format!("{hz} {:.1}", f32::from(g) / 10.0))
        .collect();
    Ok(format!(
        "# Courbe du MOMENTUM 5 exportée par OpenHeadset Control (format GraphicEQ d'Equalizer APO).\n\
         # Correspondance approchée : filtres du casque non documentés ; amplification des basses non reproduite.\n\
         GraphicEQ: {}\n",
        points.join("; ")
    ))
}

/// Lit un fichier de courbe pour un égaliseur de `bands` bandes. Une courbe
/// d'un autre modèle (six premiers caractères du nom de modèle) est refusée.
pub fn parse_curve(
    text: &str,
    model: Option<&str>,
    bands: usize,
    min: i8,
    max: i8,
) -> Result<Curve, String> {
    let mut version = None;
    let mut file_model = None;
    let mut file_bands = None;
    let mut gains = None;
    let mut bass_boost = None;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(trf("ligne illisible : « {} »", &[&line]));
        };
        let value = value.trim();
        match key.trim() {
            "version" => version = Some(value.to_string()),
            "modele" => file_model = Some(value.to_string()),
            "bandes" => file_bands = value.parse::<usize>().ok(),
            "gains" => gains = Some(value.to_string()),
            "basses" => {
                bass_boost = Some(match value {
                    "on" => true,
                    "off" => false,
                    _ => return Err(tr("basses : on ou off").to_string()),
                })
            }
            other => return Err(trf("réglage inconnu « {} »", &[&other])),
        }
    }
    if version.as_deref() != Some(&CURVE_VERSION.to_string()) {
        return Err(trf(
            "version de fichier « {} » non prise en charge (attendue : {})",
            &[&(version.unwrap_or_default()), &CURVE_VERSION],
        ));
    }
    let prefix = |m: &str| m.chars().take(6).collect::<String>().to_uppercase();
    if let (Some(file), Some(current)) = (&file_model, model)
        && prefix(file) != prefix(current)
    {
        return Err(trf(
            "courbe prévue pour le modèle « {} », pas pour « {} »",
            &[&file, &current],
        ));
    }
    if file_bands.is_some_and(|b| b != bands) {
        return Err(trf(
            "courbe à {} bandes, l'égaliseur du casque en a {}",
            &[&(file_bands.unwrap_or_default()), &bands],
        ));
    }
    let gains = parse_gains_db(
        &gains.ok_or_else(|| tr("ligne « gains = … » manquante").to_string())?,
        bands,
        min,
        max,
    )?;
    Ok(Curve { gains, bass_boost })
}

/// Préréglage dont la courbe est exactement celle du casque.
pub fn matching<'a>(presets: &'a [Preset], gains: &[i8]) -> Option<&'a Preset> {
    presets.iter().find(|p| p.gains == gains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphic_eq_export() {
        let text = format_graphic_eq(&[15, 0, -10, 0, 5, 0, 0, -60]).unwrap();
        assert!(
            text.ends_with("GraphicEQ: 90 1.5; 325 0.0; 1500 -1.0; 2500 0.0; 4500 0.5; 8000 0.0; 13000 0.0; 16000 -6.0\n"),
            "{text}"
        );
        assert!(text.lines().filter(|l| !l.starts_with('#')).count() == 1);
        assert!(format_graphic_eq(&[0; 5]).is_err());
    }

    #[test]
    fn ab_compare_switches_between_both_curves() {
        let mut ab = AbCompare::default();
        assert!(!ab.ready());
        assert_eq!(ab.toggle(&[0, 0]), None);
        ab.remember(&[10, 20]);
        assert!(ab.ready() && !ab.on_a());
        // Réglages modifiés (B), puis écoute de A.
        assert_eq!(ab.toggle(&[-5, 5]), Some(vec![10, 20]));
        assert!(ab.on_a());
        assert_eq!(ab.label(), "Écouter B");
        // Retour sur B, même si le casque a changé entre-temps.
        assert_eq!(ab.toggle(&[10, 20]), Some(vec![-5, 5]));
        assert!(!ab.on_a());
        // Une nouvelle mémorisation oublie l'ancien B.
        ab.remember(&[1, 1]);
        assert_eq!(ab.toggle(&[2, 2]), Some(vec![1, 1]));
        assert_eq!(ab.toggle(&[1, 1]), Some(vec![2, 2]));
    }

    #[test]
    fn typed_gains_use_a_decimal_comma_and_are_checked() {
        let gains = [-35, -10, 10, 6, -10, -5, -5, 30];
        let text = format_gains_db(&gains);
        assert_eq!(text, "-3,5 ; -1 ; 1 ; 0,6 ; -1 ; -0,5 ; -0,5 ; 3");
        assert_eq!(parse_gains_db(&text, 8, -60, 60).unwrap(), gains);
        assert_eq!(
            parse_gains_db("1.5;0;0;0;0;0;0;-6", 8, -60, 60).unwrap(),
            [15, 0, 0, 0, 0, 0, 0, -60]
        );
        for (bad, why) in [
            ("1;2;3", "3 valeurs"),
            ("7;0;0;0;0;0;0;0", "hors de la plage"),
            ("0,25;0;0;0;0;0;0;0", "une décimale"),
            ("x;0;0;0;0;0;0;0", "n'est pas un gain"),
            ("NaN;0;0;0;0;0;0;0", "n'est pas un gain"),
        ] {
            let err = parse_gains_db(bad, 8, -60, 60).unwrap_err();
            assert!(err.contains(why), "{bad} : {err}");
        }
    }

    #[test]
    fn curve_files_round_trip_and_refuse_other_models() {
        let gains = [-35, -10, 10, 6, -10, -5, -5, 30];
        let text = format_curve(Some("M5AEBT Black"), &gains, true);
        assert_eq!(
            parse_curve(&text, Some("M5AEBT White"), 8, -60, 60),
            Ok(Curve {
                gains: gains.to_vec(),
                bass_boost: Some(true)
            })
        );
        let err = parse_curve(&text, Some("M4AEBT"), 8, -60, 60).unwrap_err();
        assert!(err.contains("modèle"), "{err}");
        assert!(parse_curve(&text, Some("M5AEBT"), 5, -60, 60).is_err());
        assert!(
            parse_curve(
                &text.replace("version = 1", "version = 2"),
                None,
                8,
                -60,
                60
            )
            .is_err()
        );
        assert!(parse_curve("version = 1\nbasses = on\n", None, 8, -60, 60).is_err());
    }

    #[test]
    fn user_presets_round_trip() {
        let mut presets = Vec::new();
        upsert(&mut presets, "  Basses ", &[30, 20, 0, 0, 0, 0, 0, -10]).unwrap();
        upsert(&mut presets, "Voix", &[0, -10, 20, 30, 20, 0, 0, 0]).unwrap();
        let text = serialize_user_presets(&presets);
        assert_eq!(parse_user_presets(&text), presets);
        assert_eq!(presets[0].name, "Basses");
    }

    #[test]
    fn invalid_lines_and_names_are_rejected() {
        let text = "# commentaire\nsans signe égal\nA = 1;x;3\n = 1;2\nB = 5;6\nB = 7;8\n";
        let presets = parse_user_presets(text);
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].gains, [7, 8]);
        assert!(validate_name("Neutre").is_err());
        assert!(validate_name("a=b").is_err());
        assert!(validate_name(&"x".repeat(33)).is_err());
    }

    #[test]
    fn builtin_presets_cannot_be_removed_and_are_matched() {
        let mut presets = builtin_presets(8);
        assert!(!remove(&mut presets, "Neutre"));
        assert_eq!(
            matching(&presets, &[-35, -10, 10, 6, -10, -5, -5, 30]).map(|p| p.name.as_str()),
            Some("Audiophile")
        );
        assert!(matching(&presets, &[1; 8]).is_none());
        // Égaliseur d'une autre forme : pas de préréglage de l'app.
        assert!(builtin_presets(5).is_empty());
    }
}
