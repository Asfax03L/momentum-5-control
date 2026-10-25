//! Historique local de la batterie, en Rust pur (testable sans casque).
//!
//! Seules les lectures déjà faites par l'icône sont enregistrées (batterie
//! lue ou notifiée) : aucune commande supplémentaire n'est envoyée au casque.
//! Fichier `batterie.csv` : une ligne `horodatage_unix;pourcentage`, ou
//! `horodatage_unix;-` pour une interruption (casque perdu, icône quittée).
//! Une absence de mesure de plus de [`GAP_SECONDS`] est aussi traitée comme
//! une période inconnue (veille du PC, par exemple) : rien n'est interpolé.
//!
//! Le projet ne lit ni l'état de charge, ni la santé, ni le nombre de cycles
//! de la batterie : une hausse du pourcentage est seulement montrée telle
//! quelle.

/// Durée conservée : 30 jours.
pub const RETENTION_SECONDS: u64 = 30 * 24 * 3600;
/// Nombre maximal de lignes gardées (garde-fou de taille du fichier).
pub const MAX_ENTRIES: usize = 20_000;
/// Au-delà, deux mesures ne sont pas reliées.
pub const GAP_SECONDS: u64 = 30 * 60;
/// Une mesure inchangée est quand même notée toutes les 15 minutes.
pub const HEARTBEAT_SECONDS: u64 = 15 * 60;

/// Une ligne de l'historique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Pourcentage lu à cet instant.
    Level { time: u64, percent: u8 },
    /// Interruption : la suite n'est pas reliée à ce qui précède.
    Gap { time: u64 },
}

impl Entry {
    pub fn time(&self) -> u64 {
        match *self {
            Entry::Level { time, .. } | Entry::Gap { time } => time,
        }
    }
}

/// Historique en mémoire, borné.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatteryLog {
    entries: Vec<Entry>,
}

