//! Identifiants de vendor, de feature et de commande GAIA V3.
//!
//! Source : PROTOCOL.md de Oein/sennheiser-desktop-client (analyse de Smart
//! Control, vérifiée sur ACCENTUM Wireless, structure partagée avec le
//! Momentum 4). Les GET marqués « M5 » ont été relevés sur un Momentum 5
//! (voir `tests/m5_captures.rs`) ; tout le reste est à valider sur le matériel.

use std::fmt;

/// Vendor Sennheiser : toutes les commandes spécifiques au casque.
pub const VENDOR_SENNHEISER: u16 = 0x0495;
/// Vendor Qualcomm « core » (fonctions GAIA génériques).
pub const VENDOR_QUALCOMM: u16 = 0x001D;

/// Identifiants de feature (les 7 bits de poids fort du command id).
pub mod feature {
    pub const BATTERY: u8 = 3;
    pub const GENERIC_AUDIO: u8 = 4;
    pub const USER_EQ: u8 = 8;
    pub const VERSIONS: u8 = 9;
    pub const DEVICE_MANAGEMENT: u8 = 10;
    pub const MMI: u8 = 11;
    pub const TRANSPARENT_HEARING: u8 = 12;
    pub const ANC: u8 = 13;
    /// Personnalisation du son (catalogue M5 : version 2).
    pub const SOUND_PERSONALIZATION: u8 = 16;
    /// Audio spatial (relevé dans la capture Smart Control Plus sur M5).
    pub const SPATIAL_AUDIO: u8 = 27;
}

/// Type de paquet, encodé dans les bits 7-8 du command id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PacketKind {
    /// Requête envoyée par l'hôte (bits = 00).
    Command,
    /// Notification asynchrone du casque (bits = 01, soit `id | 0x0080`).
    Notification,
    /// Réponse à une requête (bits = 10, soit `id | 0x0100`).
    Response,
    /// Réponse d'erreur (bits = 11, soit `id | 0x0180`).
    Error,
}

/// Command id GAIA V3 sur 16 bits.
///
/// Découpage : `feature (7 bits) | type (2 bits) | numéro de commande (7 bits)`.
/// Exemple : `0x0603` = feature 3 (batterie), type requête, commande 3.
/// Ce découpage explique les masques `0x0100` / `0x0080` / `0x0180` du doc.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(u16);

const KIND_MASK: u16 = 0x0180;
const KIND_SHIFT: u16 = 7;

impl CommandId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn feature(self) -> u8 {
        (self.0 >> 9) as u8
    }

    pub const fn number(self) -> u8 {
        (self.0 & 0x007F) as u8
    }

    pub const fn kind(self) -> PacketKind {
        match (self.0 & KIND_MASK) >> KIND_SHIFT {
            0 => PacketKind::Command,
            1 => PacketKind::Notification,
            2 => PacketKind::Response,
            _ => PacketKind::Error,
        }
    }

    const fn with_kind(self, bits: u16) -> Self {
        Self((self.0 & !KIND_MASK) | (bits << KIND_SHIFT))
    }

    /// La requête correspondante (bits de type remis à zéro).
    pub const fn request(self) -> Self {
        self.with_kind(0)
    }

    pub const fn notification(self) -> Self {
        self.with_kind(1)
    }

    pub const fn response(self) -> Self {
        self.with_kind(2)
    }

    pub const fn error(self) -> Self {
        self.with_kind(3)
    }
}

impl fmt::Debug for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CommandId({:#06X}: feature {}, {:?}, cmd {})",
            self.0,
            self.feature(),
            self.kind(),
            self.number()
        )
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04X}", self.0)
    }
}

/// Lectures (GET) du vendor 0x0495.
///
/// Les écritures sont dans [`set`], chacune ajoutée après capture de l'app et
/// validation sur le M5. DFU, réinitialisation d'usine et suppression d'un
/// appairage sont exclus du projet et ne seront jamais déclarés.
pub mod get {
    use super::CommandId;

