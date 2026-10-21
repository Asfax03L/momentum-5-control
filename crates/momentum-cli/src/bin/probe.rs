//! `probe` : diagnostic Bluetooth du casque, en lecture seule.
//!
//! 1. liste les appareils Bluetooth appairés (Classic et LE) ;
//! 2. pour le casque, énumère les services RFCOMM (avec attributs SDP bruts)
//!    et les services GATT (UUID et propriétés, sans lire ni écrire) ;
//! 3. se connecte au service RFCOMM nommé « GAIA » (ou celui de `--service`) ;
//! 4. envoie GET batterie (et les autres GET documentés avec `--all-gets`)
//!    et affiche les trames brutes en hexadécimal.
//!
//! Le rapport s'affiche sur la sortie standard ; `--output` l'enregistre aussi.

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "probe",
    version,
    about = "Diagnostic Bluetooth en lecture seule pour casques Sennheiser (GAIA V3)"
)]
struct Args {
    /// Filtre sur le nom de l'appareil (par défaut : « momentum » ou « sennheiser »).
    #[arg(long)]
    device: Option<String>,

    /// UUID du service RFCOMM à utiliser si aucun ne s'appelle « GAIA ».
    #[arg(long)]
    service: Option<String>,

    /// Envoie aussi les autres GET documentés (modèle, firmware, codec, ANC…).
    #[arg(long)]
    all_gets: bool,

    /// Inventaire complet : tous les GET connus (réglages, égaliseur, appareils
    /// appairés au casque, features Qualcomm). Implique --all-gets.
    #[arg(long)]
    deep: bool,

    /// Enregistre aussi le rapport dans ce fichier.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Journal détaillé sur la sortie d'erreur, dont les trames brutes.
    #[arg(long, short)]
    verbose: bool,

    /// Affiche les adresses Bluetooth complètes (masquées par défaut).
    #[arg(long)]
    show_address: bool,

    /// Délai d'attente de chaque réponse, en millisecondes.
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
}

/// Rapport texte : chaque ligne est affichée tout de suite et conservée.
#[derive(Default)]
struct Report {
    text: String,
}

impl Report {
    fn section(&mut self, title: &str) {
        self.line("");
        self.line(format!("=== {title} ==="));
    }

    fn line(&mut self, line: impl AsRef<str>) {
        let line = line.as_ref();
        println!("{line}");
        self.text.push_str(line);
        self.text.push('\n');
    }
}

fn init_tracing(verbose: bool) {
    let level = if verbose { "trace" } else { "warn" };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(format!("momentum_core={level},probe={level}"))
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_tracing(args.verbose);

    let mut report = Report::default();
    report.line("Rapport probe momentum-ctl-win (projet non officiel, non affilié à Sennheiser)");

    #[cfg(windows)]
    win::run(&args, &mut report);
    #[cfg(not(windows))]
    report.line("ERREUR : probe ne fonctionne que sous Windows.");

    report.section("Fin");
    report.line("Si une étape a échoué, copiez tout ce rapport et collez-le dans la discussion.");

    if let Some(path) = &args.output {
        std::fs::write(path, &report.text)
            .map_err(|e| anyhow::anyhow!("écriture de {} impossible : {e}", path.display()))?;
        println!("Rapport enregistré dans {}", path.display());
    }
    Ok(())
}

#[cfg(windows)]
mod win {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use momentum_core::client::{ClientError, GaiaClient, RawEvent};
    use momentum_core::hex::to_hex;
    use momentum_core::protocol::command::{core_get, get};
    use momentum_core::protocol::messages::describe;
    use momentum_core::protocol::{CommandId, Frame, VENDOR_QUALCOMM, VENDOR_SENNHEISER};
    use momentum_core::sdp::{self, DataElement};
    use momentum_core::transport::TransportError;
    use momentum_core::transport::windows_rfcomm::RfcommTransport;
    use windows::Devices::Bluetooth::GenericAttributeProfile::{
        GattCharacteristicProperties, GattCommunicationStatus,
    };
    use windows::Devices::Bluetooth::Rfcomm::RfcommDeviceService;
    use windows::Devices::Bluetooth::{
        BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDevice, BluetoothError,
        BluetoothLEDevice,
    };
    use windows::Devices::Enumeration::DeviceInformation;
    use windows::Storage::Streams::{DataReader, IBuffer};
    use windows::System::Profile::AnalyticsInfo;
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::core::{GUID, HSTRING};

