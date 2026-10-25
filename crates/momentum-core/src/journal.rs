//! Journal local des événements (Rust pur) : connexions, pertes, mode de
//! réduction de bruit, refus du casque, scènes appliquées. Tenu par le
//! service dans `journal.csv` (dossier des données), consultable et
//! effaçable dans les interfaces, **jamais envoyé**.
//!
//! Une ligne `horodatage_unix;type;détail`. Le détail ne contient ni nom du
//! casque ou d'un appareil, ni adresse : les refus passent par
//! [`crate::diagnostic::mask_addresses`]. Le mode de bruit sert aussi à la
//! consommation par mode ([`crate::battery_log::consumption_by_mode`]).

use crate::i18n::{tr, trf};
use crate::protocol::messages::M5NoiseMode;

/// Durée conservée : 30 jours, comme l'historique de la batterie.
pub const RETENTION_SECONDS: u64 = 30 * 24 * 3600;
/// Nombre maximal de lignes gardées.
pub const MAX_ENTRIES: usize = 5_000;

/// Mode de réduction de bruit, tel que noté au journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NoiseKind {
    Off,
    Adaptive,
    Custom,
}

impl NoiseKind {
    pub fn from_mode(mode: M5NoiseMode) -> Self {
        match mode {
            M5NoiseMode::Off => NoiseKind::Off,
            M5NoiseMode::Adaptive => NoiseKind::Adaptive,
            M5NoiseMode::Custom { .. } => NoiseKind::Custom,
        }
    }

    fn code(self) -> &'static str {
        match self {
            NoiseKind::Off => "arret",
            NoiseKind::Adaptive => "adaptatif",
            NoiseKind::Custom => "personnalise",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "arret" => Some(NoiseKind::Off),
            "adaptatif" => Some(NoiseKind::Adaptive),
            "personnalise" => Some(NoiseKind::Custom),
            _ => None,
        }
    }

    /// Libellé affiché.
    pub fn label(self) -> &'static str {
        match self {
            NoiseKind::Off => tr("Réduction de bruit désactivée"),
            NoiseKind::Adaptive => tr("Adaptatif"),
            NoiseKind::Custom => tr("Personnalisé (ANC ↔ transparence)"),
        }
    }
}

/// Événement noté.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Connected,
    Lost,
    Noise(NoiseKind),
    Refused(String),
    /// Scène appliquée (nom, automatique, complète).
    Scene {
        name: String,
        automatic: bool,
        complete: bool,
    },
}

/// Une ligne du journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub time: u64,
    pub event: Event,
}

/// Détail sur une seule ligne, sans séparateur.
fn clean(text: &str) -> String {
    let text = crate::diagnostic::mask_addresses(text);
    let text: String = text
        .chars()
        .map(|c| if c == ';' || c.is_control() { ' ' } else { c })
        .collect();
    text.trim().chars().take(200).collect()
}

impl Entry {
    fn to_line(&self) -> String {
        let (kind, detail) = match &self.event {
            Event::Connected => ("connecte", String::new()),
            Event::Lost => ("perdu", String::new()),
            Event::Noise(k) => ("bruit", k.code().to_string()),
            Event::Refused(reason) => ("refus", clean(reason)),
            Event::Scene {
                name,
                automatic,
                complete,
            } => (
                "scene",
                format!(
                    "{};{};{}",
                    if *automatic { "auto" } else { "demande" },
                    if *complete { "complete" } else { "partielle" },
                    clean(name)
                ),
            ),
        };
        format!("{};{kind};{detail}", self.time)
    }

    fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.trim().splitn(3, ';');
        let time: u64 = parts.next()?.parse().ok()?;
        let kind = parts.next()?;
        let detail = parts.next().unwrap_or("");
        let event = match kind {
            "connecte" => Event::Connected,
            "perdu" => Event::Lost,
            "bruit" => Event::Noise(NoiseKind::from_code(detail)?),
            "refus" => Event::Refused(detail.to_string()),
            "scene" => {
                let mut d = detail.splitn(3, ';');
                let automatic = d.next()? == "auto";
                let complete = d.next()? == "complete";
                Event::Scene {
                    name: d.next()?.to_string(),
                    automatic,
                    complete,
                }
            }
            _ => return None,
        };
        Some(Entry { time, event })
    }

    /// Texte affiché (sans l'heure).
    pub fn describe(&self) -> String {
        match &self.event {
            Event::Connected => tr("Casque connecté").to_string(),
            Event::Lost => tr("Connexion perdue").to_string(),
            Event::Noise(k) => trf("Réduction de bruit : {}", &[&k.label()]),
            Event::Refused(reason) => trf("Réglage refusé : {}", &[reason]),
            Event::Scene {
                name,
                automatic,
                complete,
            } => match (automatic, complete) {
                (true, true) => trf("Scène « {} » appliquée automatiquement", &[name]),
                (true, false) => trf("Scène « {} » appliquée en partie (automatique)", &[name]),
                (false, true) => trf("Scène « {} » appliquée", &[name]),
                (false, false) => trf("Scène « {} » appliquée en partie", &[name]),
            },
        }
    }
}

/// Ancienneté d'un événement : « il y a 5 min ».
pub fn ago(now: u64, time: u64) -> String {
    let seconds = now.saturating_sub(time);
    match seconds {
        s if s < 60 => tr("à l'instant").to_string(),
        s if s < 3_600 => trf("il y a {} min", &[&(s / 60)]),
        s if s < 86_400 => trf("il y a {} h", &[&(s / 3_600)]),
        s => trf("il y a {} j", &[&(s / 86_400)]),
    }
}