impl BatteryLog {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Lit le fichier ; les lignes invalides sont ignorées, l'ordre
    /// chronologique est rétabli et les bornes appliquées.
    pub fn parse(text: &str, now: u64) -> Self {
        let mut entries: Vec<Entry> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|line| {
                let (time, value) = line.trim().split_once(';')?;
                let time: u64 = time.parse().ok()?;
                match value {
                    "-" => Some(Entry::Gap { time }),
                    v => match v.parse::<u8>() {
                        Ok(percent) if percent <= 100 => Some(Entry::Level { time, percent }),
                        _ => None,
                    },
                }
            })
            .collect();
        entries.sort_by_key(Entry::time);
        let mut log = Self { entries };
        log.prune(now);
        log
    }

    /// Texte du fichier.
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# Historique de la batterie (momentum-ctl-win) : horodatage Unix;pourcentage, « - » = interruption\n",
        );
        for entry in &self.entries {
            match *entry {
                Entry::Level { time, percent } => out.push_str(&format!("{time};{percent}\n")),
                Entry::Gap { time } => out.push_str(&format!("{time};-\n")),
            }
        }
        out
    }

    fn prune(&mut self, now: u64) {
        let oldest = now.saturating_sub(RETENTION_SECONDS);
        self.entries
            .retain(|e| e.time() >= oldest && e.time() <= now + 60);
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    fn last_level(&self) -> Option<(u64, u8)> {
        match self.entries.last() {
            Some(Entry::Level { time, percent }) => Some((*time, *percent)),
            _ => None,
        }
    }

    /// Note une lecture. Renvoie `true` si une ligne a été ajoutée (valeur
    /// changée, première mesure après une interruption, ou rappel périodique).
    pub fn record(&mut self, now: u64, percent: u8) -> bool {
        if percent > 100 {
            return false;
        }
        let keep = match self.last_level() {
            Some((time, last)) => {
                last != percent || now.saturating_sub(time) >= HEARTBEAT_SECONDS || now < time
            }
            None => true,
        };
        if keep {
            self.entries.push(Entry::Level { time: now, percent });
            self.prune(now);
        }
        keep
    }

    /// Note une interruption (casque perdu, icône quittée). Sans effet si la
    /// dernière ligne en est déjà une.
    pub fn interrupt(&mut self, now: u64) -> bool {
        if matches!(self.entries.last(), Some(Entry::Gap { .. }) | None) {
            return false;
        }
        self.entries.push(Entry::Gap { time: now });
        true
    }

    /// Efface tout l'historique.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Segments continus de mesures (ni interruption, ni trou de plus de
    /// [`GAP_SECONDS`]) compris entre `since` et maintenant.
    pub fn segments(&self, since: u64) -> Vec<Vec<(u64, u8)>> {
        let mut segments: Vec<Vec<(u64, u8)>> = Vec::new();
        let mut current: Vec<(u64, u8)> = Vec::new();
        for entry in &self.entries {
            match *entry {
                Entry::Gap { .. } => {
                    if !current.is_empty() {
                        segments.push(std::mem::take(&mut current));
                    }
                }
                Entry::Level { time, percent } => {
                    if let Some(&(last, _)) = current.last()
                        && time.saturating_sub(last) > GAP_SECONDS
                    {
                        segments.push(std::mem::take(&mut current));
                    }
                    current.push((time, percent));
                }
            }
        }
        if !current.is_empty() {
            segments.push(current);
        }
        segments
            .into_iter()
            .map(|s| {
                s.into_iter()
                    .filter(|&(t, _)| t >= since)
                    .collect::<Vec<_>>()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Estimation du temps restant, seulement si la dernière décharge
    /// continue dure au moins une heure, a fait perdre au moins 5 points et
    /// se termine il y a moins de [`GAP_SECONDS`]. Renvoie (heures restantes,
    /// heures observées). Une hausse du pourcentage (charge) remet
    /// l'observation à zéro.
    pub fn estimate(&self, now: u64) -> Option<(f32, f32)> {
        let segments = self.segments(0);
        let last = segments.last()?;
        let &(end_time, end_level) = last.last()?;
        if now.saturating_sub(end_time) > GAP_SECONDS {
            return None;
        }
        // Début de la décharge en cours : après la dernière hausse.
        let mut start = last.len() - 1;
        while start > 0 && last[start - 1].1 >= last[start].1 {
            start -= 1;
        }
        let (start_time, start_level) = last[start];
        let hours = end_time.saturating_sub(start_time) as f32 / 3600.0;
        let drop = f32::from(start_level.saturating_sub(end_level));
        if hours < 1.0 || drop < 5.0 {
            return None;
        }
        Some((f32::from(end_level) / (drop / hours), hours))
    }
}

/// Courbe en lignes brisées dans un repère 0..1 × 0..1 (x = temps sur la
/// période, y = 1 − pourcentage/100), une par segment continu, réduite à
/// `max_points` points au plus en tout (le dernier point de chaque segment
/// est gardé). Pour les interfaces qui dessinent des segments de droite.
pub fn chart_polylines(
    log: &BatteryLog,
    now: u64,
    span_seconds: u64,
    max_points: usize,
) -> Vec<Vec<(f64, f64)>> {
    let since = now.saturating_sub(span_seconds);
    let span = span_seconds.max(1) as f64;
    let segments = log.segments(since);
    let total: usize = segments.iter().map(Vec::len).sum();
    let step = total.div_ceil(max_points.max(1)).max(1);
    segments
        .iter()
        .map(|segment| {
            let point = |&(time, percent): &(u64, u8)| {
                (
                    (time.saturating_sub(since) as f64 / span).min(1.0),
                    1.0 - f64::from(percent) / 100.0,
                )
            };
            let mut line: Vec<(f64, f64)> = segment.iter().step_by(step).map(point).collect();
            if let Some(last) = segment.last()
                && (segment.len() - 1) % step != 0
            {
                line.push(point(last));
            }
            line
        })
        .collect()
}

/// Tracé SVG (commandes `M`/`L`) dans un repère de 1000 × 100 :
/// x = temps sur la période, y = 100 − pourcentage. Chaque segment continu
/// commence par un `M` : les périodes inconnues restent vides.
pub fn chart_path(log: &BatteryLog, now: u64, span_seconds: u64) -> String {
    let since = now.saturating_sub(span_seconds);
    let span = span_seconds.max(1) as f64;
    let mut out = String::new();
    for segment in log.segments(since) {
        for (i, &(time, percent)) in segment.iter().enumerate() {
            let x = (time.saturating_sub(since) as f64 / span * 1000.0).min(1000.0);
            let y = 100.0 - f64::from(percent);
            let command = if i == 0 { 'M' } else { 'L' };
            out.push_str(&format!("{command} {x:.1} {y:.1} "));
        }
        // Un point isolé reste visible : petit trait horizontal vers la
        // gauche (une mesure toute récente est au bord droit).
        if let [(time, percent)] = segment[..] {
            let x = (time.saturating_sub(since) as f64 / span * 1000.0).min(1000.0);
            out.push_str(&format!(
                "L {:.1} {:.1} ",
                (x - 8.0).max(0.0),
                100.0 - f64::from(percent)
            ));
        }
    }
    out.trim_end().to_string()
}

/// Décharge continue mesurée : le pourcentage ne remonte pas, sans
/// interruption, pendant au moins une heure et cinq points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discharge {
    pub start: u64,
    pub end: u64,
    pub from: u8,
    pub to: u8,
}

/// Autonomie mesurée sur une ou plusieurs décharges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Autonomy {
    pub discharges: usize,
    pub hours: f32,
    pub points: f32,
}

impl Autonomy {
    /// Points de batterie perdus par heure.
    pub fn rate(&self) -> f32 {
        if self.hours > 0.0 {
            self.points / self.hours
        } else {
            0.0
        }
    }

    /// Durée d'une charge complète (100 points) à ce rythme.
    pub fn full_charge_hours(&self) -> Option<f32> {
        let rate = self.rate();
        (rate > 0.0).then(|| 100.0 / rate)
    }
}

impl BatteryLog {
    /// Décharges continues (au moins une heure et cinq points).
    pub fn discharges(&self) -> Vec<Discharge> {
        let mut out = Vec::new();
        for segment in self.segments(0) {
            let mut start = 0;
            for i in 1..=segment.len() {
                // Fin d'une descente : hausse, ou fin du segment.
                if i == segment.len() || segment[i].1 > segment[i - 1].1 {
                    let (t0, l0) = segment[start];
                    let (t1, l1) = segment[i - 1];
                    if t1.saturating_sub(t0) >= 3600 && l0.saturating_sub(l1) >= 5 {
                        out.push(Discharge {
                            start: t0,
                            end: t1,
                            from: l0,
                            to: l1,
                        });
                    }
                    start = i;
                }
            }
        }
        out
    }

    /// Points mesurés à l'intérieur d'une décharge, dans l'ordre.
    fn points_in(&self, d: &Discharge) -> Vec<(u64, u8)> {
        self.entries
            .iter()
            .filter_map(|e| match *e {
                Entry::Level { time, percent } if time >= d.start && time <= d.end => {
                    Some((time, percent))
                }
                _ => None,
            })
            .collect()
    }
}

/// Autonomie mesurée sur toutes les décharges ; `None` sans décharge.
pub fn autonomy(log: &BatteryLog) -> Option<Autonomy> {
    let discharges = log.discharges();
    (!discharges.is_empty()).then(|| Autonomy {
        discharges: discharges.len(),
        hours: discharges
            .iter()
            .map(|d| d.end.saturating_sub(d.start) as f32 / 3600.0)
            .sum(),
        points: discharges
            .iter()
            .map(|d| f32::from(d.from.saturating_sub(d.to)))
            .sum(),
    })
}

/// Consommation selon le mode de réduction de bruit noté au journal : chaque
/// intervalle entre deux mesures d'une décharge compte pour le mode en
/// vigueur à son début (intervalle ignoré si le mode est inconnu). Seuls les
/// modes observés au moins une heure sont rendus.
pub fn consumption_by_mode(
    log: &BatteryLog,
    journal: &crate::journal::Journal,
) -> Vec<(crate::journal::NoiseKind, Autonomy)> {
    let mut totals: std::collections::BTreeMap<crate::journal::NoiseKind, Autonomy> =
        std::collections::BTreeMap::new();
    for d in log.discharges() {
        let points = log.points_in(&d);
        for pair in points.windows(2) {
            let ((t0, l0), (t1, l1)) = (pair[0], pair[1]);
            let Some(mode) = journal.noise_at(t0) else {
                continue;
            };
            let a = totals.entry(mode).or_default();
            a.hours += t1.saturating_sub(t0) as f32 / 3600.0;
            a.points += f32::from(l0.saturating_sub(l1));
        }
    }
    totals.into_iter().filter(|(_, a)| a.hours >= 1.0).collect()
}

/// Nombre à une décimale, avec la virgule en français.
fn decimal(value: f32) -> String {
    let text = format!("{value:.1}");
    if crate::i18n::language() == crate::i18n::Lang::Fr {
        text.replace('.', ",")
    } else {
        text
    }
}

/// Textes de l'autonomie mesurée, prêts à afficher : bilan, puis une ligne
/// par mode de bruit observé.
pub fn autonomy_lines(log: &BatteryLog, journal: &crate::journal::Journal) -> Vec<String> {
    use crate::i18n::{tr, trf};
    let Some(total) = autonomy(log) else {
        return vec![
            tr("Pas encore assez de mesures : il faut au moins une heure de décharge continue, casque connecté à ce PC.")
                .to_string(),
        ];
    };
    let full = total.full_charge_hours().map_or("?".to_string(), decimal);
    let mut lines = vec![trf(
        "Mesuré sur {} h ({} décharges) : {} % par heure, soit environ {} h pour une charge complète.",
        &[
            &decimal(total.hours),
            &total.discharges,
            &decimal(total.rate()),
            &full,
        ],
    )];
    for (mode, a) in consumption_by_mode(log, journal) {
        lines.push(trf(
            "{} : {} % par heure (environ {} h), sur {} h",
            &[
                &mode.label(),
                &decimal(a.rate()),
                &a.full_charge_hours().map_or("?".to_string(), decimal),
                &decimal(a.hours),
            ],
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_autonomy_and_consumption_by_mode() {
        use crate::journal::{Event, Journal, NoiseKind};
        let mut log = BatteryLog::default();
        // Décharge de 90 à 80 % en deux heures (mesure toutes les 12 min),
        // puis recharge, puis une descente trop courte (ignorée).
        for i in 0..=10u64 {
            log.record(i * 720, 90 - i as u8);
        }
        log.record(7_300, 95);
        log.record(7_900, 94);
        let discharges = log.discharges();
        assert_eq!(discharges.len(), 1);
        let total = autonomy(&log).unwrap();
        assert!((total.rate() - 5.0).abs() < 0.01, "{total:?}");
        assert!((total.full_charge_hours().unwrap() - 20.0).abs() < 0.1);

        // Première heure en adaptatif, seconde en mode désactivé.
        let mut journal = Journal::default();
        journal.record(0, Event::Connected);
        journal.record(0, Event::Noise(NoiseKind::Adaptive));
        journal.record(3_600, Event::Noise(NoiseKind::Off));
        let by_mode = consumption_by_mode(&log, &journal);
        assert_eq!(by_mode.len(), 2);
        assert!(by_mode.iter().all(|(_, a)| (a.hours - 1.0).abs() < 0.01));
        let lines = autonomy_lines(&log, &journal);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("20,0"), "{lines:?}");
    }

    #[test]
    fn no_autonomy_without_a_long_discharge() {
        let mut log = BatteryLog::default();
        log.record(0, 80);
        log.record(1_800, 70);
        assert!(autonomy(&log).is_none());
        assert_eq!(
            autonomy_lines(&log, &crate::journal::Journal::default()).len(),
            1
        );
    }

    #[test]
    fn polylines_are_normalized_and_reduced() {
        let now = 10 * 3600;
        let mut log = BatteryLog::default();
        // Une mesure par minute pendant 2 h : 120 points, de 100 % à 41 %.
        for i in 0..120u64 {
            log.record(now - 2 * 3600 + i * 60, 100 - (i / 2) as u8);
        }
        let lines = chart_polylines(&log, now, 4 * 3600, 30);
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert!(line.len() <= 31, "{} points", line.len());
        assert!(
            line.iter()
                .all(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y))
        );
        // Début à mi-période, 100 % en haut ; fin près du bord droit.
        assert!((line[0].0 - 0.5).abs() < 1e-9 && line[0].1.abs() < 1e-9);
        assert!(line.last().unwrap().0 > 0.99);
    }

    const H: u64 = 3600;

    #[test]
    fn file_round_trip_ignores_bad_lines_and_old_entries() {
        let now = 40 * 24 * H;
        let text = format!(
            "# commentaire\n{};50\nn'importe quoi\n{};101\n{};80\n{};-\n{};79\n",
            now - 35 * 24 * H,
            now - 2 * H,
            now - 3 * H,
            now - H,
            now - H / 2
        );
        let log = BatteryLog::parse(&text, now);
        assert_eq!(
            log.entries(),
            [
                Entry::Level {
                    time: now - 3 * H,
                    percent: 80
                },
                Entry::Gap { time: now - H },
                Entry::Level {
                    time: now - H / 2,
                    percent: 79
                },
            ]
        );
        assert_eq!(BatteryLog::parse(&log.to_text(), now), log);
    }

    #[test]
    fn records_changes_and_heartbeats_only() {
        let mut log = BatteryLog::default();
        assert!(log.record(1000, 80));
        assert!(!log.record(1060, 80));
        assert!(log.record(1120, 79));
        assert!(log.record(1120 + HEARTBEAT_SECONDS, 79));
        assert!(!log.record(1200, 101));
        assert!(log.interrupt(5000));
        assert!(!log.interrupt(5001));
        // Après une interruption, la première mesure est toujours notée.
        assert!(log.record(6000, 79));
        assert_eq!(log.entries().len(), 5);
    }

    #[test]
    fn gaps_and_long_silences_split_the_curve() {
        let mut log = BatteryLog::default();
        log.record(0, 90);
        log.record(600, 89);
        log.interrupt(700);
        log.record(800, 88);
        // Veille du PC : 2 h sans mesure.
        log.record(800 + 2 * H, 80);
        let segments = log.segments(0);
        assert_eq!(segments.len(), 3);
        let path = chart_path(&log, 800 + 2 * H, 800 + 2 * H);
        assert_eq!(path.matches('M').count(), 3, "{path}");
    }

    #[test]
    fn estimate_needs_an_hour_and_five_points_of_steady_discharge() {
        let mut log = BatteryLog::default();
        // Charge jusqu'à 100 %, puis décharge de 10 points en 2 h.
        log.record(0, 95);
        log.record(600, 100);
        for i in 1..=10u64 {
            log.record(600 + i * 720, (100 - i) as u8);
        }
        let now = 600 + 10 * 720;
        let (remaining, observed) = log.estimate(now).unwrap();
        assert!((observed - 2.0).abs() < 0.01, "{observed}");
        // 5 points par heure, 90 % restants : 18 h.
        assert!((remaining - 18.0).abs() < 0.1, "{remaining}");
        // Plus de mesure depuis longtemps : pas d'estimation.
        assert!(log.estimate(now + 2 * H).is_none());
        // Moins d'une heure de décharge : pas d'estimation.
        let mut short = BatteryLog::default();
        short.record(0, 90);
        short.record(1800, 80);
        assert!(short.estimate(1800).is_none());
    }
}
