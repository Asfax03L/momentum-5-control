//! Parcours EQ local, commun aux interfaces. Aucun son généré, aucun profil
//! auditif : trois comparaisons bornées avec les commandes EQ existantes.
//! La progression attend le résultat numéroté ET la courbe relue.

use crate::{Command, Outcome, Snapshot};
use momentum_core::{device::AudioMode, i18n::tr, presets::EqTopology};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Start,
    Less,
    Reference,
    More,
    Next,
    Initial,
    Result,
    Restore,
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    session: u64,
    headset_id: Option<String>,
    topology: EqTopology,
    bass_boost: bool,
}

impl Context {
    fn read(s: &Snapshot) -> Option<Self> {
        (s.session != 0 && s.state?.audio_mode == Some(AudioMode::Equalizer)).then_some(Self {
            session: s.session,
            headset_id: s.headset_id.clone(),
            topology: s.eq_topology()?,
            bass_boost: s.equalizer.as_ref()?.bass_boost,
        })
    }
}

#[derive(Clone, Debug)]
struct Pending {
    target: Vec<i8>,
    choice: Action,
    restore: bool,
    id: Option<u64>,
    early: Vec<(u64, Outcome)>,
    outcome: Option<Outcome>,
}

#[derive(Clone, Debug)]
struct Session {
    context: Context,
    initial: Vec<i8>,
    chosen: Vec<i8>,
    expected: Vec<i8>,
    step: usize,
    listening: Action,
    pending: Option<Pending>,
    problem: Option<String>,
    invalid: bool,
}

#[derive(Clone, Debug, Default)]
pub struct GuidedEq {
    session: Option<Session>,
    notice: String,
}

#[derive(serde::Serialize)]
pub struct GuidedView {
    pub active: bool,
    pub can_start: bool,
    pub can_listen: bool,
    pub can_next: bool,
    pub can_restore: bool,
    pub can_dismiss: bool,
    pub review: bool,
    pub can_save: bool,
    pub title: String,
    pub status: String,
    pub initial: String,
    pub current: String,
}

impl GuidedEq {
    pub fn active(&self) -> bool {
        self.session.is_some()
    }

    pub fn view(&self, snapshot: &Snapshot, busy: bool, drafts: bool) -> GuidedView {
        let mut v = GuidedView {
            active: self.active(),
            can_start: !self.active() && !busy && !drafts && Self::eligible(snapshot).is_ok(),
            can_listen: false,
            can_next: false,
            can_restore: false,
            can_dismiss: false,
            review: false,
            can_save: false,
            title: tr("Assistant EQ guidé").into(),
            status: self.notice.clone(),
            initial: String::new(),
            current: snapshot.equalizer.as_ref().map_or_else(String::new, |eq| {
                momentum_core::presets::format_gains_db(&eq.gains)
            }),
        };
        if let Some(s) = &self.session {
            let ready = !busy && s.pending.is_none() && !s.invalid;
            v.review = s.step == 3;
            v.can_listen = ready && s.problem.is_none();
            v.can_next = v.can_listen && !v.review;
            v.can_restore = ready;
            v.can_dismiss = (s.invalid || s.problem.is_some()) && !busy && s.pending.is_none();
            v.can_save = v.can_listen && v.review && s.expected == s.chosen;
            v.title = tr(match s.step {
                0 => "Étape 1/3 — graves (90 et 325 Hz)",
                1 => "Étape 2/3 — médiums (1,5 à 4,5 kHz)",
                2 => "Étape 3/3 — aigus (8 à 16 kHz)",
                _ => "Comparaison finale — initiale / résultat",
            })
            .into();
            v.status = if let Some(problem) = &s.problem {
                problem.clone()
            } else if s.pending.is_some() {
                tr("Attente du résultat et de la courbe relue…").into()
            } else {
                momentum_core::i18n::trf(
                    "Courbe relue : {}.",
                    &[&tr(match s.listening {
                        Action::Less => "Moins (−1 dB)",
                        Action::More => "Plus (+1 dB)",
                        Action::Initial => "Courbe initiale",
                        Action::Result => "Résultat",
                        _ => "Référence de l'étape",
                    })],
                )
            };
            v.initial = momentum_core::presets::format_gains_db(&s.initial);
        } else if v.status.is_empty() {
            v.status = if Self::eligible(snapshot).is_err() {
                tr("Connectez un M5 autorisé en écriture, puis choisissez Égaliseur graphique.")
                    .into()
            } else if busy || drafts {
                tr("Attendez la fin des réglages en cours avant de commencer.").into()
            } else {
                tr("Trois étapes : graves, médiums, aigus. Aucun son n'est lancé automatiquement.")
                    .into()
            };
        }
        v
    }

