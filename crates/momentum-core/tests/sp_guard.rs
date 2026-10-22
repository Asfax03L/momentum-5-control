//! Garde de la sélection du profil de personnalisation du son : trames
//! synthétiques sur MockTransport, aucun casque ni profil réel.
//!
//! Contexte (recherche du 12/10/2026) : le casque refuse un `0803 [00 03]`
//! redondant par `0983 [80]` (`modeIsAlreadySet` dans le décodeur de
//! Smart Control Plus, analyse statique). La sélection lit donc le mode
//! (`0804`) dans la même opération et n'écrit pas s'il est déjà actif.

use std::time::Duration;

use momentum_core::client::ClientError;
use momentum_core::device::{
    DeviceError, Headset, HeadsetState, ProfileSelection, audio_mode_error_meaning,
};
use momentum_core::protocol::{CommandId, Frame, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;

const T: Duration = Duration::from_millis(20);
const QUALCOMM: u16 = 0x001D;

fn frame(vendor: u16, id: u16, payload: &[u8]) -> Vec<u8> {
    Frame::new(vendor, CommandId::new(id), payload.to_vec())
        .encode()
        .unwrap()
}

fn s(id: u16, payload: &[u8]) -> Vec<u8> {
    frame(VENDOR_SENNHEISER, id, payload)
}

/// Profil reçu (`2001 → [02]`), spatial coupé (`3605 → [00]`), puis la
/// réponse (et ce qui l'accompagne) à la lecture du mode `0804`.
fn ready(mode_reply: &[u8]) -> MockTransport {
    MockTransport::new()
        .expect(&s(0x2001, &[]), &s(0x2101, &[2]))
        .expect(&s(0x3605, &[]), &s(0x3705, &[0]))
        .expect(&s(0x0804, &[]), mode_reply)
}

fn set_profile() -> Vec<u8> {
    s(0x0803, &[0, 3])
}

fn sent_set(h: &mut Headset<MockTransport>) -> bool {
    h.client()
        .transport()
        .sent()
        .iter()
        .any(|f| *f == set_profile())
}

fn headset(mock: MockTransport) -> Headset<MockTransport> {
    Headset::validated_for_tests(mock, T)
}

fn pending(h: &mut Headset<MockTransport>) -> Vec<Frame> {
    h.poll_notifications(Duration::ZERO).unwrap()
}

#[test]
fn profile_already_active_is_not_written_again() {
    let mut h = headset(ready(&s(0x0904, &[0, 3])));
    assert_eq!(
        h.select_sound_profile().unwrap(),
        ProfileSelection::AlreadySelectedAtRead
    );
    assert!(!sent_set(&mut h));
    assert_eq!(h.client().transport().sent().len(), 3);
    assert!(h.client().transport().is_exhausted());
}

#[test]
fn other_modes_and_unknown_values_keep_the_normal_write() {
    for mode in [1u8, 2, 0x7F] {
        let mock = ready(&s(0x0904, &[0, mode])).expect(&set_profile(), &s(0x0903, &[]));
        let mut h = headset(mock);
        assert_eq!(
            h.select_sound_profile().unwrap(),
            ProfileSelection::WriteAcknowledged,
            "mode {mode}"
        );
        assert!(h.client().transport().is_exhausted());
    }
}

#[test]
fn the_app_cache_never_replaces_the_fresh_reading() {
    // Le cache dit « profil », le casque dit « égaliseur » : on écrit.
    let cache = HeadsetState {
        audio_mode: Some(momentum_core::device::AudioMode::Personalization),
        ..HeadsetState::default()
    };
    let mock = ready(&s(0x0904, &[0, 1])).expect(&set_profile(), &s(0x0903, &[]));
    let mut h = headset(mock);
    assert_eq!(
        h.select_sound_profile().unwrap(),
        ProfileSelection::WriteAcknowledged
    );
    assert_eq!(
        cache.audio_mode,
        Some(momentum_core::device::AudioMode::Personalization)
    );
}

#[test]
fn spatial_audio_is_still_switched_off_first_and_separately() {
    // Logique existante conservée : 3604 [00] acquitté, puis la garde.
    let mock = MockTransport::new()
        .expect(&s(0x2001, &[]), &s(0x2101, &[2]))
        .expect(&s(0x3605, &[]), &s(0x3705, &[1]))
        .expect(&s(0x3604, &[0]), &s(0x3704, &[]))
        .expect(&s(0x0804, &[]), &s(0x0904, &[0, 3]));
    let mut h = headset(mock);
    assert_eq!(
        h.select_sound_profile().unwrap(),
        ProfileSelection::AlreadySelectedAtRead
    );
    assert!(h.client().transport().is_exhausted());
}

#[test]
fn preconditions_stop_before_any_write() {
    // Profil en cours de traitement ou absent.
    for status in [1u8, 0] {
        let mock = MockTransport::new().expect(&s(0x2001, &[]), &s(0x2101, &[status]));
        let mut h = headset(mock);
        assert!(matches!(
            h.select_sound_profile(),
            Err(DeviceError::NoSoundProfile(Some(v))) if v == status
        ));
        assert_eq!(h.client().transport().sent().len(), 1);
    }
    // État ou spatial mal formés.
    for status in [&[][..], &[2, 0][..]] {
        let mock = MockTransport::new().expect(&s(0x2001, &[]), &s(0x2101, status));
        let mut h = headset(mock);
        assert!(matches!(
            h.select_sound_profile(),
            Err(DeviceError::Protocol(_))
        ));
        assert_eq!(h.client().transport().sent().len(), 1);
    }
    let mock = MockTransport::new()
        .expect(&s(0x2001, &[]), &s(0x2101, &[2]))
        .expect(&s(0x3605, &[]), &s(0x3705, &[0, 0]));
    let mut h = headset(mock);
    assert!(matches!(
        h.select_sound_profile(),
        Err(DeviceError::Protocol(_))
    ));
    assert_eq!(h.client().transport().sent().len(), 2);
}

#[test]
fn malformed_mode_readings_stop_without_writing() {
    for payload in [&[][..], &[0][..], &[0, 3, 9][..], &[1, 3][..]] {
        let mut h = headset(ready(&s(0x0904, payload)));
        assert!(
            matches!(h.select_sound_profile(), Err(DeviceError::Protocol(_))),
            "{payload:?}"
        );
        assert!(!sent_set(&mut h));
    }
}

#[test]
fn a_mode_notification_during_or_after_the_reading_invalidates_it() {
    // Avant la réponse, puis en suffixe dans le même bloc reçu.
    for suffix in [false, true] {
        let note = s(0x0884, &[0, 1]);
        let reply = s(0x0904, &[0, 3]);
        let bytes = if suffix {
            [reply, note].concat()
        } else {
            [note, reply].concat()
        };
        let mock = ready(&bytes).expect(&set_profile(), &s(0x0903, &[]));
        let mut h = headset(mock);
        assert_eq!(
            h.select_sound_profile().unwrap(),
            ProfileSelection::WriteAcknowledged,
            "suffixe : {suffix}"
        );
        // La notification reste pour le propriétaire de la connexion.
        let events = pending(&mut h);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].command.raw(), 0x0884);
    }
}