    /// GET batterie → payload `[pourcentage]`. Validé en lecture sur M5.
    pub const BATTERY: CommandId = CommandId::new(0x0603);
    /// GET codec → payload `[codec]` (5 = aptX HD). Répond sur M5, mais la
    /// valeur observée (1) n'est pas documentée.
    pub const CODEC: CommandId = CommandId::new(0x0800);
    /// GET version firmware → 3 × u16. Validé en lecture sur M5 (6.21.5).
    pub const FIRMWARE_VERSION: CommandId = CommandId::new(0x1201);
    /// GET nom du modèle → chaîne ASCII terminée par `\0`. Validé sur M5.
    pub const MODEL_NAME: CommandId = CommandId::new(0x1206);
    /// GET mode de contrôle du bruit combiné (encodage Momentum 4 : paires
    /// `[mode, état]`). Répond sur M5 avec cet encodage ; sens des états à confirmer.
    pub const NOISE_CONTROL_MODES: CommandId = CommandId::new(0x1A01);
    /// GET niveau de transparence. Répond sur M5 (échelle à confirmer).
    pub const TRANSPARENCY_LEVEL: CommandId = CommandId::new(0x1A03);
    /// GET ANC on/off (encodage ACCENTUM). Répond sur M5.
    pub const ANC_ENABLED: CommandId = CommandId::new(0x1A05);
    /// GET transparence on/off (encodage ACCENTUM, feature 12). Répond sur M5.
    pub const TRANSPARENCY_ENABLED: CommandId = CommandId::new(0x1805);
    /// GET Smart Pause → `[0/1]`. Validé sur M5.
    pub const SMART_PAUSE: CommandId = CommandId::new(0x080D);
    /// GET détection de port (on-head). À valider sur le matériel M5.
    pub const ON_HEAD_DETECTION: CommandId = CommandId::new(0x0401);

    // Catalogue étendu : GET relevés dans le client macOS d'Oein (issus de
    // l'APK Smart Control Plus), envoyés seulement par `probe --deep`.
    // Tous à valider sur le matériel M5.

    /// GET révision matérielle → u16.
    pub const HARDWARE_REVISION: CommandId = CommandId::new(0x1200);
    /// GET sidetone (retour de sa propre voix en appel) → niveau 0 à 5.
    pub const SIDETONE: CommandId = CommandId::new(0x0806);
    /// GET invites sonores → 0 aucune, 1 tonalités, 2 tonalités et voix.
    pub const PROMPT_MODE: CommandId = CommandId::new(0x0802);
    /// GET langue des invites vocales.
    pub const PROMPT_LANGUAGE: CommandId = CommandId::new(0x0807);
    /// GET décroché automatique → [0|1].
    pub const AUTO_ANSWER: CommandId = CommandId::new(0x080B);
    /// GET Comfort Call → [0|1].
    pub const COMFORT_CALL: CommandId = CommandId::new(0x0815);
    /// GET mode faible latence → [0|1].
    pub const LOW_LATENCY: CommandId = CommandId::new(0x0818);
    /// GET « Qualité audio avancée » : 1 = activée (haute résolution
    /// coupée), 0 = haute résolution 24 bits/96 kHz activée. Capture du
    /// 09/10/2026 (écriture 0x0405, appliquée par un redémarrage 0x060F).
    pub const BT_COMPATIBILITY: CommandId = CommandId::new(0x0406);
    /// GET mode sans perte 16 bits/44,1 kHz → [0|1] (écriture 0x0407).
    pub const LOSSLESS: CommandId = CommandId::new(0x0408);

    /// GET catalogue des features Sennheiser : premier octet puis paires
    /// `(feature, version)` (lecture probable). Envoyé par Smart Control
    /// Plus à chaque connexion ; les numéros de feature sont propres au
    /// vendor 0x0495 (distincts de ceux du core Qualcomm).
    pub const FEATURE_CATALOG: CommandId = CommandId::new(0x0001);