    use super::{Args, Report};

    /// Requêtes envoyées avec `--all-gets` (toutes en lecture seule).
    const EXTRA_GETS: &[(&str, CommandId)] = &[
        ("modèle", get::MODEL_NAME),
        ("firmware", get::FIRMWARE_VERSION),
        ("codec", get::CODEC),
        ("contrôle du bruit (encodage M4)", get::NOISE_CONTROL_MODES),
        ("ANC on/off (encodage ACCENTUM)", get::ANC_ENABLED),
        (
            "transparence on/off (encodage ACCENTUM)",
            get::TRANSPARENCY_ENABLED,
        ),
        ("niveau de transparence", get::TRANSPARENCY_LEVEL),
    ];

    /// Requêtes sans paramètre ajoutées par `--deep` (catalogue étendu, lecture seule).
    const DEEP_GETS: &[(&str, CommandId)] = &[
        ("révision matérielle", get::HARDWARE_REVISION),
        ("Smart Pause", get::SMART_PAUSE),
        ("détection de port", get::ON_HEAD_DETECTION),
        ("sidetone", get::SIDETONE),
        ("invites sonores", get::PROMPT_MODE),
        ("langue des invites", get::PROMPT_LANGUAGE),
        ("décroché automatique", get::AUTO_ANSWER),
        ("Comfort Call", get::COMFORT_CALL),
        ("faible latence", get::LOW_LATENCY),
        ("priorité du mode audio", get::BT_COMPATIBILITY),
        ("mode sans perte", get::LOSSLESS),
        ("personnalisation du son", get::SOUND_PERSONALIZATION),
        ("profil de son, bloc 1", get::SOUND_PROFILE_1),
        ("profil de son, bloc 2", get::SOUND_PROFILE_2),
        ("profil de son, bloc 3", get::SOUND_PROFILE_3),
        ("mode audio", get::AUDIO_MODE),
        ("Bass Boost", get::BASS_BOOST),
        ("audio spatial", get::SPATIAL_AUDIO),
    ];

    /// Nombre maximal de bandes d'égaliseur / d'appareils interrogés.
    const MAX_ITEMS: u8 = 10;