#[test]
fn profile_status_usage_and_spatial_on_also_invalidate() {
    for note in [s(0x2081, &[2]), s(0x208A, &[1]), s(0x3685, &[1])] {
        let bytes = [note.clone(), s(0x0904, &[0, 3])].concat();
        let mock = ready(&bytes).expect(&set_profile(), &s(0x0903, &[]));
        let mut h = headset(mock);
        assert_eq!(
            h.select_sound_profile().unwrap(),
            ProfileSelection::WriteAcknowledged
        );
        assert_eq!(pending(&mut h).len(), 1);
    }
}

#[test]
fn unrelated_or_foreign_notifications_do_not_invalidate_and_are_kept() {
    for note in [
        s(0x0683, &[80]),                 // batterie
        s(0x3685, &[0]),                  // spatial coupé (suite de notre propre 3604)
        frame(QUALCOMM, 0x0884, &[0, 1]), // même ID, autre vendor
    ] {
        let bytes = [note, s(0x0904, &[0, 3])].concat();
        let mut h = headset(ready(&bytes));
        assert_eq!(
            h.select_sound_profile().unwrap(),
            ProfileSelection::AlreadySelectedAtRead
        );
        assert!(!sent_set(&mut h));
        assert_eq!(pending(&mut h).len(), 1);
    }
}

#[test]
fn notifications_older_than_the_reading_do_not_decide() {
    let mut mock = ready(&s(0x0904, &[0, 3]));
    mock.push_incoming(&s(0x0884, &[0, 1]));
    let mut h = headset(mock);
    assert_eq!(
        h.select_sound_profile().unwrap(),
        ProfileSelection::AlreadySelectedAtRead
    );
    assert_eq!(pending(&mut h).len(), 1);
}

