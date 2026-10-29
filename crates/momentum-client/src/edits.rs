//! Intentions locales distinctes des lectures du casque. Un retour ancien
//! ne remplace jamais une intention plus récente ; une autre session ou
//! topologie annule ce qui n'a pas encore été envoyé.

use crate::{Command, Snapshot};
use momentum_core::presets::EqTopology;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SliderKey {
    Level,
    PromptVolume,
    Band(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    session: u64,
    writable: bool,
    topology: Option<EqTopology>,
    audio_mode: Option<momentum_core::device::AudioMode>,
}

#[derive(Clone, Debug)]
struct Draft {
    value: i16,
    sent: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SliderEdits {
    epoch: u64,
    context: Option<Context>,
    values: BTreeMap<SliderKey, Draft>,
}

impl SliderEdits {
    /// Retourne true quand le contexte invalide les intentions précédentes.
    pub fn sync(&mut self, snapshot: &Snapshot) -> bool {
        let next = Context {
            session: snapshot.session,
            writable: snapshot.state.is_some() && snapshot.read_only.is_none(),
            topology: snapshot.eq_topology(),
            audio_mode: snapshot.state.and_then(|s| s.audio_mode),
        };
        let changed = self.context.as_ref() != Some(&next);
        if changed {
            self.values.clear();
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.context = Some(next);
        changed
    }

    pub fn value(&self, key: SliderKey) -> Option<i16> {
        self.values.get(&key).map(|d| d.value)
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn edit(&mut self, key: SliderKey, value: i16, read: i16) -> bool {
        if !self.context.as_ref().is_some_and(|c| c.writable) {
            return false;
        }
        if self.value(key) == Some(value) || (self.value(key).is_none() && value == read) {
            return false;
        }
        self.values.insert(key, Draft { value, sent: false });
        true
    }

    pub fn discard(&mut self, key: SliderKey) {
        self.values.remove(&key);
    }

    pub fn discard_bands(&mut self) {
        self.values.retain(|k, _| !matches!(k, SliderKey::Band(_)));
    }

    pub fn notice(&self, snapshot: &Snapshot) -> String {
        use momentum_core::i18n::trf;
        self.values
            .iter()
            .filter_map(|(key, draft)| match key {
                SliderKey::Level => snapshot.state.map(|s| {
                    trf(
                        "ANC/transparence demandé : {} % ; relu : {} %.",
                        &[&draft.value, &s.noise.level],
                    )
                }),
                SliderKey::Band(i) => snapshot
                    .equalizer
                    .as_ref()
                    .and_then(|e| e.gains.get(*i as usize))
                    .map(|read| {
                        trf(
                            "Bande {} demandée : {} dB ; relue : {} dB.",
                            &[
                                &(*i as usize + 1),
                                &momentum_core::presets::format_gains_db(&[draft.value as i8]),
                                &momentum_core::presets::format_gains_db(&[*read]),
                            ],
                        )
                    }),
                SliderKey::PromptVolume => Some(
                    momentum_core::i18n::tr(
                        "Volume des invites demandé : attente de la relecture.",
                    )
                    .to_string(),
                ),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// N'efface pas l'intention affichée tant que les résultats des demandes
    /// propres à cette interface ne sont pas tous arrivés.
    pub fn take_commands(&mut self) -> Vec<Command> {
        let mut commands = Vec::new();
        for (&key, draft) in self.values.iter_mut().filter(|(_, d)| !d.sent) {
            let command = match key {
                SliderKey::Level => u8::try_from(draft.value)
                    .ok()
                    .filter(|v| *v <= 100)
                    .map(Command::SetLevel),
                SliderKey::PromptVolume => {
                    i8::try_from(draft.value).ok().map(Command::SetPromptVolume)
                }
                SliderKey::Band(band) => i8::try_from(draft.value)
                    .ok()
                    .map(|v| Command::SetEqBand(band, v)),
            };
            if let Some(command) = command {
                draft.sent = true;
                commands.push(command);
            }
        }
        commands
    }

    pub fn submitted(&mut self, command: &Command) {
        let pair = match command {
            Command::SetLevel(v) => Some((SliderKey::Level, i16::from(*v))),
            Command::SetPromptVolume(v) => Some((SliderKey::PromptVolume, i16::from(*v))),
            Command::SetEqBand(b, v) => Some((SliderKey::Band(*b), i16::from(*v))),
            _ => None,
        };
        if let Some((key, value)) = pair {
            self.values.insert(key, Draft { value, sent: true });
        }
    }

    pub fn finish_if_idle(&mut self, busy: bool) {
        if !busy {
            self.values.retain(|_, d| !d.sent);
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::Demo;

    #[test]
    fn delayed_readback_never_replaces_the_last_intention() {
        let mut edits = SliderEdits::default();
        let mut snapshot = Demo::new().snapshot();
        edits.sync(&snapshot);
        assert!(edits.edit(SliderKey::Level, 10, 0));
        assert_eq!(edits.take_commands(), [Command::SetLevel(10)]);
        assert!(edits.edit(SliderKey::Level, 30, 0));
        assert!(edits.edit(SliderKey::Level, 70, 0));
        snapshot.state.as_mut().unwrap().noise.level = 10;
        assert!(!edits.sync(&snapshot));
        edits.finish_if_idle(false);
        assert_eq!(edits.value(SliderKey::Level), Some(70));
        assert_eq!(edits.take_commands(), [Command::SetLevel(70)]);
        edits.finish_if_idle(true);
        assert_eq!(edits.value(SliderKey::Level), Some(70));
        edits.finish_if_idle(false);
        assert_eq!(edits.value(SliderKey::Level), None);
    }

    #[test]
    fn loss_read_only_and_session_change_discard_unsent_values() {
        for kind in 0..3 {
            let mut snapshot = Demo::new().snapshot();
            let mut edits = SliderEdits::default();
            edits.sync(&snapshot);
            edits.edit(SliderKey::Band(0), 30, 0);
            match kind {
                0 => snapshot.state = None,
                1 => snapshot.read_only = Some("non qualifié".into()),
                _ => snapshot.session += 1,
            }
            assert!(edits.sync(&snapshot));
            assert!(edits.take_commands().is_empty());
        }
    }

    #[test]
    fn replacing_a_mode_cancels_only_its_level() {
        let mut edits = SliderEdits::default();
        edits.sync(&Demo::new().snapshot());
        edits.edit(SliderKey::Level, 70, 0);
        edits.edit(SliderKey::Band(2), 20, 0);
        edits.discard(SliderKey::Level);
        assert_eq!(edits.take_commands(), [Command::SetEqBand(2, 20)]);
    }

    #[test]
    fn gain_limit_change_and_independent_bands_are_not_confused() {
        let mut edits = SliderEdits::default();
        let mut snapshot = Demo::new().snapshot();
        edits.sync(&snapshot);
        edits.edit(SliderKey::Band(0), 15, 0);
        edits.edit(SliderKey::Band(1), 25, 0);
        assert_eq!(edits.take_commands().len(), 2);
        snapshot.equalizer.as_mut().unwrap().max_gain = 30;
        assert!(edits.sync(&snapshot));
        assert!(edits.take_commands().is_empty());
    }

    #[test]
    fn an_early_or_foreign_result_cannot_drop_a_later_intention() {
        use crate::{Outcome, tracker::Tracker};
        let mut edits = SliderEdits::default();
        edits.sync(&Demo::new().snapshot());
        let mut tracker = Tracker::default();
        edits.edit(SliderKey::Level, 10, 0);
        edits.take_commands();
        tracker.sent();
        edits.edit(SliderKey::Level, 70, 0);
        tracker.done(999, &Outcome::Applied);
        edits.finish_if_idle(tracker.busy());
        assert_eq!(edits.value(SliderKey::Level), Some(70));
        tracker.done(1, &Outcome::Applied);
        tracker.submitted(1);
        edits.finish_if_idle(tracker.busy());
        assert_eq!(edits.take_commands(), [Command::SetLevel(70)]);
        tracker.sent();
        tracker.submitted(2);
        tracker.done(2, &Outcome::Refused("refus synthétique".into()));
        edits.finish_if_idle(tracker.busy());
        assert_eq!(edits.value(SliderKey::Level), None);
    }

    #[test]
    fn a_new_connection_invalidates_a_drag_even_with_identical_bands() {
        let mut snapshot = Demo::new().snapshot();
        let mut edits = SliderEdits::default();
        edits.sync(&snapshot);
        let drag_epoch = edits.epoch();
        snapshot.session += 1;
        edits.sync(&snapshot);
        assert_ne!(drag_epoch, edits.epoch());
        assert!(edits.take_commands().is_empty());
    }
}