    /// Fin de l'UUID de base Bluetooth : xxxxxxxx-0000-1000-8000-00805F9B34FB.
    const BASE_UUID_TAIL: (u16, u16, [u8; 8]) = (
        0x0000,
        0x1000,
        [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
    );

    type WinResult<T> = windows::core::Result<T>;

    struct PairedDevice {
        name: String,
        id: HSTRING,
        address: Option<u64>,
    }

    pub fn run(args: &Args, report: &mut Report) {
        // Initialise WinRT en mode multithread (MTA) : les callbacks des
        // opérations asynchrones peuvent alors arriver sur n'importe quel thread.
        // SAFETY: appel unique au démarrage, avant tout objet WinRT.
        if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            report.line(format!("Avertissement RoInitialize : {}", win_err(&e)));
        }

        environment(report);

        let filters: Vec<String> = match &args.device {
            Some(f) => vec![f.to_lowercase()],
            None => vec!["momentum".into(), "sennheiser".into()],
        };
        let matches = |name: &str| {
            let name = name.to_lowercase();
            filters.iter().any(|f| name.contains(f.as_str()))
        };

        report.section("Appareils Bluetooth Classic appairés");
        let classic = list_paired(report, false, args.show_address);
        report.section("Appareils Bluetooth LE appairés");
        let le = list_paired(report, true, args.show_address);

        let targets: Vec<&PairedDevice> = classic.iter().filter(|d| matches(&d.name)).collect();
        let le_targets: Vec<&PairedDevice> = le.iter().filter(|d| matches(&d.name)).collect();
        report.section("Sélection du casque");
        report.line(format!("Filtre de nom : {filters:?}"));
        if targets.is_empty() && le_targets.is_empty() {
            report.line("Aucun appareil appairé ne correspond. Vérifiez l'appairage dans");
            report.line("Paramètres > Bluetooth et appareils, ou utilisez --device <nom>.");
            return;
        }
        report.line(format!(
            "{} appareil(s) Classic et {} appareil(s) LE retenus.",
            targets.len(),
            le_targets.len()
        ));

        let mut gaia_candidates = Vec::new();
        for dev in &targets {
            report.section(&format!("Services RFCOMM de « {} »", dev.name));
            gaia_candidates.extend(list_rfcomm(
                report,
                dev,
                args.service.as_deref(),
                args.show_address,
            ));
        }

        // GATT : appareils LE appairés retenus, sinon l'adresse Classic
        // (les casques bimodes exposent parfois le GATT à la même adresse).
        if le_targets.is_empty() {
            for dev in &targets {
                if let Some(addr) = dev.address {
                    report.section(&format!(
                        "Services GATT de « {} » (adresse Classic)",
                        dev.name
                    ));
                    list_gatt(
                        report,
                        BluetoothLEDevice::FromBluetoothAddressAsync(addr).and_then(|op| op.join()),
                    );
                }
            }
        } else {
            for dev in &le_targets {
                report.section(&format!("Services GATT de « {} »", dev.name));
                list_gatt(
                    report,
                    BluetoothLEDevice::FromIdAsync(&dev.id).and_then(|op| op.join()),
                );
            }
        }

        report.section("Connexion GAIA (RFCOMM)");
        let Some((label, service)) = gaia_candidates.into_iter().next() else {
            report.line("Aucun service RFCOMM nommé « GAIA » trouvé : AUCUNE trame envoyée.");
            report.line("Relancez avec --service <UUID> en choisissant un UUID non standard");
            report.line("de la liste ci-dessus (évitez les profils audio connus).");
            return;
        };
        report.line(format!("Service retenu : {label}"));
        talk_gaia(report, &service, args);
    }