    // Personnalisation du son (feature 16), capture du 09/10/2026 : l'app
    // envoie le profil calculé en trois blocs de flottants (0x2002, 0x2004,
    // 0x2006) ; l'état est notifié par 0x2081 (0 aucun profil, 1 pendant le
    // test, 2 après réception du profil). L'état 2 ne dit PAS si le profil
    // est le mode audio sélectionné : c'est le rôle de 0x0804.
    /// GET état de la personnalisation du son.
    pub const SOUND_PERSONALIZATION: CommandId = CommandId::new(0x2001);
    /// GET profil, bloc 1 : 6 × f32 (hypothèse : 3 paires de seuils en dB).
    pub const SOUND_PROFILE_1: CommandId = CommandId::new(0x2003);
    /// GET profil, bloc 2 : 6 × f32 (hypothèse : 3 paires de rapports).
    pub const SOUND_PROFILE_2: CommandId = CommandId::new(0x2005);
    /// GET profil, bloc 3 : 3 × f32 puis 2 octets.
    pub const SOUND_PROFILE_3: CommandId = CommandId::new(0x2007);
    /// GET mode audio → `[00, mode]` : 1 égaliseur, 2 Speech Clarity,
    /// 3 profil de personnalisation du son (sélection validée le 09/10/2026).
    pub const AUDIO_MODE: CommandId = CommandId::new(0x0804);
    /// GET volume des invites → `[i8]`, de −20 (0 %) à +6 (100 %), 0 = 77 %
    /// dans l'app ; échelle relevée le 09/10/2026.
    pub const PROMPT_VOLUME: CommandId = CommandId::new(0x081D);
    /// GET nom du casque (feature 20) → nom en UTF-8, lu par l'app.
    pub const DEVICE_NAME: CommandId = CommandId::new(0x2802);
    /// GET minuteur, payload `[timer id]` (0 = extinction auto) →
    /// `[id, valeur u16]`.
    pub const TIMER: CommandId = CommandId::new(0x0601);
    /// GET taille de la liste des appareils appairés au casque.
    pub const PAIRED_LIST_SIZE: CommandId = CommandId::new(0x1400);
    /// GET infos d'un appareil appairé, payload `[index]` →
    /// `[index, priorité, état, nom…]`. La suppression d'un appairage n'est
    /// jamais déclarée ici.
    pub const PAIRED_DEVICE_INFO: CommandId = CommandId::new(0x1401);
    /// GET liste des appareils appairés → `[n, n × adresse (6 octets)]`
    /// (capture F6 du 09/10/2026, « Gestion des connexions »).
    pub const CONNECTION_LIST: CommandId = CommandId::new(0x140D);
    /// GET fiche d'un appareil, payload `[adresse]` → `[adresse, état,
    /// 3 octets inconnus, nom…\0]` ; état 1 = non connecté, 2 = connecté,
    /// 3 = connecté et vraisemblablement en lecture audio (corrélé à
    /// l'affichage de l'app, 09/10/2026).
    pub const CONNECTION_INFO: CommandId = CommandId::new(0x140E);
    /// GET adresse de l'appareil qui interroge le casque (« Cet appareil »
    /// dans l'app).
    pub const OWN_ADDRESS: CommandId = CommandId::new(0x1413);
    /// GET configuration de l'égaliseur (feature 8) → `[bandes, min, max, …]`.
    pub const EQ_CONFIG: CommandId = CommandId::new(0x1000);
    /// GET gain d'une bande, payload `[bande]` → `[bande, gain i8 = dB × 10]`.
    pub const EQ_BAND: CommandId = CommandId::new(0x1002);
    /// GET Bass Boost → [0|1].
    pub const BASS_BOOST: CommandId = CommandId::new(0x1009);
    /// Identifiant de la notification `1082` (tous les gains, un octet par
    /// bande), relevée dans la capture ; ce n'est pas une requête envoyée.
    pub const EQ_ALL_GAINS_NOTIFICATION: CommandId = CommandId::new(0x1082);
    /// Notification `208A [0|1|2]` (capture HCI du 09/10/2026) : 0 en mode
    /// égaliseur, 1 profil personnalisé sélectionné, 2 Dolby Atmos activé.
    /// Lecture « profil utilisé / bloqué par Dolby Atmos » : hypothèse.
    pub const SOUND_PROFILE_USAGE_NOTIFICATION: CommandId = CommandId::new(0x208A);
    /// Notification `089A [u32]` : fréquence d'échantillonnage du flux audio
    /// en Hz (`00 00 AC 44` = 44 100 ; relevée pendant la lecture).
    pub const SAMPLE_RATE_NOTIFICATION: CommandId = CommandId::new(0x089A);

