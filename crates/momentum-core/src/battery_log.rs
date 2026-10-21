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

/// Tracé pour Slint (`Path.commands`) dans un repère de 1000 × 100 :
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

#[cfg(test)]
mod tests {
    use super::*;

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