    fn environment(report: &mut Report) {
        report.section("Environnement");
        report.line(format!("probe {}", env!("CARGO_PKG_VERSION")));
        let version = AnalyticsInfo::VersionInfo()
            .and_then(|v| v.DeviceFamilyVersion())
            .map(|v| {
                // Version empaquetée en u64 : 4 champs de 16 bits.
                v.to_string().parse::<u64>().map_or(v.to_string(), |n| {
                    format!(
                        "{}.{}.{}.{}",
                        n >> 48,
                        (n >> 32) & 0xFFFF,
                        (n >> 16) & 0xFFFF,
                        n & 0xFFFF
                    )
                })
            });
        match version {
            Ok(v) => report.line(format!("Windows {v}")),
            Err(e) => report.line(format!("Version de Windows inconnue : {}", win_err(&e))),
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        report.line(format!("Horodatage Unix : {now}"));
    }

    fn list_paired(report: &mut Report, le: bool, show_address: bool) -> Vec<PairedDevice> {
        let found = (|| -> WinResult<Vec<PairedDevice>> {
            let selector = if le {
                BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?
            } else {
                BluetoothDevice::GetDeviceSelectorFromPairingState(true)?
            };
            let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
            let mut out = Vec::new();
            for i in 0..infos.Size()? {
                let info = infos.GetAt(i)?;
                out.push(PairedDevice {
                    name: info.Name()?.to_string(),
                    id: info.Id()?,
                    address: None,
                });
            }
            Ok(out)
        })();

        let mut devices = match found {
            Ok(d) => d,
            Err(e) => {
                report.line(format!("ERREUR énumération : {}", win_err(&e)));
                return Vec::new();
            }
        };
        if devices.is_empty() {
            report.line("(aucun)");
        }
        for dev in &mut devices {
            let details = if le {
                BluetoothLEDevice::FromIdAsync(&dev.id)
                    .and_then(|op| op.join())
                    .and_then(|d| Ok((d.BluetoothAddress()?, d.ConnectionStatus()?)))
            } else {
                BluetoothDevice::FromIdAsync(&dev.id)
                    .and_then(|op| op.join())
                    .and_then(|d| Ok((d.BluetoothAddress()?, d.ConnectionStatus()?)))
            };
            match details {
                Ok((addr, status)) => {
                    dev.address = Some(addr);
                    let connected = if status == BluetoothConnectionStatus::Connected {
                        "connecté"
                    } else {
                        "déconnecté"
                    };
                    report.line(format!(
                        "- « {} » [{}] {connected}",
                        dev.name,
                        fmt_address(addr, show_address)
                    ));
                }
                Err(e) => report.line(format!("- « {} » : {}", dev.name, win_err(&e))),
            }
        }
        devices
    }

    /// Liste les services RFCOMM et renvoie ceux qui ressemblent à GAIA.
    fn list_rfcomm(
        report: &mut Report,
        dev: &PairedDevice,
        wanted_uuid: Option<&str>,
        show_address: bool,
    ) -> Vec<(String, RfcommDeviceService)> {
        let services = (|| -> WinResult<_> {
            let device = BluetoothDevice::FromIdAsync(&dev.id)?.join()?;
            // Uncached : on interroge vraiment le casque (SDP), pas le cache Windows.
            let result = device
                .GetRfcommServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                .join()?;
            Ok((result.Error()?, result.Services()?))
        })();
        let (error, services) = match services {
            Ok(s) => s,
            Err(e) => {
                report.line(format!("ERREUR requête SDP : {}", win_err(&e)));
                return Vec::new();
            }
        };
        if error != BluetoothError::Success {
            report.line(format!(
                "BluetoothError = {} ({})",
                error.0,
                bluetooth_error_label(error)
            ));
        }

        let wanted = wanted_uuid.map(normalize_uuid);
        let mut candidates = Vec::new();
        let count = services.Size().unwrap_or(0);
        report.line(format!("{count} service(s) RFCOMM."));
        for i in 0..count {
            let Ok(service) = services.GetAt(i) else {
                continue;
            };
            let uuid = service.ServiceId().and_then(|s| s.Uuid());
            let uuid_text = uuid.as_ref().map_or_else(win_err, |g| format!("{g:?}"));
            let label = uuid
                .as_ref()
                .ok()
                .and_then(short_uuid)
                .and_then(rfcomm_label)
                .unwrap_or("non standard");
            report.line(format!("- UUID {uuid_text} ({label})"));
            if let Ok(name) = service.ConnectionServiceName() {
                let name = name.to_string();
                let name = if show_address { name } else { mask_macs(&name) };
                report.line(format!("    ConnectionServiceName : {name}"));
            }

            let mut sdp_name = None;
            match sdp_attributes(&service) {
                Ok(attrs) => {
                    for (id, bytes) in &attrs {
                        let shown = &bytes[..bytes.len().min(64)];
                        let more = if bytes.len() > 64 { " …" } else { "" };
                        report.line(format!("    SDP {id:#06X} : {}{more}", to_hex(shown)));
                        let parsed = DataElement::parse(bytes);
                        match (*id, parsed) {
                            (sdp::ATTR_SERVICE_NAME, Ok(el)) => {
                                sdp_name = el.as_text();
                                if let Some(n) = &sdp_name {
                                    report.line(format!("      → nom du service « {n} »"));
                                }
                            }
                            (sdp::ATTR_PROTOCOL_DESCRIPTOR_LIST, Ok(el)) => {
                                if let Some(ch) = sdp::rfcomm_channel(&el) {
                                    report.line(format!("      → canal RFCOMM {ch}"));
                                }
                            }
                            (_, Err(e)) => report.line(format!("      → SDP illisible : {e}")),
                            _ => {}
                        }
                    }
                }
                Err(e) => report.line(format!("    attributs SDP : {}", win_err(&e))),
            }

            let is_gaia = sdp_name
                .as_deref()
                .is_some_and(|n| n.to_lowercase().contains("gaia"));
            let is_wanted = wanted.is_some()
                && uuid
                    .as_ref()
                    .ok()
                    .map(|g| normalize_uuid(&format!("{g:?}")))
                    == wanted;
            if is_wanted || (wanted.is_none() && is_gaia) {
                candidates.push((format!("{uuid_text} sur « {} »", dev.name), service));
            }
        }
        candidates
    }

    fn sdp_attributes(service: &RfcommDeviceService) -> WinResult<Vec<(u32, Vec<u8>)>> {
        let map = service
            .GetSdpRawAttributesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .join()?;
        let mut out = Vec::new();
        for pair in map.First()? {
            out.push((pair.Key()?, buffer_bytes(&pair.Value()?)?));
        }
        out.sort_by_key(|(id, _)| *id);
        Ok(out)
    }

    fn buffer_bytes(buffer: &IBuffer) -> WinResult<Vec<u8>> {
        let mut bytes = vec![0u8; buffer.Length()? as usize];
        DataReader::FromBuffer(buffer)?.ReadBytes(&mut bytes)?;
        Ok(bytes)
    }

    /// Liste les services et caractéristiques GATT, sans lire aucune valeur.
    fn list_gatt(report: &mut Report, device: WinResult<BluetoothLEDevice>) {
        let device = match device {
            Ok(d) => d,
            Err(e) => {
                report.line(format!(
                    "Pas d'appareil LE accessible (normal pour un casque purement Classic) : {}",
                    win_err(&e)
                ));
                return;
            }
        };
        let result = device
            .GetGattServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)
            .and_then(|op| op.join())
            .and_then(|r| Ok((r.Status()?, r.Services()?)));
        let (status, services) = match result {
            Ok(r) => r,
            Err(e) => {
                report.line(format!("ERREUR découverte GATT : {}", win_err(&e)));
                let _ = device.Close();
                return;
            }
        };
        if status != GattCommunicationStatus::Success {
            report.line(format!(
                "GattCommunicationStatus = {} ({})",
                status.0,
                gatt_status_label(status)
            ));
        }
        let count = services.Size().unwrap_or(0);
        report.line(format!("{count} service(s) GATT."));
        for i in 0..count {
            let Ok(service) = services.GetAt(i) else {
                continue;
            };
            let uuid = service.Uuid();
            let label = uuid
                .as_ref()
                .ok()
                .and_then(short_uuid)
                .and_then(gatt_label)
                .unwrap_or("non standard");
            let uuid_text = uuid.map_or_else(|e| win_err(&e), |g| format!("{g:?}"));
            report.line(format!("- service {uuid_text} ({label})"));

            let chars = service
                .GetCharacteristicsWithCacheModeAsync(BluetoothCacheMode::Uncached)
                .and_then(|op| op.join())
                .and_then(|r| Ok((r.Status()?, r.Characteristics()?)));
            match chars {
                Ok((st, list)) => {
                    if st != GattCommunicationStatus::Success {
                        report.line(format!("    caractéristiques : {}", gatt_status_label(st)));
                    }
                    for j in 0..list.Size().unwrap_or(0) {
                        let Ok(c) = list.GetAt(j) else { continue };
                        let cu = c.Uuid().map_or_else(|e| win_err(&e), |g| format!("{g:?}"));
                        let props = c
                            .CharacteristicProperties()
                            .map(props_label)
                            .unwrap_or_default();
                        report.line(format!("    - {cu} [{props}]"));
                    }
                }
                Err(e) => report.line(format!("    caractéristiques : {}", win_err(&e))),
            }
            let _ = service.Close();
        }
        let _ = device.Close();
    }

