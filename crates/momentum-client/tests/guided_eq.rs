use momentum_client::{
    Command, Outcome, Snapshot,
    demo::Demo,
    sound_check::{Action, GuidedEq},
};

fn start() -> (GuidedEq, Snapshot) {
    let mut s = Demo::new().snapshot();
    s.equalizer.as_mut().unwrap().gains = vec![0; 8];
    let mut g = GuidedEq::default();
    assert_eq!(g.action(Action::Start, &s, false, false).unwrap(), None);
    (g, s)
}

fn apply(g: &mut GuidedEq, s: &mut Snapshot, action: Action, id: u64) {
    if let Some(Command::SetEqGains(gains)) = g.action(action, s, false, false).unwrap() {
        g.submitted(id);
        s.equalizer.as_mut().unwrap().gains = gains;
        g.sync(s);
        assert!(!g.view(s, false, false).can_next);
        g.done(id, &Outcome::Applied);
        g.sync(s);
    }
}

#[test]
fn three_groups_then_comparison_save_and_restore_are_exact() {
    let (mut g, mut s) = start();
    for (step, expected) in [
        vec![10, 10, 0, 0, 0, 0, 0, 0],
        vec![10, 10, -10, -10, -10, 0, 0, 0],
        vec![10, 10, -10, -10, -10, 10, 10, 10],
    ]
    .into_iter()
    .enumerate()
    {
        apply(
            &mut g,
            &mut s,
            if step == 1 {
                Action::Less
            } else {
                Action::More
            },
            step as u64 + 1,
        );
        assert_eq!(s.equalizer.as_ref().unwrap().gains, expected);
        g.action(Action::Next, &s, false, false).unwrap();
    }
    assert!(g.view(&s, false, false).review);
    assert!(g.curve_to_save(&s, false).is_ok());
    apply(&mut g, &mut s, Action::Initial, 4);
    assert_eq!(s.equalizer.as_ref().unwrap().gains, vec![0; 8]);
    assert!(g.curve_to_save(&s, false).is_err());
    apply(&mut g, &mut s, Action::Result, 5);
    assert!(g.curve_to_save(&s, false).is_ok());
    apply(&mut g, &mut s, Action::Restore, 6);
    assert!(!g.active());
    assert_eq!(s.equalizer.as_ref().unwrap().gains, vec![0; 8]);
}

#[test]
fn previews_do_not_accumulate_and_respect_read_limits() {
    let (mut g, mut s) = start();
    apply(&mut g, &mut s, Action::More, 1);
    assert!(g.action(Action::More, &s, false, false).unwrap().is_none());
    apply(&mut g, &mut s, Action::Less, 2);
    assert_eq!(s.equalizer.as_ref().unwrap().gains[..2], [-10, -10]);
    let mut s = Demo::new().snapshot();
    s.equalizer.as_mut().unwrap().gains = vec![60; 8];
    let mut g = GuidedEq::default();
    g.action(Action::Start, &s, false, false).unwrap();
    assert!(g.action(Action::More, &s, false, false).unwrap().is_none());
    apply(&mut g, &mut s, Action::Less, 3);
    assert_eq!(s.equalizer.as_ref().unwrap().gains[..2], [50, 50]);
}

#[test]
fn only_its_number_and_confirmed_readback_unlock_the_next_step() {
    for early in [false, true] {
        let (mut g, mut s) = start();
        let Some(Command::SetEqGains(gains)) = g.action(Action::More, &s, false, false).unwrap()
        else {
            panic!()
        };
        g.done(99, &Outcome::Applied);
        s.equalizer.as_mut().unwrap().gains = gains;
        g.sync(&s);
        assert!(g.action(Action::Next, &s, false, false).is_err());
        if early {
            g.done(7, &Outcome::Applied);
        }
        g.submitted(7);
        if !early {
            g.done(7, &Outcome::Applied);
        }
        g.sync(&s);
        assert!(g.view(&s, false, false).can_next);
    }
}

#[test]
fn acknowledgment_mismatch_failure_and_unknown_never_count_as_audition_confirmed() {
    for outcome in [
        Outcome::Acknowledged,
        Outcome::Refused("x".into()),
        Outcome::Unknown("x".into()),
        Outcome::NotApplied("x".into()),
        Outcome::Applied,
    ] {
        let (mut g, s) = start();
        assert!(g.action(Action::More, &s, false, false).unwrap().is_some());
        g.submitted(1);
        g.done(1, &outcome);
        g.sync(&s);
        assert!(!g.view(&s, false, false).can_next);
        assert!(g.curve_to_save(&s, false).is_err());
        // Même si la dernière lecture est l'initiale, demander explicitement
        // une restauration relue après l'échec, jamais supposer la restauration.
        assert!(
            g.action(Action::Restore, &s, false, false)
                .unwrap()
                .is_some()
        );
    }
}

#[test]
fn context_changes_interrupt_without_replay_or_restore_on_another_headset() {
    for kind in 0..7 {
        let (mut g, mut s) = start();
        g.action(Action::More, &s, false, false).unwrap();
        match kind {
            0 => s.session += 1,
            1 => s.state = None,
            2 => s.read_only = Some("firmware non qualifié".into()),
            3 => s.equalizer.as_mut().unwrap().max_gain = 30,
            4 => {
                s.state.as_mut().unwrap().audio_mode =
                    Some(momentum_core::device::AudioMode::SpeechClarity)
            }
            5 => s.equalizer.as_mut().unwrap().bass_boost = true,
            _ => s.headset_id = Some("synthetique-autre".into()),
        }
        g.sync(&s);
        assert!(!g.view(&s, false, false).can_restore);
        assert!(g.action(Action::Restore, &s, false, false).is_err());
        assert_eq!(g.action(Action::Dismiss, &s, false, false).unwrap(), None);
        assert!(!g.active());
    }
}

#[test]
fn another_curve_interrupts_and_a_rejected_submission_is_visible() {
    let (mut g, mut s) = start();
    s.equalizer.as_mut().unwrap().gains[7] = 3;
    g.sync(&s);
    assert!(!g.view(&s, false, false).can_listen);
    assert!(g.view(&s, false, false).can_restore);
    g.action(Action::Restore, &s, false, false).unwrap();
    g.rejected();
    assert!(!g.view(&s, false, false).can_next);
    assert!(g.view(&s, false, false).can_dismiss);
}

#[test]
fn start_and_save_never_race_other_pending_edits() {
    let mut g = GuidedEq::default();
    let s = Demo::new().snapshot();
    for (busy, drafts) in [(true, false), (false, true)] {
        assert!(g.action(Action::Start, &s, busy, drafts).is_err());
        assert!(!g.active());
    }
    g.action(Action::Start, &s, false, false).unwrap();
    for _ in 0..3 {
        g.action(Action::Next, &s, false, false).unwrap();
    }
    assert!(g.curve_to_save(&s, true).is_err());
    assert!(g.curve_to_save(&s, false).is_ok());
    g.saved();
    assert!(!g.active());
}