/// Journal enregistré par le service (vide s'il n'existe pas).
pub fn load(now: u64) -> Journal {
    crate::paths::journal_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| Journal::parse(&text, now))
        .unwrap_or_default()
}

/// Lignes affichées : les `count` derniers événements, du plus récent au
/// plus ancien, « il y a 5 min — Casque connecté ».
pub fn lines(journal: &Journal, now: u64, count: usize) -> Vec<String> {
    journal
        .recent(count)
        .into_iter()
        .map(|e| format!("{} — {}", ago(now, e.time), e.describe()))
        .collect()
}

/// Journal en mémoire, borné.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Journal {
    entries: Vec<Entry>,
}

impl Journal {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Lit le fichier ; lignes invalides ignorées, ordre rétabli, bornes
    /// appliquées.
    pub fn parse(text: &str, now: u64) -> Self {
        let mut entries: Vec<Entry> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(Entry::from_line)
            .collect();
        entries.sort_by_key(|e| e.time);
        let mut journal = Self { entries };
        journal.prune(now);
        journal
    }

    /// Texte du fichier.
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# Journal des événements (OpenHeadset Control) : horodatage Unix;type;détail\n",
        );
        for e in &self.entries {
            out.push_str(&e.to_line());
            out.push('\n');
        }
        out
    }

    fn prune(&mut self, now: u64) {
        let oldest = now.saturating_sub(RETENTION_SECONDS);
        self.entries
            .retain(|e| e.time >= oldest && e.time <= now + 60);
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    /// Dernier mode de bruit noté.
    pub fn last_noise(&self) -> Option<NoiseKind> {
        self.entries.iter().rev().find_map(|e| match e.event {
            Event::Noise(k) => Some(k),
            _ => None,
        })
    }

    /// Note un événement. Un mode de bruit identique au dernier noté n'est
    /// pas répété. Renvoie `true` si une ligne a été ajoutée.
    pub fn record(&mut self, now: u64, event: Event) -> bool {
        if let Event::Noise(k) = event
            && self.last_noise() == Some(k)
            && !matches!(
                self.entries.last(),
                Some(Entry {
                    event: Event::Connected,
                    ..
                })
            )
        {
            return false;
        }
        self.entries.push(Entry { time: now, event });
        self.prune(now);
        true
    }

    /// Efface tout le journal.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Mode de bruit en vigueur à `time` (dernier noté avant, sans perte
    /// de connexion entre les deux).
    pub fn noise_at(&self, time: u64) -> Option<NoiseKind> {
        let mut mode = None;
        for e in self.entries.iter().take_while(|e| e.time <= time) {
            match e.event {
                Event::Noise(k) => mode = Some(k),
                Event::Lost => mode = None,
                _ => {}
            }
        }
        mode
    }

    /// Les `count` derniers événements, du plus récent au plus ancien.
    pub fn recent(&self, count: usize) -> Vec<&Entry> {
        self.entries.iter().rev().take(count).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_privacy() {
        let mut j = Journal::default();
        j.record(100, Event::Connected);
        j.record(110, Event::Noise(NoiseKind::Adaptive));
        j.record(
            120,
            Event::Refused("refus ; casque AA:BB:CC:DD:EE:FF\nsuite".into()),
        );
        j.record(
            130,
            Event::Scene {
                name: "Transport".into(),
                automatic: true,
                complete: true,
            },
        );
        j.record(140, Event::Lost);
        let text = j.to_text();
        assert!(!text.contains("AA:BB"), "{text}");
        let back = Journal::parse(&text, 200);
        assert_eq!(back.entries().len(), 5);
        assert_eq!(back.entries()[1].event, Event::Noise(NoiseKind::Adaptive));
        assert!(
            matches!(&back.entries()[2].event, Event::Refused(r) if !r.contains(';') && !r.contains('\n'))
        );
        assert_eq!(back, Journal::parse(&back.to_text(), 200));
    }

    #[test]
    fn noise_is_not_repeated_and_lost_resets_it() {
        let mut j = Journal::default();
        j.record(10, Event::Connected);
        assert!(j.record(11, Event::Noise(NoiseKind::Custom)));
        assert!(!j.record(12, Event::Noise(NoiseKind::Custom)));
        assert!(j.record(13, Event::Noise(NoiseKind::Off)));
        j.record(20, Event::Lost);
        j.record(30, Event::Connected);
        // Après une reconnexion, le mode est noté de nouveau.
        assert!(j.record(31, Event::Noise(NoiseKind::Off)));
        assert_eq!(j.noise_at(15), Some(NoiseKind::Off));
        assert_eq!(j.noise_at(25), None);
        assert_eq!(j.noise_at(40), Some(NoiseKind::Off));
    }

    #[test]
    fn displayed_lines() {
        let mut j = Journal::default();
        j.record(1_000, Event::Connected);
        j.record(4_000, Event::Lost);
        let l = lines(&j, 4_030, 10);
        assert_eq!(l.len(), 2);
        assert!(l[0].starts_with("à l'instant") && l[0].contains("Connexion perdue"));
        assert!(l[1].starts_with("il y a 50 min"));
        assert_eq!(ago(200_000, 0), "il y a 2 j");
    }

    #[test]
    fn old_and_invalid_lines_are_dropped() {
        let now = RETENTION_SECONDS + 1_000;
        let text = "# entête\n1;connecte;\nxx;perdu;\n2000;inconnu;x\n2000;bruit;turbo\n";
        assert!(Journal::parse(text, now).entries().is_empty());
        let text = format!("{};connecte;\n", now - 10);
        assert_eq!(Journal::parse(&text, now).entries().len(), 1);
    }
}