    fn talk_gaia(report: &mut Report, service: &RfcommDeviceService, args: &Args) {
        let transport = match RfcommTransport::connect(service) {
            Ok(t) => t,
            Err(e) => {
                report.line(format!("ÉCHEC de connexion : {}", transport_err(&e)));
                return;
            }
        };
        report.line("Connecté.");
        let mut client = GaiaClient::new(transport);
        client.record_raw_log(true);
        let mut session = Session {
            client,
            timeout: Duration::from_millis(args.timeout_ms),
            closed: false,
        };

        let mut requests = vec![("batterie", get::BATTERY)];
        if args.all_gets || args.deep {
            requests.extend_from_slice(EXTRA_GETS);
        }
        if args.deep {
            requests.extend_from_slice(DEEP_GETS);
        }
        for (label, command) in requests {
            session.ask(report, label, Frame::sennheiser_request(command), false);
        }
        if args.deep {
            deep_inventory(report, &mut session, args.show_address);
        }
    }

    /// Requêtes à paramètre ou en plusieurs temps du mode `--deep`.
    fn deep_inventory(report: &mut Report, s: &mut Session, show_sensitive: bool) {
        let sen = |cmd: CommandId, payload: Vec<u8>| Frame::new(VENDOR_SENNHEISER, cmd, payload);

        // Minuteur 0 = extinction automatique.
        s.ask(report, "extinction auto", sen(get::TIMER, vec![0]), false);

        // Égaliseur : configuration, puis chaque bande annoncée.
        if let Some(cfg) = s.ask(
            report,
            "configuration égaliseur",
            sen(get::EQ_CONFIG, vec![]),
            false,
        ) {
            let bands = cfg.payload.first().copied().unwrap_or(0).min(MAX_ITEMS);
            for band in 0..bands {
                let label = format!("égaliseur bande {band}");
                s.ask(report, &label, sen(get::EQ_BAND, vec![band]), false);
            }
        }

        // Appareils appairés au casque (lecture seule : jamais connecter/supprimer).
        if let Some(size) = s.ask(
            report,
            "appareils appairés",
            sen(get::PAIRED_LIST_SIZE, vec![]),
            false,
        ) {
            let count = size.payload.last().copied().unwrap_or(0).min(MAX_ITEMS);
            for idx in 0..count {
                let label = format!("appareil appairé n°{idx}");
                s.ask(
                    report,
                    &label,
                    sen(get::PAIRED_DEVICE_INFO, vec![idx]),
                    false,
                );
            }
        }

        // Core Qualcomm.
        let qc = |cmd: CommandId| Frame::new(VENDOR_QUALCOMM, cmd, Vec::new());
        s.ask(
            report,
            "features GAIA (Qualcomm)",
            qc(core_get::SUPPORTED_FEATURES),
            false,
        );
        // Le numéro de série est masqué sauf avec --show-address.
        s.ask(
            report,
            "numéro de série (Qualcomm)",
            qc(core_get::SERIAL_NUMBER),
            !show_sensitive,
        );
    }