    fn eligible(snapshot: &Snapshot) -> Result<Context, &'static str> {
        let context = Context::read(snapshot).ok_or(tr(
            "Connectez un M5 autorisé en écriture, puis choisissez Égaliseur graphique.",
        ))?;
        context.topology.check_gains(
            &snapshot
                .equalizer
                .as_ref()
                .ok_or(tr("Égaliseur indisponible"))?
                .gains,
        )?;
        Ok(context)
    }

    /// Met à jour le contexte sans jamais produire de commande.
    pub fn sync(&mut self, snapshot: &Snapshot) {
        let Some(s) = &mut self.session else { return };
        if s.invalid {
            return;
        }
        if Context::read(snapshot).as_ref() != Some(&s.context) {
            s.invalid = true;
            s.pending = None;
            s.problem = Some(tr("Assistant interrompu : connexion, mode ou égaliseur changé. Aucune restauration automatique. La copie initiale reste sur ce PC.").into());
            return;
        }
        let Some(eq) = snapshot.equalizer.as_ref() else {
            return;
        };
        let current = &eq.gains;
        let mut restored = false;
        if let Some(p) = &mut s.pending {
            if let Some(outcome) = p.outcome.take() {
                if matches!(outcome, Outcome::Applied | Outcome::AlreadySatisfied)
                    && current == &p.target
                {
                    s.expected = p.target.clone();
                    s.listening = p.choice;
                    restored = p.restore;
                    s.pending = None;
                } else {
                    s.pending = None;
                    s.problem = Some(tr("Essai non confirmé. Vérifiez la connexion, puis restaurez la courbe initiale avant de recommencer.").into());
                }
            }
        } else if s.problem.is_none() && current != &s.expected {
            s.problem = Some(tr("Une autre modification de l'égaliseur a interrompu l'assistant. Restaurez la courbe initiale ou fermez le parcours.").into());
        }
        if restored {
            self.session = None;
            self.notice = tr("Courbe initiale restaurée et relue. Assistant terminé.").into();
        }
    }

    pub fn submitted(&mut self, id: u64) {
        if let Some(p) = self.session.as_mut().and_then(|s| s.pending.as_mut())
            && p.id.is_none()
        {
            p.id = Some(id);
            if let Some(i) = p.early.iter().position(|(n, _)| *n == id) {
                p.outcome = Some(p.early.remove(i).1);
            }
            p.early.clear();
        }
    }

    pub fn done(&mut self, id: u64, outcome: &Outcome) {
        if let Some(p) = self.session.as_mut().and_then(|s| s.pending.as_mut()) {
            if p.id == Some(id) {
                p.outcome = Some(outcome.clone());
            } else if p.id.is_none() {
                p.early.push((id, outcome.clone()));
                if p.early.len() > 32 {
                    p.early.remove(0);
                }
            }
        }
    }

    pub fn rejected(&mut self) {
        if let Some(s) = &mut self.session {
            s.pending = None;
            s.problem = Some(tr("Essai non confirmé. Vérifiez la connexion, puis restaurez la courbe initiale avant de recommencer.").into());
        }
    }

    pub fn action(
        &mut self,
        action: Action,
        snapshot: &Snapshot,
        busy: bool,
        drafts: bool,
    ) -> Result<Option<Command>, &'static str> {
        self.sync(snapshot);
        if busy || drafts {
            return Err(tr(
                "Attendez la fin des réglages en cours avant de commencer.",
            ));
        }
        if action == Action::Start {
            if self.active() {
                return Err(tr("Terminez ou restaurez l'assistant EQ en cours."));
            }
            let context = Self::eligible(snapshot)?;
            let initial = snapshot
                .equalizer
                .as_ref()
                .ok_or(tr("Égaliseur indisponible"))?
                .gains
                .clone();
            self.session = Some(Session {
                context,
                chosen: initial.clone(),
                expected: initial.clone(),
                initial,
                step: 0,
                listening: Action::Reference,
                pending: None,
                problem: None,
                invalid: false,
            });
            self.notice.clear();
            return Ok(None);
        }
        let s = self
            .session
            .as_mut()
            .ok_or(tr("Assistant EQ non démarré."))?;
        if action == Action::Dismiss && (s.invalid || s.problem.is_some()) {
            self.session = None;
            self.notice = tr("Assistant fermé sans restauration. La dernière courbe appliquée peut rester active ; la copie initiale est conservée sur ce PC.").into();
            return Ok(None);
        }
        if s.invalid || s.pending.is_some() {
            return Err(tr("Terminez ou restaurez l'assistant EQ en cours."));
        }
        if s.problem.is_some() && action != Action::Restore {
            return Err(tr("Terminez ou restaurez l'assistant EQ en cours."));
        }
        if action == Action::Next && s.step < 3 {
            s.chosen = s.expected.clone();
            s.step += 1;
            s.listening = if s.step == 3 {
                Action::Result
            } else {
                Action::Reference
            };
            return Ok(None);
        }
        let target = match action {
            Action::Restore | Action::Initial => s.initial.clone(),
            Action::Result if s.step == 3 => s.chosen.clone(),
            Action::Reference if s.step < 3 => s.chosen.clone(),
            Action::Less | Action::More if s.step < 3 => {
                let mut gains = s.chosen.clone();
                let range = match s.step {
                    0 => 0..2,
                    1 => 2..5,
                    _ => 5..8,
                };
                let delta: i16 = if action == Action::Less { -10 } else { 10 };
                for gain in &mut gains[range] {
                    *gain = (i16::from(*gain) + delta).clamp(
                        i16::from(s.context.topology.min_gain),
                        i16::from(s.context.topology.max_gain),
                    ) as i8;
                }
                gains
            }
            _ => return Err(tr("Action indisponible à cette étape de l'assistant.")),
        };
        s.context.topology.check_gains(&target)?;
        if target == s.expected && s.problem.is_none() {
            s.listening = action;
            if action == Action::Restore {
                self.session = None;
                self.notice = tr("Courbe initiale déjà relue. Assistant terminé.").into();
            }
            return Ok(None);
        }
        s.pending = Some(Pending {
            target: target.clone(),
            choice: action,
            restore: action == Action::Restore,
            id: None,
            early: Vec::new(),
            outcome: None,
        });
        s.problem = None;
        Ok(Some(Command::SetEqGains(target)))
    }

    /// Rien n'est enregistré tant que le résultat final n'est pas relu.
    pub fn curve_to_save(
        &self,
        snapshot: &Snapshot,
        busy: bool,
    ) -> Result<(Vec<i8>, EqTopology), &'static str> {
        let s = self
            .session
            .as_ref()
            .ok_or(tr("Assistant EQ non démarré."))?;
        if busy
            || s.invalid
            || s.problem.is_some()
            || s.pending.is_some()
            || s.step != 3
            || Context::read(snapshot).as_ref() != Some(&s.context)
            || snapshot.equalizer.as_ref().map(|e| &e.gains) != Some(&s.chosen)
        {
            return Err(tr(
                "Écoutez et faites relire le résultat final avant de l'enregistrer.",
            ));
        }
        Ok((s.chosen.clone(), s.context.topology.clone()))
    }

    pub fn saved(&mut self) {
        self.session = None;
        self.notice = tr("Résultat enregistré sur ce PC. Assistant terminé.").into();
    }
}