#[test]
fn a_fragmented_reply_with_notification_is_read_in_full() {
    let bytes = [s(0x2081, &[1]), s(0x0904, &[0, 3])].concat();
    let mock = MockTransport::new()
        .expect(&s(0x2001, &[]), &s(0x2101, &[2]))
        .expect(&s(0x3605, &[]), &s(0x3705, &[0]))
        .expect_chunked(&s(0x0804, &[]), &[&bytes[..4], &bytes[4..12], &bytes[12..]])
        .expect(&set_profile(), &s(0x0903, &[]));
    let mut h = headset(mock);
    assert_eq!(
        h.select_sound_profile().unwrap(),
        ProfileSelection::WriteAcknowledged
    );
}

#[test]
fn a_refused_write_stays_an_error_with_its_code() {
    // Lecture « égaliseur », puis refus 0983 [80] : erreur, pas un succès,
    // même si une relecture montrerait ensuite le profil.
    let mock = ready(&s(0x0904, &[0, 1])).expect(
        &set_profile(),
        &[s(0x2081, &[2]), s(0x0983, &[0x80])].concat(),
    );
    let mut h = headset(mock);
    let err = h.select_sound_profile().unwrap_err();
    assert!(matches!(&err, DeviceError::AudioModeRefused(p) if p == &[0x80]));
    let text = err.to_string();
    assert!(
        text.contains("0983") && text.contains("déjà sélectionné"),
        "{text}"
    );
    // Pas de désynchronisation : refus définitif ; notification gardée.
    assert!(!h.client().is_desynchronized());
    assert_eq!(pending(&mut h).len(), 1);
}

#[test]
fn unknown_refusal_codes_keep_their_raw_value() {
    let mock = ready(&s(0x0904, &[0, 1])).expect(&set_profile(), &s(0x0983, &[0x99]));
    let mut h = headset(mock);
    let err = h.select_sound_profile().unwrap_err();
    assert!(matches!(&err, DeviceError::AudioModeRefused(p) if p == &[0x99]));
    assert!(err.to_string().contains("code 99 inconnu"));
    assert_eq!(
        audio_mode_error_meaning(0x80),
        Some("mode déjà sélectionné")
    );
    assert_eq!(audio_mode_error_meaning(0x99), None);
}

#[test]
fn code_80_of_other_commands_is_not_read_as_mode_already_set() {
    // Erreur 80 sur la lecture d'état du profil (autre namespace) : erreur
    // générique du client, sans sens « mode déjà sélectionné ».
    let mock = MockTransport::new().expect(
        &s(0x2001, &[]),
        &[s(0x2081, &[1]), s(0x2181, &[0x80])].concat(),
    );
    let mut h = headset(mock);
    let err = h.select_sound_profile().unwrap_err();
    assert!(matches!(
        &err,
        DeviceError::Client(ClientError::Device { command, payload })
            if command.raw() == 0x2181 && payload == &[0x80]
    ));
    assert!(!err.to_string().contains("déjà sélectionné"));
    assert_eq!(pending(&mut h).len(), 1);

    // Erreur 80 sur la lecture du mode (0984) : pas une preuve de mode actif.
    let mut h = headset(ready(&s(0x0984, &[0x80])));
    assert!(matches!(
        h.select_sound_profile(),
        Err(DeviceError::Client(ClientError::Device { command, .. })) if command.raw() == 0x0984
    ));
    assert!(!sent_set(&mut h));
}

#[test]
fn a_timeout_stops_without_retry_and_keeps_parsed_notifications() {
    let mut h = headset(ready(&s(0x2081, &[1])));
    assert!(matches!(
        h.select_sound_profile(),
        Err(DeviceError::Client(ClientError::Transport(_)))
    ));
    assert!(h.client().is_desynchronized());
    assert!(!sent_set(&mut h));
    // La notification déjà décodée reste disponible.
    assert_eq!(h.client().pending_notifications().len(), 1);
    // Une nouvelle tentative n'envoie rien sur la session désynchronisée.
    assert!(h.select_sound_profile().is_err());
    assert_eq!(h.client().transport().sent().len(), 3);
}

#[test]
fn speech_clarity_refusals_use_the_same_command_namespace() {
    let mock = MockTransport::new().expect(&s(0x0803, &[0, 2]), &s(0x0983, &[0x82]));
    let mut h = headset(mock);
    let err = h.set_speech_clarity(true).unwrap_err();
    assert!(err.to_string().contains("mode non pris en charge"));
}