    struct Session {
        client: GaiaClient<RfcommTransport>,
        timeout: Duration,
        closed: bool,
    }

    impl Session {
        /// Envoie une requête, écrit TX/RX et l'interprétation dans le rapport.
        /// `sensitive` : masque les octets reçus (numéro de série).
        fn ask(
            &mut self,
            report: &mut Report,
            label: &str,
            request: Frame,
            sensitive: bool,
        ) -> Option<Frame> {
            if self.closed {
                return None;
            }
            report.line("");
            report.line(format!(
                "GET {label} ({:04X}:{}{})",
                request.vendor,
                request.command,
                if request.payload.is_empty() {
                    String::new()
                } else {
                    format!(" {}", to_hex(&request.payload))
                }
            ));
            let result = self.client.request(&request, self.timeout);
            for event in self.client.take_raw_log() {
                match event {
                    RawEvent::Sent(b) => report.line(format!("  TX {}", to_hex(&b))),
                    RawEvent::Received(_) if sensitive => {
                        report.line("  RX (masqué, relancer avec --show-address pour voir)")
                    }
                    RawEvent::Received(b) => report.line(format!("  RX {}", to_hex(&b))),
                }
            }
            for frame in self.client.take_unsolicited() {
                report.line(format!("  autre trame : {}", describe(&frame)));
            }
            let answer = match result {
                Ok(frame) => {
                    if sensitive {
                        report.line(format!("  OK : {} octet(s) reçus", frame.payload.len()));
                    } else {
                        report.line(format!("  OK : {}", describe(&frame)));
                    }
                    Some(frame)
                }
                Err(ClientError::Transport(e)) => {
                    report.line(format!("  ÉCHEC : {}", transport_err(&e)));
                    if matches!(e, TransportError::Closed) {
                        report.line("  Connexion fermée : arrêt des requêtes.");
                        self.closed = true;
                    }
                    None
                }
                Err(e) => {
                    report.line(format!("  ÉCHEC : {e}"));
                    None
                }
            };
            if self.client.is_desynchronized() && !self.closed {
                // Une réponse tardive serait ambiguë : on n'envoie plus rien.
                report.line("  Délai dépassé : arrêt des requêtes (relancer le probe).");
                self.closed = true;
            }
            answer
        }
    }

