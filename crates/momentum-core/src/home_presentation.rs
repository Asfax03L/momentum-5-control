//! Présentation de l'accueil et raisons de disponibilité, sans accès matériel.
//! Une valeur absente ne permet pas de distinguer un refus d'une lecture non
//! terminée : le texte reste volontairement neutre.

use crate::device::{AudioMode, DeviceInfo, HeadsetState};
use crate::i18n::{tr, trf};
use crate::protocol::messages::{ChargerStatus, M5NoiseMode, OnHeadState};
use crate::viewmodel::stream_label;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    Available,
    #[default]
    Disconnected,
    ReadOnly,
    Unread,
    CustomRequired,
    SpatialRequired,
    ProfileSelected,
}

impl Availability {
    pub fn enabled(self) -> bool {
        self == Self::Available
    }

    /// Explication adjacente au contrôle, y compris quand il est grisé.
    pub fn reason(self) -> String {
        match self {
            Self::Available => "",
            Self::Disconnected => tr("Connectez le casque pour utiliser ce réglage."),
            Self::ReadOnly => tr("Lecture seule : les réglages du casque ne sont pas modifiables."),
            Self::Unread => tr("Valeur non disponible pour cette connexion."),
            Self::CustomRequired => tr("Choisissez Personnalisé pour ajuster le niveau."),
            Self::SpatialRequired => tr("Activez l'audio spatial pour régler le suivi de tête."),
            Self::ProfileSelected => {
                tr("L'audio spatial est indisponible avec le profil personnalisé sélectionné.")
            }
        }
        .to_string()
    }
}

pub fn availability(connected: bool, writable: bool, known: bool) -> Availability {
    if !connected {
        Availability::Disconnected
    } else if !known {
        Availability::Unread
    } else if !writable {
        Availability::ReadOnly
    } else {
        Availability::Available
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadsetSummary {
    pub charge: String,
    pub wearing: String,
    pub stream: String,
}

/// L'état de charge ne dépend jamais du pourcentage de batterie. Les codes
/// inconnus restent inconnus, y compris 02 (fin de charge non observée).
pub fn headset_summary(state: Option<&HeadsetState>) -> HeadsetSummary {
    let Some(s) = state else {
        return HeadsetSummary {
            charge: tr("État de charge non disponible").into(),
            wearing: tr("État de port non disponible").into(),
            stream: tr("Flux audio non disponible").into(),
        };
    };
    HeadsetSummary {
        charge: match s.charge_state.map(|r| r.primary_status) {
            None => tr("État de charge non disponible").into(),
            Some(ChargerStatus::Disconnected) => tr("Sur batterie").into(),
            Some(ChargerStatus::Charging) => tr("En charge").into(),
            Some(ChargerStatus::Unknown(code)) => {
                trf("Charge inconnue (0x{})", &[&format!("{code:02X}")])
            }
        },
        wearing: match s.on_head_state {
            None => tr("État de port non disponible").into(),
            Some(OnHeadState::Worn) => tr("Porté").into(),
            Some(OnHeadState::NotWorn) => tr("Non porté").into(),
            Some(OnHeadState::Unknown(code)) => {
                trf("Port inconnu (0x{})", &[&format!("{code:02X}")])
            }
        },
        stream: if s.codec.is_none() {
            tr("Flux audio non disponible").into()
        } else {
            stream_label(s.codec, s.sample_rate_hz)
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeAvailability {
    pub noise: Availability,
    pub level: Availability,
    pub anti_wind: Availability,
    pub spatial: Availability,
    pub tracking: Availability,
    pub touch: Availability,
    pub smart_pause: Availability,
    pub multipoint: Availability,
}

pub fn home_availability(
    state: Option<&HeadsetState>,
    info: Option<&DeviceInfo>,
    writable: bool,
) -> HomeAvailability {
    let Some(s) = state else {
        return HomeAvailability::default();
    };
    let base = availability(true, writable, true);
    let mut level = base;
    if level.enabled() && !matches!(s.noise.mode(), M5NoiseMode::Custom { .. }) {
        level = Availability::CustomRequired;
    }
    let mut spatial = availability(true, writable, s.spatial_audio.is_some());
    if spatial.enabled()
        && s.audio_mode == Some(AudioMode::Personalization)
        && s.spatial_audio == Some(false)
    {
        spatial = Availability::ProfileSelected;
    }
    let mut tracking = availability(true, writable, s.head_tracking.is_some());
    if tracking.enabled() && s.spatial_audio != Some(true) {
        tracking = if s.spatial_audio.is_none() {
            Availability::Unread
        } else {
            Availability::SpatialRequired
        };
    }
    HomeAvailability {
        noise: base,
        level,
        anti_wind: base,
        spatial,
        tracking,
        touch: availability(true, writable, s.touch_controls.is_some()),
        smart_pause: availability(true, writable, s.smart_pause.is_some()),
        multipoint: availability(true, writable, info.is_some_and(|i| i.multipoint.is_some())),
    }
}
