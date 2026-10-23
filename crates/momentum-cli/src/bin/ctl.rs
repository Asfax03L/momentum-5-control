//! `ctl` : réglages du casque en ligne de commande.
//!
//! Chaque écriture reproduit la séquence de Smart Control Plus, puis l'état
//! est relu : la commande échoue si le casque n'a pas appliqué le réglage.

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use momentum_core::json::{self, Readback};
use std::io::Write;
use std::process::ExitCode;

#[cfg(test)]
#[path = "../ctl_tests.rs"]
mod tests;

#[derive(Parser, Debug)]
#[command(
    name = "ctl",
    version,
    about = "Réglages du casque Sennheiser MOMENTUM 5 (projet non officiel)"
)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Écrit un seul objet JSON versionné ; les messages humains vont sur stderr.
    #[arg(long, global = true)]
    json: bool,

    /// Filtre sur le nom de l'appareil appairé.
    #[arg(long, global = true)]
    device: Option<String>,

    /// Délai d'attente de chaque réponse, en millisecondes.
    #[arg(long, global = true, default_value_t = 3000)]
    timeout_ms: u64,

    /// Journal détaillé (trames brutes) sur la sortie d'erreur.
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Linux : canal RFCOMM à ouvrir directement, sans recherche SDP
    /// (le MOMENTUM 5 annonce GAIA sur les canaux 11 et 12).
    #[cfg(target_os = "linux")]
    #[arg(long, global = true)]
    canal: Option<u8>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Exporte ou importe uniquement les réglages locaux, sans service ni casque.
    Reglages {
        #[command(subcommand)]
        action: SettingsAction,
    },
    /// Écrit la complétion du shell sur la sortie standard, sans casque.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Écrit la page de manuel ctl.1 sur la sortie standard, sans casque.
    Man,
    /// Affiche l'état du casque. Passe par le service s'il tourne.
    Etat,
    /// Liste les scènes du fichier scenes.txt.
    Scenes,
    /// Applique une scène (réglages un par un, arrêt à la première erreur).
    /// Passe par le service s'il tourne.
    Scene { nom: String },
    /// Enregistre l'état déjà connu du service comme scène, sans écrire au casque.
    SceneEnregistrer {
        nom: String,
        /// Remplace la scène et son préréglage si le nom existe déjà.
        #[arg(long)]
        remplacer: bool,
    },
    /// Change le mode de contrôle du bruit.
    Mode { mode: Mode },
    /// Règle le curseur ANC ↔ transparence (0 = ANC max, 50 = neutre, 100 = transparence max).
    Niveau {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        niveau: u8,
    },
    /// Active ou désactive l'anti-vent.
    AntiVent { etat: OnOff },
    /// Active ou désactive l'audio spatial.
    Spatial { etat: OnOff },
    /// Suivi de tête de Dolby Atmos (exige l'audio spatial activé).
    SuiviTete { etat: OnOff },
    /// Multipoint (deux appareils connectés à la fois). Le couper ne garde
    /// qu'un appareil : ce PC, ou le téléphone.
    Multipoint { etat: OnOff },
    /// Affiche l'égaliseur (gains par bande, Amplification des basses).
    Eq,
    /// Règle le gain d'une bande de l'égaliseur, en dB (pas de 0,1 dB).
    Bande {
        /// Numéro de bande, de 1 (90 Hz) à 8 (16 kHz) sur le M5.
        #[arg(value_parser = clap::value_parser!(u8).range(1..=16))]
        numero: u8,
        /// Gain en dB, par exemple -3.5 ou 2.
        #[arg(allow_negative_numbers = true)]
        gain: f32,
    },
    /// Remet toutes les bandes de l'égaliseur à 0 dB.
    Plat,
    /// Active ou désactive l'Amplification des basses.
    Basses { etat: OnOff },
    /// Active Speech Clarity (voix mises en avant) ou revient à l'égaliseur.
    Voix { etat: OnOff },
    /// Sélectionne le profil de personnalisation du son déjà enregistré
    /// (coupe Dolby Atmos s'il est actif, comme l'app), ou revient à
    /// l'égaliseur.
    Profil { etat: OnOff },
    /// Active ou désactive les commandes tactiles du casque.
    Tactile { etat: OnOff },
    /// Active ou désactive la protection de la batterie (charge limitée).
    ProtectionBatterie { etat: OnOff },
    /// Active ou désactive Smart Pause (pause quand on retire le casque).
    SmartPause { etat: OnOff },
    /// Active ou désactive la réponse automatique aux appels.
    ReponseAuto { etat: OnOff },
    /// Active ou désactive les appels confortables.
    AppelsConfort { etat: OnOff },
    /// Extinction automatique : jamais, 15, 30 ou 60 minutes.
    Extinction { duree: AutoOff },
    /// Invites : tonalités seules, ou tonalités et voix.
    Invites { mode: Prompts },
    /// Affiche le nom du casque, ou le change (ASCII sans accent, 32
    /// caractères au plus). Windows peut garder l'ancien nom jusqu'à la
    /// prochaine connexion.
    Nom { nouveau: Option<String> },
    /// Liste les appareils appairés au casque et leur état de connexion.
    Connexions,
    /// Connecte l'appareil n° <n> de `connexions`.
    Connecter { n: usize },
    /// Déconnecte l'appareil n° <n> de `connexions` (jamais ce PC).
    Deconnecter { n: usize },
    /// Volume des invites, en % comme dans l'app (77 % = valeur 0).
    VolumeInvites {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
        pourcent: u8,
    },
    /// Sauvegarde le profil de personnalisation du son, octets bruts, dans
    /// un fichier JSON (lecture seule ; aucune restauration). Le fichier
    /// contient des données auditives personnelles : ne pas le publier.
    ExporterProfil {
        /// Fichier à créer (jamais écrasé s'il existe déjà).
        fichier: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum SettingsAction {
    /// Écrit une sauvegarde JSON neuve (aucun écrasement).
    Exporter {
        fichier: std::path::PathBuf,
        /// Inclut explicitement l'historique local de batterie.
        #[arg(long)]
        avec_batterie: bool,
    },
    /// Valide tous les textes puis importe, avec sauvegardes .ancien.
    Importer {
        fichier: std::path::PathBuf,
        /// Affiche seulement le résumé, sans écrire de fichier.
        #[arg(long)]
        simulation: bool,
        /// Autorise aussi l'import de batterie.csv, s'il est présent.
        #[arg(long)]
        avec_batterie: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AutoOff {
    Jamais,
    #[value(name = "15")]
    Min15,
    #[value(name = "30")]
    Min30,
    #[value(name = "60")]
    Min60,
}

impl AutoOff {
    fn seconds(self) -> u16 {
        match self {
            AutoOff::Jamais => 0,
            AutoOff::Min15 => 15 * 60,
            AutoOff::Min30 => 30 * 60,
            AutoOff::Min60 => 60 * 60,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Prompts {
    Tonalites,
    Voix,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Arret,
    Adaptatif,
    Personnalise,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OnOff {
    On,
    Off,
}

impl OnOff {
    fn is_on(self) -> bool {
        matches!(self, OnOff::On)
    }
}

/// Collecte uniquement les valeurs déjà lues, sans changer les échanges.
struct Output {
    json: bool,
    readback: Readback,
    value: Option<String>,
    details: Option<String>,
    outcome: Option<String>,
    exit_code: u8,
}

impl Output {
    fn new(json: bool) -> Self {
        Self {
            json,
            readback: Readback::default(),
            value: None,
            details: None,
            outcome: None,
            exit_code: 0,
        }
    }

    fn message(&self, text: std::fmt::Arguments<'_>) {
        if self.json {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }

    fn render(&self, command: &Command, error: Option<&str>) -> String {
        if error.is_none() {
            if let Some(value) = &self.value {
                return value.clone();
            }
            match command {
                // Garder ce sérialiseur et l'ordre historique de ses champs.
                Command::Etat => return json::state_json(self.readback.state.as_ref()),
                Command::Eq => return json::equalizer_json(self.readback.equalizer.as_ref()),
                Command::Connexions => {
                    return json::connections_json(self.readback.connections.as_deref());
                }
                Command::Nom { nouveau: None } => {
                    return json::name_json(self.readback.name.as_deref());
                }
                _ => {}
            }
        }
        json::result_json(
            &self.readback,
            error,
            self.outcome.as_deref(),
            self.details.as_deref(),
        )
    }
}

fn print_json(text: &str) -> std::io::Result<()> {
    writeln!(std::io::stdout().lock(), "{text}")
}

fn main() -> ExitCode {
    let argv: Vec<_> = std::env::args_os().collect();
    let json = argv
        .iter()
        .take_while(|a| *a != "--")
        .any(|a| a == "--json");
    let args = match Args::try_parse_from(argv) {
        Ok(args) => args,
        Err(error) if json => {
            let text = if error.use_stderr() {
                json::result_json(&Readback::default(), Some(&error.to_string()), None, None)
            } else {
                json::documentation_json("aide", &error.to_string())
            };
            let code = error.exit_code() as u8;
            if let Err(e) = print_json(&text) {
                eprintln!("Erreur de sortie : {e}");
                return ExitCode::FAILURE;
            }
            return ExitCode::from(code);
        }
        Err(error) => error.exit(),
    };
    let mut output = Output::new(args.json);
    let result = execute(&args, &mut output);
    let error = result.err().map(|e| format!("{e:#}"));
    if let Some(error) = &error {
        eprintln!("Erreur : {error}");
        if output.exit_code == 0 {
            output.exit_code = 1;
        }
    }
    if args.json
        && let Err(e) = print_json(&output.render(&args.command, error.as_deref()))
    {
        eprintln!("Erreur de sortie : {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::from(output.exit_code)
}

fn execute(args: &Args, output: &mut Output) -> anyhow::Result<()> {
    // La documentation ne doit ni démarrer le service ni ouvrir Bluetooth.
    let mut documentation = Vec::new();
    if write_documentation(&args.command, &mut documentation)? {
        if args.json {
            let kind = if matches!(args.command, Command::Man) {
                "manuel"
            } else {
                "completion"
            };
            output.value = Some(json::documentation_json(
                kind,
                &String::from_utf8(documentation)?,
            ));
        } else {
            std::io::stdout().lock().write_all(&documentation)?;
        }
        return Ok(());
    }
    let level = if args.verbose { "trace" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(format!("momentum_core={level},ctl={level}"))
            }),
        )
        .with_writer(std::io::stderr)
        .init();

    #[cfg(any(windows, target_os = "linux"))]
    return app::run(args, output);
    #[cfg(not(any(windows, target_os = "linux")))]
    anyhow::bail!("ctl ne fonctionne que sous Windows et Linux");
}

fn write_documentation(
    command: &Command,
    output: &mut dyn std::io::Write,
) -> std::io::Result<bool> {
    match command {
        Command::Completions { shell } => {
            // Le générateur ne renvoie pas les erreurs d'écriture : passer
            // par un tampon pour propager celles de la sortie standard.
            let mut buffer = Vec::new();
            clap_complete::generate(*shell, &mut Args::command(), "ctl", &mut buffer);
            output.write_all(&buffer)?;
        }
        Command::Man => clap_mangen::Man::new(Args::command()).render(output)?,
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(any(windows, target_os = "linux"))]
mod app {
    use std::time::Duration;

    use anyhow::{Context, bail};
    use momentum_core::device::{
        AudioMode, EqState, Headset, HeadsetState, NoiseModeRequest, NoiseState, PromptMode,
    };
    use momentum_core::json::ConnectionSummary;
    use momentum_core::paths;
    use momentum_core::protocol::messages::M5NoiseMode;
    use momentum_core::scenes;
    use momentum_core::transport::Transport;
    #[cfg(target_os = "linux")]
    use momentum_core::transport::bluez_rfcomm::{
        BluezTransport as PlatformTransport, list_gaia_devices,
    };
    use momentum_core::transport::choice::{Candidate, Choice, choose};
    #[cfg(windows)]
    use momentum_core::transport::windows_rfcomm::{
        RfcommTransport as PlatformTransport, list_gaia_services,
    };
    use momentum_core::ui::{band_label, describe_noise, describe_state, format_db};
    #[cfg(windows)]
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

    use super::{Args, Command, Mode, Output, Prompts};

    /// Délai d'attente de la réponse du casque à une commande passée par le
    /// service.
    #[cfg(any(windows, target_os = "linux"))]
    const DAEMON_REPLY: Duration = Duration::from_secs(20);
    /// Attente du premier état quand le service vient de démarrer.
    #[cfg(any(windows, target_os = "linux"))]
    const DAEMON_FIRST_STATE: Duration = Duration::from_secs(15);
    /// Une scène enchaîne plusieurs écritures et relectures.
    #[cfg(any(windows, target_os = "linux"))]
    const DAEMON_SCENE: Duration = Duration::from_secs(90);

    /// Passe la commande au service `momentum-daemon` s'il tourne ; `false` :
    /// pas de service (connexion directe), ou commande sans casque (scènes).
    #[cfg(any(windows, target_os = "linux"))]
    fn through_daemon(args: &Args, output: &mut Output) -> anyhow::Result<bool> {
        let command = &args.command;
        use momentum_client::tracker;
        use momentum_client::{Client, ClientError, Command as Api, Event};

        let client = match Client::connect() {
            Ok(c) => c,
            Err(ClientError::NoService) => return Ok(false),
            Err(e) => bail!("service momentum-daemon : {e}"),
        };
        let mut snapshot = client.snapshot()?;
        // Recherche ratée signalée par le service : inutile d'attendre.
        let mut not_found = None;
        // Service tout juste lancé (activation D-Bus, démarrage) : l'état arrive dans
        // quelques secondes, le temps de joindre le casque ; sans casque, la
        // recherche en cours se termine par `NotFound`.
        if snapshot.state.is_none() && !matches!(command, Command::Scenes) {
            let events = client.events()?;
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                for event in events {
                    let end = match event {
                        Event::State(_) => None,
                        Event::NotFound(reason) => Some(reason),
                        _ => continue,
                    };
                    let _ = tx.send(end);
                    break;
                }
            });
            if snapshot.device_name.is_none() {
                eprintln!("(recherche du casque par le service…)");
            }
            not_found = rx.recv_timeout(DAEMON_FIRST_STATE).ok().flatten();
            snapshot = client.snapshot()?;
        }
        let state = || {
            snapshot.state.ok_or_else(|| {
                if let Some(reason) = &not_found {
                    anyhow::anyhow!("casque introuvable : {reason} (le service réessaie)")
                } else if snapshot.device_name.is_some() {
                    anyhow::anyhow!("casque en cours de connexion, réessayer dans un instant")
                } else {
                    anyhow::anyhow!(
                        "casque introuvable : le service le cherche (allumé et connecté ?)"
                    )
                }
            })
        };
        let api = match command {
            Command::SceneEnregistrer { .. } => {
                bail!("enregistrer une scène exige l'état connu du service")
            }
            Command::Reglages { .. } => bail!("les réglages locaux ne passent pas par le service"),
            Command::Scenes | Command::Completions { .. } | Command::Man => return Ok(false),
            Command::Etat => {
                eprintln!("(par le service momentum-daemon)");
                output.readback.state = snapshot.state;
                if !args.json {
                    print_state(output, &state()?);
                }
                return Ok(true);
            }
            Command::Eq => {
                eprintln!("(par le service momentum-daemon)");
                if args.json {
                    output.readback.equalizer = snapshot.equalizer;
                    return Ok(true);
                }
                state()?;
                let eq = snapshot
                    .equalizer
                    .ok_or_else(|| anyhow::anyhow!("égaliseur pas encore lu"))?;
                print_eq(output, &eq);
                return Ok(true);
            }
            Command::Connexions => {
                eprintln!("(par le service momentum-daemon)");
                if args.json {
                    output.readback.connections = connection_summaries(&snapshot);
                    return Ok(true);
                }
                for (i, d) in snapshot.connections.iter().enumerate() {
                    output.message(format_args!(
                        "{}. {}{} — {}",
                        i + 1,
                        d.name,
                        if d.this_device { " (ce PC)" } else { "" },
                        if d.connected {
                            "connecté"
                        } else {
                            "non connecté"
                        }
                    ));
                }
                return Ok(true);
            }
            Command::Scene { nom } => Api::ApplyScene(nom.clone()),
            Command::Mode { mode } => Api::SetMode(match mode {
                Mode::Arret => NoiseModeRequest::Off,
                Mode::Adaptatif => NoiseModeRequest::Adaptive,
                Mode::Personnalise => NoiseModeRequest::Custom,
            }),
            Command::Niveau { niveau } => Api::SetLevel(*niveau),
            Command::AntiVent { etat } => Api::SetAntiWind(etat.is_on()),
            Command::Spatial { etat } => Api::SetSpatial(etat.is_on()),
            Command::SuiviTete { etat } => Api::SetHeadTracking(etat.is_on()),
            Command::Multipoint { etat } => Api::SetMultipoint(etat.is_on()),
            Command::Bande { numero, gain } => {
                let tenths = (gain * 10.0).round();
                if !(f32::from(i8::MIN)..=f32::from(i8::MAX)).contains(&tenths) {
                    bail!("gain {gain} dB hors limites");
                }
                Api::SetEqBand(numero - 1, tenths as i8)
            }
            Command::Plat => Api::SetEqFlat,
            Command::Basses { etat } => Api::SetBassBoost(etat.is_on()),
            Command::Voix { etat } => Api::SetSpeechClarity(etat.is_on()),
            Command::Profil { etat } => Api::SetSoundProfile(etat.is_on()),
            Command::Tactile { etat } => Api::SetTouchControls(etat.is_on()),
            Command::ProtectionBatterie { etat } => Api::SetBatteryProtection(etat.is_on()),
            Command::SmartPause { etat } => Api::SetSmartPause(etat.is_on()),
            Command::ReponseAuto { etat } => Api::SetAutoAnswer(etat.is_on()),
            Command::AppelsConfort { etat } => Api::SetComfortCall(etat.is_on()),
            Command::Extinction { duree } => Api::SetAutoOff(duree.seconds()),
            Command::Invites { mode } => Api::SetPromptMode(match mode {
                Prompts::Tonalites => PromptMode::TonesOnly,
                Prompts::Voix => PromptMode::TonesAndVoice,
            }),
            Command::VolumeInvites { pourcent } => {
                Api::SetPromptVolume(momentum_core::device::prompt_volume_from_percent(*pourcent))
            }
            Command::Connecter { n } | Command::Deconnecter { n } => {
                let index = n
                    .checked_sub(1)
                    .filter(|&i| i < snapshot.connections.len())
                    .ok_or_else(|| {
                        anyhow::anyhow!("pas d'appareil n° {n} (voir `ctl connexions`)")
                    })?;
                let row = &snapshot.connections[index];
                Api::ToggleConnection {
                    index,
                    name: row.name.clone(),
                    connect: matches!(command, Command::Connecter { .. }),
                    device: Some(row.id.clone()).filter(|id| !id.is_empty()),
                }
            }
            Command::Nom { nouveau: None } => {
                eprintln!("(par le service momentum-daemon)");
                let name = snapshot.info.as_ref().and_then(|i| i.name.clone());
                output.readback.name = name.clone();
                output.message(format_args!("{}", name.as_deref().unwrap_or("nom non lu")));
                return Ok(true);
            }
            Command::Nom {
                nouveau: Some(nouveau),
            } => Api::SetName(nouveau.clone()),
            Command::ExporterProfil { fichier } => {
                let absolute = std::path::absolute(fichier)
                    .with_context(|| format!("chemin {}", fichier.display()))?;
                Api::ExportProfile(absolute)
            }
        };
        // Aucun casque trouvé : rien n'est envoyé (code 1, comme un refus).
        if let (None, Some(reason)) = (&snapshot.state, &not_found) {
            bail!("casque introuvable : {reason} (rien n'a été envoyé)");
        }
        eprintln!("(par le service momentum-daemon)");
        let is_scene = matches!(api, Api::ApplyScene(_));
        // Abonnement avant l'envoi : la réponse ne peut pas être manquée.
        let events = client.events()?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for event in events {
                if tx.send(event).is_err() {
                    break;
                }
            }
        });
        // Demande suivie : seul l'événement `Done` portant son numéro dit ce
        // qu'il en est (F01 : un autre événement n'est pas une confirmation).
        let is_eq = matches!(
            api,
            Api::SetEqBand(..) | Api::SetEqFlat | Api::SetEqGains(_) | Api::SetBassBoost(_)
        );
        let id = client.submit(&api, snapshot.session).map_err(|e| match e {
            ClientError::Service(reason) => anyhow::anyhow!("refusé par le service : {reason}"),
            other => anyhow::anyhow!(other),
        })?;
        let timeout = if is_scene { DAEMON_SCENE } else { DAEMON_REPLY };
        // Codes de sortie : 0 confirmé, 1 refusé ou annulé, 2 non appliqué,
        // 3 résultat inconnu (voir `tracker::exit_code`).
        let Some((outcome, details)) = tracker::wait_for(&rx, id, timeout) else {
            output.exit_code = tracker::exit_code(None) as u8;
            bail!("pas de résultat dans le délai : résultat inconnu (relire avec `ctl etat`)");
        };
        if args.json {
            output.details = details;
            output.outcome = Some(outcome.describe());
            output.exit_code = tracker::exit_code(Some(&outcome)) as u8;
            // Le worker relit déjà ces valeurs. Une scène annonce Done
            // avant sa relecture : attendre les événements qui la suivent.
            if is_scene
                && matches!(
                    outcome,
                    momentum_client::Outcome::Applied | momentum_client::Outcome::NotApplied(_)
                )
            {
                scene_readback(&rx, output)?;
            } else if matches!(
                outcome,
                momentum_client::Outcome::Applied
                    | momentum_client::Outcome::AlreadySatisfied
                    | momentum_client::Outcome::NotApplied(_)
            ) && !matches!(api, Api::ExportProfile(_))
            {
                daemon_readback(command, &client.snapshot()?, output);
            }
            if !outcome.is_success() {
                bail!("{}", outcome.describe());
            }
            return Ok(true);
        }
        if let Some(details) = &details {
            if is_scene {
                output.message(format_args!("{details}"));
            } else {
                output.message(format_args!("Fichier : {details}"));
            }
        }
        if !outcome.is_success() {
            output.exit_code = tracker::exit_code(Some(&outcome)) as u8;
            bail!("{}", outcome.describe());
        }
        if !is_scene && !matches!(api, Api::ExportProfile(_)) {
            let now = client.snapshot()?;
            match (is_eq, now.equalizer, now.state) {
                (true, Some(eq), _) => print_eq(output, &eq),
                (false, _, Some(state)) => print_state(output, &state),
                _ => {}
            }
        }
        output.message(format_args!("Demande {} ({}).", id, outcome.describe()));
        Ok(true)
    }

    fn list_scenes(output: &mut Output) {
        let (list, errors) = paths::load_scenes();
        output.value = Some(momentum_core::json::scenes_json(
            &list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            &errors,
        ));
        for scene in list {
            output.message(format_args!("{}", scene.name));
        }
        for error in errors {
            output.message(format_args!("# {error}"));
        }
    }

    fn connection_summaries(
        snapshot: &momentum_client::Snapshot,
    ) -> Option<Vec<ConnectionSummary>> {
        // L'API ne distingue pas une liste vide d'une liste non encore lue.
        // Ne pas présenter l'absence d'information comme une lecture vide.
        (!snapshot.connections.is_empty()).then(|| {
            snapshot
                .connections
                .iter()
                .map(|c| ConnectionSummary {
                    name: c.name.clone(),
                    connected: c.connected,
                    this_device: c.this_device,
                })
                .collect()
        })
    }

    fn daemon_readback(
        command: &Command,
        snapshot: &momentum_client::Snapshot,
        output: &mut Output,
    ) {
        match command {
            Command::Bande { .. } | Command::Plat | Command::Basses { .. } => {
                output.readback.equalizer = snapshot.equalizer.clone();
            }
            Command::Connecter { .. } | Command::Deconnecter { .. } => {
                output.readback.connections = connection_summaries(snapshot);
            }
            _ => {
                output.readback.state = snapshot.state;
                if let Some(info) = &snapshot.info {
                    match command {
                        Command::Nom { .. } => output.readback.name = info.name.clone(),
                        Command::VolumeInvites { .. } => {
                            output.readback.prompt_volume = info.prompt_volume
                        }
                        Command::Multipoint { .. } => output.readback.multipoint = info.multipoint,
                        _ => {}
                    }
                }
            }
        }
    }

    fn scene_readback(
        events: &std::sync::mpsc::Receiver<momentum_client::Event>,
        output: &mut Output,
    ) -> anyhow::Result<()> {
        use momentum_client::Event;
        let deadline = std::time::Instant::now() + DAEMON_REPLY;
        loop {
            let event =
                events.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
            match event {
                Ok(Event::Equalizer(eq)) => output.readback.equalizer = Some(eq),
                Ok(Event::State(state)) => {
                    output.readback.state = Some(state);
                    return Ok(());
                }
                Ok(
                    Event::Lost(_)
                    | Event::NotFound(_)
                    | Event::Searching
                    | Event::Connected { .. },
                )
                | Err(_) => {
                    output.exit_code = 3;
                    bail!(
                        "résultat de la scène reçu, mais relecture indisponible (relire avec `ctl etat`)"
                    );
                }
                _ => {}
            }
        }
    }

    /// Casque parmi les appareils GAIA connectés : celui de `--device`
    /// (nom), sinon la même règle que le service (casque mémorisé, seul
    /// candidat, seul nom distinctif ; jamais au hasard).
    fn pick(args: &Args, candidates: &[Candidate]) -> anyhow::Result<usize> {
        if let Some(filter) = &args.device {
            let filter = filter.to_lowercase();
            return candidates
                .iter()
                .position(|c| c.name.to_lowercase().contains(&filter))
                .with_context(|| {
                    format!("aucun casque connecté dont le nom contient « {filter} »")
                });
        }
        match choose(
            candidates,
            paths::load_headset_fingerprint().as_deref(),
            &["momentum", "sennheiser"],
        ) {
            Choice::One(i) => Ok(i),
            Choice::None => bail!(
                "aucun casque appairé, connecté et annonçant le service GAIA (allumé et connecté ?)"
            ),
            Choice::Ambiguous(names) => bail!(
                "plusieurs appareils GAIA connectés ({}) : préciser avec --device",
                names.join(", ")
            ),
        }
    }

    /// Trouve le casque et ouvre le canal GAIA (WinRT).
    #[cfg(windows)]
    fn connect(args: &Args) -> anyhow::Result<(PlatformTransport, String)> {
        // SAFETY: appel unique au démarrage, avant tout objet WinRT.
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let mut found = list_gaia_services().context("recherche du service GAIA")?;
        let candidates: Vec<Candidate> = found
            .iter()
            .map(|s| Candidate {
                key: &s.device_id,
                name: &s.device_name,
            })
            .collect();
        let chosen = found.swap_remove(pick(args, &candidates)?);
        let transport =
            PlatformTransport::connect(&chosen.service).context("connexion RFCOMM au casque")?;
        Ok((transport, chosen.device_name))
    }

    /// Trouve le casque et ouvre le canal GAIA (BlueZ).
    #[cfg(target_os = "linux")]
    fn connect(args: &Args) -> anyhow::Result<(PlatformTransport, String)> {
        let mut found = list_gaia_devices().context("recherche du casque dans BlueZ")?;
        let keys: Vec<String> = found.iter().map(|d| d.address.to_string()).collect();
        let candidates: Vec<Candidate> = found
            .iter()
            .zip(&keys)
            .map(|(d, key)| Candidate {
                key,
                name: &d.device_name,
            })
            .collect();
        let chosen = found.swap_remove(pick(args, &candidates)?);
        let transport = PlatformTransport::connect(&chosen, args.canal)
            .context("connexion RFCOMM au casque")?;
        Ok((transport, chosen.device_name))
    }

    pub fn run(args: &Args, output: &mut Output) -> anyhow::Result<()> {
        // Avant toute connexion au service ou recherche Bluetooth.
        if let Command::Reglages { action } = &args.command {
            use super::SettingsAction;
            use momentum_core::settings_transfer;
            let dir = paths::data_dir().context("dossier des données introuvable")?;
            let details = match action {
                SettingsAction::Exporter {
                    fichier,
                    avec_batterie,
                } => settings_transfer::export(&dir, fichier, *avec_batterie)?,
                SettingsAction::Importer {
                    fichier,
                    simulation,
                    avec_batterie,
                } => {
                    let plan = settings_transfer::prepare_import(&dir, fichier, *avec_batterie)?;
                    if *simulation {
                        plan.summary().to_string()
                    } else {
                        format!("{}\n{}", plan.summary(), plan.apply()?)
                    }
                }
            };
            output.message(format_args!("{details}"));
            output.details = Some(details);
            return Ok(());
        }
        // Cette opération ne doit jamais tomber dans le chemin RFCOMM direct,
        // attendre une nouvelle lecture ou soumettre une commande au service.
        if let Command::SceneEnregistrer { nom, remplacer } = &args.command {
            let client = momentum_client::Client::connect()
                .context("service requis pour enregistrer son dernier état connu")?;
            let snapshot = client.snapshot()?;
            let state = snapshot
                .state
                .as_ref()
                .context("aucun état connu du service à enregistrer")?;
            let scene = momentum_core::scene_capture::save_current(
                nom,
                state,
                snapshot.equalizer.as_ref(),
                *remplacer,
            )?;
            let message = format!(
                "Scène « {} » enregistrée (rien n'a été envoyé au casque).",
                scene.name
            );
            output.message(format_args!("{message}"));
            output.details = Some(message);
            output.readback.state = snapshot.state;
            output.readback.equalizer = snapshot.equalizer;
            return Ok(());
        }
        // Si le service tourne, il tient la connexion ; on passe par lui
        // plutôt que d'en ouvrir une seconde.
        if args.json && matches!(args.command, Command::Scenes) {
            list_scenes(output);
            return Ok(());
        }
        if through_daemon(args, output)? {
            return Ok(());
        }
        // Liste des scènes : aucun besoin du casque.
        if let Command::Scenes = args.command {
            list_scenes(output);
            return Ok(());
        }

        let (transport, device_name) = connect(args)?;
        let mut headset = Headset::new(transport, Duration::from_millis(args.timeout_ms));
        output.message(format_args!("Connecté à « {device_name} »."));
        headset.identify();
        if let Some(reason) = headset.write_block_reason() {
            output.message(format_args!("Lecture seule : {reason}."));
        }

        run_direct(args, output, &mut headset)
    }

    /// Même séquence d'échanges en mode texte et JSON, testable sur transport simulé.
    pub(super) fn run_direct<T: Transport>(
        args: &Args,
        output: &mut Output,
        headset: &mut Headset<T>,
    ) -> anyhow::Result<()> {
        match args.command {
            Command::SceneEnregistrer { .. } => {
                bail!("service requis pour enregistrer une scène ; aucune connexion directe")
            }
            Command::Reglages { .. } => bail!("aucun accès au casque pour les réglages locaux"),
            Command::Etat => print_state(output, &headset.read_state()?),
            Command::Scenes | Command::Completions { .. } | Command::Man => {}
            Command::Scene { ref nom } => {
                let (list, _) = paths::load_scenes();
                let Some(scene) = scenes::find(&list, nom) else {
                    bail!("scène « {nom} » introuvable (voir ctl scenes)");
                };
                let report = scenes::apply_scene(headset, scene, &paths::scene_presets());
                output.details = Some(report.details());
                output.message(format_args!("{}", report.details()));
                print_state(output, &headset.read_state()?);
                if !report.is_complete() {
                    bail!("scène appliquée en partie (voir ci-dessus)");
                }
            }
            Command::ExporterProfil { ref fichier } => {
                use std::io::Write;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let export = headset.read_sound_profile(now)?;
                // create_new : on ne remplace jamais une sauvegarde existante.
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(fichier)
                    .with_context(|| format!("création de {}", fichier.display()))?;
                file.write_all(export.to_json().as_bytes())
                    .with_context(|| format!("écriture de {}", fichier.display()))?;
                output.message(format_args!(
                    "Profil exporté dans {} ({} blocs, empreinte {:016x}).",
                    fichier.display(),
                    export.blocks.len(),
                    export.fingerprint()
                ));
                output.message(format_args!(
                    "Données auditives personnelles : ne pas publier ce fichier."
                ));
                output.details = Some(fichier.display().to_string());
            }
            Command::Mode { mode } => {
                let request = match mode {
                    Mode::Arret => NoiseModeRequest::Off,
                    Mode::Adaptatif => NoiseModeRequest::Adaptive,
                    Mode::Personnalise => NoiseModeRequest::Custom,
                };
                headset.set_mode(request)?;
                let state = headset.read_noise()?;
                print_noise(output, &state);
                let applied = matches!(
                    (request, state.mode()),
                    (NoiseModeRequest::Off, M5NoiseMode::Off)
                        | (NoiseModeRequest::Adaptive, M5NoiseMode::Adaptive)
                        | (NoiseModeRequest::Custom, M5NoiseMode::Custom { .. })
                );
                if !applied {
                    bail!("le casque n'a pas appliqué le mode demandé");
                }
            }
            Command::Niveau { niveau } => {
                headset.set_level(niveau)?;
                let state = headset.read_noise()?;
                print_noise(output, &state);
                if state.level != niveau {
                    bail!("niveau relu {} au lieu de {niveau}", state.level);
                }
            }
            Command::AntiVent { etat } => {
                headset.set_anti_wind(etat.is_on())?;
                let state = headset.read_noise()?;
                print_noise(output, &state);
                if state.anti_wind != etat.is_on() {
                    bail!("le casque n'a pas appliqué l'anti-vent");
                }
            }
            Command::Spatial { etat } => {
                // Comme l'app : pas de Dolby Atmos tant que le profil
                // personnalisé est sélectionné.
                if etat.is_on() && headset.audio_mode()? == AudioMode::Personalization {
                    bail!(
                        "Dolby Atmos est incompatible avec le profil personnalisé : \
                         revenez d'abord à l'égaliseur (ctl profil off)"
                    );
                }
                headset.set_spatial_audio(etat.is_on())?;
                let spatial = headset.spatial_audio()?;
                output.readback.spatial_audio = Some(spatial);
                output.message(format_args!(
                    "Audio spatial : {}",
                    if spatial { "activé" } else { "désactivé" }
                ));
                if spatial != etat.is_on() {
                    bail!("le casque n'a pas appliqué l'audio spatial");
                }
            }
            Command::SuiviTete { etat } => {
                headset.set_head_tracking(etat.is_on())?;
                let on = headset.head_tracking()?;
                output.readback.head_tracking = Some(on);
                output.message(format_args!(
                    "Suivi de tête : {}",
                    if on { "activé" } else { "désactivé" }
                ));
                if on != etat.is_on() {
                    bail!("le casque n'a pas appliqué le suivi de tête");
                }
            }
            Command::Multipoint { etat } => {
                headset.set_multipoint(etat.is_on())?;
                // Laisse au casque le temps de couper ou de rouvrir les liens.
                std::thread::sleep(Duration::from_secs(3));
                let slots = headset
                    .connection_slots()
                    .context("relecture du nombre de connexions (le PC a-t-il été déconnecté ?)")?;
                output.readback.multipoint = Some(slots >= 2);
                output.message(format_args!(
                    "Multipoint : {} (connexions simultanées : {slots})",
                    if slots >= 2 { "activé" } else { "désactivé" }
                ));
                if (slots >= 2) != etat.is_on() {
                    bail!("le casque n'a pas appliqué le Multipoint");
                }
                print_connections(output, &headset.read_connections()?);
            }
            Command::Eq => print_eq(output, &headset.read_eq()?),
            Command::Bande { numero, gain } => {
                let mut eq = headset.read_eq()?;
                let band = numero - 1;
                // dB → dixièmes de dB ; les limites sont vérifiées par la bibliothèque.
                let tenths = (gain * 10.0).round();
                if !(f32::from(i8::MIN)..=f32::from(i8::MAX)).contains(&tenths) {
                    bail!("gain {gain} dB hors limites");
                }
                let tenths = tenths as i8;
                headset.set_eq_band(&mut eq, band, tenths)?;
                let after = headset.read_eq()?;
                print_eq(output, &after);
                if after.gains.get(usize::from(band)) != Some(&tenths) {
                    bail!("le casque n'a pas appliqué le gain demandé");
                }
            }
            Command::Plat => {
                let mut eq = headset.read_eq()?;
                let flat = vec![0; eq.gains.len()];
                headset.set_eq_gains(&mut eq, &flat)?;
                let after = headset.read_eq()?;
                print_eq(output, &after);
                if after.gains.iter().any(|&g| g != 0) {
                    bail!("le casque n'a pas remis toutes les bandes à 0");
                }
            }
            Command::Voix { etat } => {
                headset.set_speech_clarity(etat.is_on())?;
                let state = headset.read_state()?;
                print_state(output, &state);
                let wanted = if etat.is_on() {
                    AudioMode::SpeechClarity
                } else {
                    AudioMode::Equalizer
                };
                if state.audio_mode != Some(wanted) {
                    bail!("le casque n'a pas appliqué Speech Clarity");
                }
            }
            Command::Profil { etat } => {
                if etat.is_on() {
                    if headset.select_sound_profile()?
                        == momentum_core::device::ProfileSelection::AlreadySelectedAtRead
                    {
                        output.message(format_args!(
                            "Profil déjà sélectionné à la lecture : rien n'a été écrit."
                        ));
                    }
                } else {
                    headset.set_speech_clarity(false)?;
                }
                let state = headset.read_state()?;
                print_state(output, &state);
                let wanted = if etat.is_on() {
                    AudioMode::Personalization
                } else {
                    AudioMode::Equalizer
                };
                if state.audio_mode != Some(wanted) {
                    bail!("le casque n'a pas appliqué le profil personnalisé");
                }
            }
            Command::Tactile { etat } => {
                headset.set_touch_controls(etat.is_on())?;
                let state = headset.read_state()?;
                print_state(output, &state);
                if state.touch_controls != Some(etat.is_on()) {
                    bail!("le casque n'a pas appliqué les commandes tactiles");
                }
            }
            Command::ProtectionBatterie { etat } => {
                headset.set_battery_protection(etat.is_on())?;
                let state = headset.read_state()?;
                print_state(output, &state);
                if state.battery_protection != Some(etat.is_on()) {
                    bail!("le casque n'a pas appliqué la protection de la batterie");
                }
            }
            Command::SmartPause { etat } => {
                headset.set_smart_pause(etat.is_on())?;
                check(headset, output, "Smart Pause", |s| {
                    s.smart_pause == Some(etat.is_on())
                })?;
            }
            Command::ReponseAuto { etat } => {
                headset.set_auto_answer(etat.is_on())?;
                check(headset, output, "la réponse automatique", |s| {
                    s.auto_answer == Some(etat.is_on())
                })?;
            }
            Command::AppelsConfort { etat } => {
                headset.set_comfort_call(etat.is_on())?;
                check(headset, output, "les appels confortables", |s| {
                    s.comfort_call == Some(etat.is_on())
                })?;
            }
            Command::Extinction { duree } => {
                headset.set_auto_off(duree.seconds())?;
                check(headset, output, "l'extinction automatique", |s| {
                    s.auto_off_seconds == Some(duree.seconds())
                })?;
            }
            Command::Nom { ref nouveau } => {
                if let Some(wanted) = nouveau {
                    headset.set_name(wanted)?;
                }
                let read = headset.read_device_info()?.name;
                output.readback.name = read.clone();
                output.message(format_args!(
                    "Nom : {}",
                    read.as_deref().unwrap_or("non lu")
                ));
                if nouveau.is_some() && read != *nouveau {
                    bail!("le casque n'a pas appliqué le nom");
                }
            }
            Command::Connexions => print_connections(output, &headset.read_connections()?),
            Command::Connecter { n } | Command::Deconnecter { n } => {
                let connect = matches!(args.command, Command::Connecter { .. });
                let list = headset.read_connections()?;
                let Some(device) = n.checked_sub(1).and_then(|i| list.get(i)) else {
                    bail!("pas d'appareil n° {n} (voir `ctl connexions`)");
                };
                headset.set_connection(device, connect)?;
                output.message(format_args!(
                    "Demande envoyée : {} « {} ».",
                    if connect {
                        "connexion de"
                    } else {
                        "déconnexion de"
                    },
                    device.name
                ));
                // La connexion Bluetooth prend quelques secondes.
                std::thread::sleep(Duration::from_secs(5));
                print_connections(output, &headset.read_connections()?);
            }
            Command::VolumeInvites { pourcent } => {
                let wanted = momentum_core::device::prompt_volume_from_percent(pourcent);
                headset.set_prompt_volume(wanted)?;
                let read = headset.read_device_info()?.prompt_volume;
                output.readback.prompt_volume = read;
                output.message(format_args!(
                    "Volume des invites : {} (valeur {})",
                    read.map_or("non lu".to_string(), |v| format!(
                        "{} %",
                        momentum_core::device::prompt_volume_percent(v)
                    )),
                    read.map_or("?".to_string(), |v| v.to_string())
                ));
                if read != Some(wanted) {
                    bail!("le casque n'a pas appliqué le volume des invites");
                }
            }
            Command::Invites { mode } => {
                let wanted = match mode {
                    Prompts::Tonalites => PromptMode::TonesOnly,
                    Prompts::Voix => PromptMode::TonesAndVoice,
                };
                headset.set_prompt_mode(wanted)?;
                check(headset, output, "le mode des invites", |s| {
                    s.prompt_mode == Some(wanted)
                })?;
            }
            Command::Basses { etat } => {
                headset.set_bass_boost(etat.is_on())?;
                let after = headset.read_eq()?;
                print_eq(output, &after);
                if after.bass_boost != etat.is_on() {
                    bail!("le casque n'a pas appliqué l'Amplification des basses");
                }
            }
        }
        Ok(())
    }

    /// Relit l'état, l'affiche et échoue si `applied` est faux.
    fn check<T: Transport>(
        headset: &mut Headset<T>,
        output: &mut Output,
        what: &str,
        applied: impl Fn(&HeadsetState) -> bool,
    ) -> anyhow::Result<()> {
        let state = headset.read_state()?;
        print_state(output, &state);
        if !applied(&state) {
            bail!("le casque n'a pas appliqué {what}");
        }
        Ok(())
    }

    fn print_connections(output: &mut Output, list: &[momentum_core::device::Connection]) {
        output.readback.connections = Some(
            list.iter()
                .map(|c| ConnectionSummary {
                    name: c.name.clone(),
                    connected: c.connected,
                    this_device: c.this_device,
                })
                .collect(),
        );
        for (i, d) in list.iter().enumerate() {
            output.message(format_args!(
                "{}. {}{} : {}",
                i + 1,
                d.name,
                if d.this_device { " (ce PC)" } else { "" },
                if d.connected {
                    "connecté"
                } else {
                    "non connecté"
                }
            ));
        }
    }

    fn print_state(output: &mut Output, state: &HeadsetState) {
        output.readback.state = Some(*state);
        output.message(format_args!("{}", describe_state(state)));
    }

    fn print_eq(output: &mut Output, eq: &EqState) {
        output.readback.equalizer = Some(eq.clone());
        let count = eq.gains.len();
        output.message(format_args!(
            "Égaliseur : {count} bandes, de {} à {}",
            format_db(eq.min_gain),
            format_db(eq.max_gain)
        ));
        for (i, &g) in eq.gains.iter().enumerate() {
            output.message(format_args!(
                "  {:>2}. {:>8} : {}",
                i + 1,
                band_label(i, count),
                format_db(g)
            ));
        }
        output.message(format_args!(
            "Amplification des basses : {}",
            if eq.bass_boost {
                "activée"
            } else {
                "désactivée"
            }
        ));
    }

    fn print_noise(output: &mut Output, state: &NoiseState) {
        output.readback.noise = Some(*state);
        output.message(format_args!("{}", describe_noise(state)));
    }
}