    /// GET protection de la batterie → [0|1] (capture du 09/10/2026).
    pub const BATTERY_PROTECTION: CommandId = CommandId::new(0x060B);
    /// GET verrouillage des commandes tactiles → [0|1] : **1 = désactivées**
    /// (logique inversée, capture du 09/10/2026).
    pub const TOUCH_LOCK: CommandId = CommandId::new(0x1607);

    /// GET audio spatial (feature 27) → [0|1]. Relevé dans une capture de
    /// Smart Control Plus sur M5 : l'app lit 0x3605, écrit 0x3604 [0|1] et
    /// reçoit la notification 0x3685 [0|1].
    pub const SPATIAL_AUDIO: CommandId = CommandId::new(0x3605);
    /// GET suivi de tête (feature 27) → [0|1], lu par l'app ; écriture
    /// `3602`, notification `3683` (essai E4.2 du 09/10/2026).
    pub const HEAD_TRACKING: CommandId = CommandId::new(0x3603);
    /// GET nombre de connexions simultanées → `[1]` sans Multipoint, `[2]`
    /// avec (relu par l'app après chaque changement, essai E4.3).
    pub const CONNECTION_SLOTS: CommandId = CommandId::new(0x1409);
}

/// Écritures (vendor 0x0495), **uniquement** celles relevées dans la capture
/// HCI de Smart Control Plus sur M5 (08/10/2026), puis **validées depuis le
/// PC** avec `ctl` le même jour (l'app a suivi chaque changement en direct).
/// Le casque répond par un accusé vide (`id | 0x0100`) puis notifie le
/// nouvel état.
pub mod set {
    use super::CommandId;

    /// ANC marche/arrêt : `[0|1]`.
    pub const ANC_ENABLED: CommandId = CommandId::new(0x1A04);
    /// Paire du contrôle du bruit : `[mode, état]` (mode 1 = anti-vent,
    /// 3 = adaptatif ; état 0/1).
    pub const NOISE_CONTROL_MODE: CommandId = CommandId::new(0x1A00);
    /// Curseur ANC ↔ transparence : `[0..=100]` (0 = ANC max, 100 =
    /// transparence max).
    pub const TRANSPARENCY_LEVEL: CommandId = CommandId::new(0x1A02);
    /// Audio spatial : `[0|1]`.
    pub const SPATIAL_AUDIO: CommandId = CommandId::new(0x3604);
    /// Suivi de tête : `3602 [0|1]`, réponse `3702` vide, notification
    /// `3683` (essai E4.2 : l'app ne le propose qu'avec l'audio spatial actif).
    pub const HEAD_TRACKING: CommandId = CommandId::new(0x3602);
    /// Gain d'une bande de l'égaliseur : `[bande, gain i8 = dB × 10]`.
    /// Capture du 09/10/2026 : un préréglage = 8 écritures successives.
    pub const EQ_BAND: CommandId = CommandId::new(0x1001);
    /// Amplification des basses : `[0|1]` (capture du 09/10/2026).
    pub const BASS_BOOST: CommandId = CommandId::new(0x1008);
    /// Mode audio : `[00, mode]`, 1 = égaliseur graphique, 2 = Speech Clarity
    /// (capture du 09/10/2026 ; les autres valeurs ne sont jamais envoyées).
    pub const AUDIO_MODE: CommandId = CommandId::new(0x0803);
    /// Protection de la batterie : `[0|1]` (capture du 09/10/2026).
    pub const BATTERY_PROTECTION: CommandId = CommandId::new(0x060A);
    /// Verrouillage des commandes tactiles : `[1]` désactive, `[0]` active.
    pub const TOUCH_LOCK: CommandId = CommandId::new(0x1606);
    /// Smart Pause : `[0|1]` (capture du 09/10/2026).
    pub const SMART_PAUSE: CommandId = CommandId::new(0x080C);
    /// Réponse automatique aux appels : `[0|1]`.
    pub const AUTO_ANSWER: CommandId = CommandId::new(0x080A);
    /// Appels confortables : `[0|1]`.
    pub const COMFORT_CALL: CommandId = CommandId::new(0x0814);
    /// Minuteur : `[timer id, secondes u16]` ; id 0 = extinction automatique
    /// (capture : 30 min = `00 07 08`, jamais = `00 00 00`).
    pub const TIMER: CommandId = CommandId::new(0x0600);
    /// Invites : `[1]` audio uniquement, `[2]` audio et vocales.
    pub const PROMPT_MODE: CommandId = CommandId::new(0x0801);
    /// Volume des invites : `[i8]` (décalage relatif, 0 = 77 % dans l'app).
    pub const PROMPT_VOLUME: CommandId = CommandId::new(0x081C);
    /// Connecter un appareil appairé : `[adresse]`, réponse `150F` vide
    /// (capture F6 du 09/10/2026).
    pub const CONNECT_DEVICE: CommandId = CommandId::new(0x140F);
    /// Déconnecter un appareil : `[adresse]`, réponse `1510` vide, puis
    /// notifications `1491 [adresse, 01, 00]` et `1484 [00 00]`.
    pub const DISCONNECT_DEVICE: CommandId = CommandId::new(0x1410);
    /// Nom du casque : le nom en octets, sans terminateur (capture F3 du
    /// 09/10/2026 : `2801 "M5 Essai"`, réponse `2901` vide).
    pub const DEVICE_NAME: CommandId = CommandId::new(0x2801);
}