    // ---------- Formatage et diagnostic ----------

    fn win_err(e: &windows::core::Error) -> String {
        let hr = e.code().0 as u32;
        let mut text = format!("HRESULT {hr:#010X} — {}", e.message().trim());
        if let Some(h) = hresult_hint(hr) {
            text.push_str(&format!(" (piste : {h})"));
        }
        text
    }

    fn transport_err(e: &TransportError) -> String {
        match e {
            TransportError::Os { hresult, .. } => match hresult_hint(*hresult) {
                Some(h) => format!("{e} (piste : {h})"),
                None => e.to_string(),
            },
            TransportError::Timeout(_) => format!(
                "{e} (piste : le casque ne parle peut-être pas GAIA sur ce service, \
                 ou le protocole du M5 diffère)"
            ),
            _ => e.to_string(),
        }
    }

    fn hresult_hint(hr: u32) -> Option<&'static str> {
        Some(match hr {
            0x8007_0490 => "élément introuvable : casque éteint, hors de portée ou service absent",
            0x8007_0005 => "accès refusé : vérifiez Paramètres > Confidentialité > Bluetooth",
            0x8007_048F => "appareil non connecté : allumez le casque et connectez-le",
            0x8007_0079 => "délai de connexion dépassé : rapprochez le casque et réessayez",
            0x8007_274C => "délai réseau dépassé : le casque n'a pas répondu",
            0x8007_274D => "connexion refusée : service occupé (téléphone avec Smart Control ?)",
            0x8007_2740 => "adresse déjà utilisée : une autre app tient déjà ce canal",
            _ => return None,
        })
    }

    fn bluetooth_error_label(e: BluetoothError) -> &'static str {
        match e {
            BluetoothError::RadioNotAvailable => "radio Bluetooth indisponible",
            BluetoothError::ResourceInUse => "ressource déjà utilisée",
            BluetoothError::DeviceNotConnected => "appareil non connecté",
            BluetoothError::DisabledByPolicy => "désactivé par une stratégie",
            BluetoothError::NotSupported => "non pris en charge",
            BluetoothError::DisabledByUser => "désactivé par l'utilisateur",
            BluetoothError::ConsentRequired => "consentement requis",
            BluetoothError::TransportNotSupported => "transport non pris en charge",
            _ => "autre erreur",
        }
    }

    fn gatt_status_label(s: GattCommunicationStatus) -> &'static str {
        match s {
            GattCommunicationStatus::Success => "succès",
            GattCommunicationStatus::Unreachable => "injoignable",
            GattCommunicationStatus::ProtocolError => "erreur de protocole",
            GattCommunicationStatus::AccessDenied => "accès refusé",
            _ => "inconnu",
        }
    }

    fn props_label(p: GattCharacteristicProperties) -> String {
        const NAMES: &[(GattCharacteristicProperties, &str)] = &[
            (GattCharacteristicProperties::Read, "read"),
            (GattCharacteristicProperties::Write, "write"),
            (
                GattCharacteristicProperties::WriteWithoutResponse,
                "write-no-resp",
            ),
            (GattCharacteristicProperties::Notify, "notify"),
            (GattCharacteristicProperties::Indicate, "indicate"),
        ];
        NAMES
            .iter()
            .filter(|(flag, _)| p.0 & flag.0 != 0)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Renvoie l'UUID court si `g` est dérivé de l'UUID de base Bluetooth.
    fn short_uuid(g: &GUID) -> Option<u32> {
        (g.data2 == BASE_UUID_TAIL.0 && g.data3 == BASE_UUID_TAIL.1 && g.data4 == BASE_UUID_TAIL.2)
            .then_some(g.data1)
    }

    /// Profils Classic courants (Assigned Numbers du Bluetooth SIG).
    fn rfcomm_label(short: u32) -> Option<&'static str> {
        Some(match short {
            0x1101 => "Serial Port",
            0x1108 => "Headset",
            0x110A => "Audio Source",
            0x110B => "Audio Sink",
            0x110C => "A/V Remote Control Target",
            0x110E => "A/V Remote Control",
            0x111E => "Handsfree",
            0x111F => "Handsfree Audio Gateway",
            0x1200 => "PnP Information",
            _ => return None,
        })
    }

    /// Services GATT courants (Assigned Numbers du Bluetooth SIG).
    fn gatt_label(short: u32) -> Option<&'static str> {
        Some(match short {
            0x1800 => "Generic Access",
            0x1801 => "Generic Attribute",
            0x180A => "Device Information",
            0x180F => "Battery Service",
            0x1844 => "Volume Control (LE Audio)",
            0x184E => "Audio Stream Control (LE Audio)",
            0x1850 => "Published Audio Capabilities (LE Audio)",
            _ => return None,
        })
    }

    fn normalize_uuid(s: &str) -> String {
        s.trim_matches(|c| c == '{' || c == '}').to_uppercase()
    }

    /// Masque les 4 premiers octets de chaque adresse « aa:bb:cc:dd:ee:ff »
    /// trouvée dans le texte (Windows les inclut dans les identifiants).
    fn mask_macs(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < bytes.len() {
            let window = bytes.get(i..i + 17);
            let is_mac = window.is_some_and(|w| {
                w.iter().enumerate().all(|(j, b)| {
                    if j % 3 == 2 {
                        *b == b':'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                })
            });
            if is_mac {
                out.push_str("XX:XX:XX:XX:");
                out.push_str(&text[i + 12..i + 17]);
                i += 17;
            } else {
                // Avance d'un caractère UTF-8 complet.
                let ch = text[i..].chars().next().unwrap_or(' ');
                out.push(ch);
                i += ch.len_utf8().max(1);
            }
        }
        out
    }

    fn fmt_address(addr: u64, show: bool) -> String {
        let bytes = addr.to_be_bytes(); // 8 octets, l'adresse occupe les 6 derniers.
        let parts: Vec<String> = bytes[2..]
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if show || i >= 4 {
                    format!("{b:02X}")
                } else {
                    "XX".into()
                }
            })
            .collect();
        parts.join(":")
    }
}