/// Requêtes du « core » Qualcomm (vendor 0x001D), en lecture seule.
pub mod core_get {
    use super::CommandId;

    /// Liste des features GAIA prises en charge (format de réponse non documenté).
    pub const SUPPORTED_FEATURES: CommandId = CommandId::new(0x0001);
    /// Numéro de série (chaîne).
    pub const SERIAL_NUMBER: CommandId = CommandId::new(0x0003);
}

/// Écritures du « core » Qualcomm (vendor 0x001D), relevées dans l'app.
pub mod core_set {
    use super::CommandId;

    /// Multipoint : `001D:0E00 [0|1]`, réponse `0F00` vide, notification
    /// `0E80 [état]` (essai E4.3 du 09/10/2026). Couper le Multipoint ne
    /// garde qu'un appareil connecté.
    pub const MULTIPOINT: CommandId = CommandId::new(0x0E00);
}

/// Enregistrement aux notifications d'une feature : payload `[feature id]`.
///
/// Écriture sans effet sur les réglages du casque, mais le vendor à utiliser
/// (0x0495 ou 0x001D) n'est pas explicite dans la doc : à valider sur le
/// matériel. Non utilisé en phase 2.
pub const REGISTER_NOTIFICATION: CommandId = CommandId::new(0x0007);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_id_layout() {
        let id = get::BATTERY;
        assert_eq!(id.feature(), feature::BATTERY);
        assert_eq!(id.kind(), PacketKind::Command);
        assert_eq!(id.number(), 3);
    }

    #[test]
    fn kind_masks_match_documented_convention() {
        let id = get::BATTERY;
        assert_eq!(id.response().raw(), 0x0603 | 0x0100);
        assert_eq!(id.notification().raw(), 0x0603 | 0x0080);
        assert_eq!(id.error().raw(), 0x0603 | 0x0180);
        assert_eq!(id.error().request(), id);
        assert_eq!(CommandId::new(0x0683).kind(), PacketKind::Notification);
        assert_eq!(CommandId::new(0x0703).kind(), PacketKind::Response);
        assert_eq!(CommandId::new(0x0783).kind(), PacketKind::Error);
    }

    #[test]
    fn anc_and_transparency_features() {
        assert_eq!(get::NOISE_CONTROL_MODES.feature(), feature::ANC);
        assert_eq!(
            get::TRANSPARENCY_ENABLED.feature(),
            feature::TRANSPARENT_HEARING
        );
        assert_eq!(get::FIRMWARE_VERSION.feature(), feature::VERSIONS);
    }
}
