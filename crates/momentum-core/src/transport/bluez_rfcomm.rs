//! Transport RFCOMM (Bluetooth Classic) sous Linux, via BlueZ et `bluer`.
//!
//! BlueZ ne donne pas les noms SDP des services, seulement leurs UUID : le
//! service GAIA est désigné par l'UUID relevé sur le MOMENTUM 5 (probe
//! Windows, `docs/MOMENTUM5.md`). La connexion passe par un profil client
//! BlueZ (`RegisterProfile` + `ConnectProfile`) : BlueZ fait la recherche
//! SDP et nous remet le socket. Avec `--canal`, on ouvre directement un
//! canal RFCOMM donné, sans SDP. Les deux chemins sont **à valider sur le
//! matériel**.
//!
//! `bluer` est asynchrone : un petit runtime Tokio (un thread) est gardé
//! par le transport, et chaque opération est bornée par un délai, comme
//! sous Windows.

use std::future::Future;
use std::time::Duration;

use bluer::rfcomm::{Profile, ProfileHandle, Role, SocketAddr, Stream};
use bluer::{Address, Session, Uuid};
use futures::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;
use tracing::debug;

use super::{Transport, TransportError};

/// UUID du service RFCOMM nommé « GAIA » sur le MOMENTUM 5, relevé par le
/// probe Windows (deux enregistrements, canaux 11 et 12).
pub const GAIA_SERVICE_UUID: Uuid = Uuid::from_u128(0xA2129FF3_081B_4C45_8AFE_469D9C4842EC);

/// Délai maximal d'une recherche (adaptateur, appareils, propriétés).
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Délai maximal de l'ouverture du canal RFCOMM.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Délai maximal de l'envoi d'une trame.
const SEND_TIMEOUT: Duration = Duration::from_secs(5);
/// Taille maximale lue d'un coup (une trame GAIA tient largement).
const READ_CHUNK: usize = 1024;

fn bt(context: &'static str) -> impl Fn(bluer::Error) -> TransportError {
    move |e| TransportError::Bluetooth {
        context,
        message: e.to_string(),
    }
}

fn io(context: &'static str) -> impl Fn(std::io::Error) -> TransportError {
    move |e| TransportError::Bluetooth {
        context,
        message: e.to_string(),
    }
}

fn runtime() -> Result<Runtime, TransportError> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(io("démarrage du runtime Tokio"))
}

/// Attend `future` au plus `timeout` ; au-delà, l'opération est abandonnée.
async fn bounded<T>(
    context: &'static str,
    timeout: Duration,
    future: impl Future<Output = Result<T, TransportError>>,
) -> Result<T, TransportError> {
    tokio::time::timeout(timeout, future)
        .await
        .unwrap_or(Err(TransportError::Deadline { context, timeout }))
}

/// Casque appairé et connecté qui annonce le service GAIA.
#[derive(Clone, Debug)]
pub struct GaiaDevice {
    pub device_name: String,
    pub address: Address,
}

/// Cherche, parmi les appareils appairés **et connectés** de l'adaptateur
/// par défaut dont le nom contient l'un des `name_filters` (insensible à la
/// casse), le premier qui annonce [`GAIA_SERVICE_UUID`].
pub fn find_gaia_device(name_filters: &[&str]) -> Result<Option<GaiaDevice>, TransportError> {
    let rt = runtime()?;
    let filters: Vec<String> = name_filters.iter().map(|f| f.to_lowercase()).collect();
    rt.block_on(bounded("recherche du casque", DISCOVERY_TIMEOUT, async {
        let session = Session::new().await.map_err(bt("connexion à BlueZ"))?;
        let adapter = session
            .default_adapter()
            .await
            .map_err(bt("adaptateur Bluetooth"))?;
        for address in adapter
            .device_addresses()
            .await
            .map_err(bt("liste des appareils"))?
        {
            let device = adapter.device(address).map_err(bt("appareil"))?;
            let name = device.alias().await.map_err(bt("nom de l'appareil"))?;
            let lower = name.to_lowercase();
            if !filters.iter().any(|f| lower.contains(f.as_str())) {
                continue;
            }
            let paired = device.is_paired().await.map_err(bt("appairage"))?;
            let connected = device.is_connected().await.map_err(bt("connexion"))?;
            let uuids = device.uuids().await.map_err(bt("services"))?;
            let has_gaia = uuids.is_some_and(|u| u.contains(&GAIA_SERVICE_UUID));
            debug!(%name, paired, connected, has_gaia, "appareil candidat");
            if paired && connected && has_gaia {
                return Ok(Some(GaiaDevice {
                    device_name: name,
                    address,
                }));
            }
        }
        Ok(None)
    }))
}

pub struct BluezTransport {
    // Ordre de destruction : le flux d'abord, puis le profil (qui le
    // fermerait côté BlueZ), la session, et enfin le runtime.
    stream: Stream,
    _profile: Option<ProfileHandle>,
    _session: Session,
    rt: Runtime,
}

impl BluezTransport {
    /// Ouvre le canal GAIA. `channel` : canal RFCOMM imposé (sans SDP) ;
    /// sinon BlueZ choisit l'enregistrement GAIA via SDP.
    pub fn connect(device: &GaiaDevice, channel: Option<u8>) -> Result<Self, TransportError> {
        let rt = runtime()?;
        let address = device.address;
        let (stream, profile, session) =
            rt.block_on(bounded("connexion RFCOMM", CONNECT_TIMEOUT, async move {
                let session = Session::new().await.map_err(bt("connexion à BlueZ"))?;
                if let Some(channel) = channel {
                    let stream = Stream::connect(SocketAddr::new(address, channel))
                        .await
                        .map_err(io("ouverture du canal RFCOMM"))?;
                    return Ok((stream, None, session));
                }
                let (stream, profile) = connect_profile(&session, address).await?;
                Ok((stream, Some(profile), session))
            }))?;
        debug!(?channel, "canal GAIA ouvert");
        Ok(Self {
            stream,
            _profile: profile,
            _session: session,
            rt,
        })
    }
}

/// Profil client BlueZ : `ConnectProfile` ne rend la main qu'après que la
/// connexion remise (`NewConnection`) a été acceptée, d'où l'attente en
/// parallèle des deux.
async fn connect_profile(
    session: &Session,
    address: Address,
) -> Result<(Stream, ProfileHandle), TransportError> {
    let adapter = session
        .default_adapter()
        .await
        .map_err(bt("adaptateur Bluetooth"))?;
    let device = adapter.device(address).map_err(bt("appareil"))?;
    let mut profile = session
        .register_profile(Profile {
            uuid: GAIA_SERVICE_UUID,
            role: Some(Role::Client),
            require_authentication: Some(true),
            auto_connect: Some(false),
            ..Default::default()
        })
        .await
        .map_err(bt("enregistrement du profil GAIA"))?;
    let connect = device.connect_profile(&GAIA_SERVICE_UUID);
    tokio::pin!(connect);
    let mut connected = false;
    let request = loop {
        tokio::select! {
            result = &mut connect, if !connected => {
                result.map_err(bt("ConnectProfile GAIA"))?;
                connected = true;
            }
            request = profile.next() => {
                break request.ok_or(TransportError::Closed)?;
            }
        }
    };
    let stream = request.accept().map_err(bt("acceptation du canal GAIA"))?;
    if !connected {
        connect.await.map_err(bt("ConnectProfile GAIA"))?;
    }
    Ok((stream, profile))
}

impl Transport for BluezTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        let stream = &mut self.stream;
        self.rt
            .block_on(bounded("envoi de la trame", SEND_TIMEOUT, async move {
                stream
                    .write_all(bytes)
                    .await
                    .map_err(io("envoi de la trame"))?;
                stream.flush().await.map_err(io("envoi de la trame"))
            }))
    }

    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        let stream = &mut self.stream;
        self.rt.block_on(async move {
            let mut buf = vec![0u8; READ_CHUNK];
            match tokio::time::timeout(timeout, stream.read(&mut buf)).await {
                Err(_) => Err(TransportError::Timeout(timeout)),
                Ok(Ok(0)) => Err(TransportError::Closed),
                Ok(Ok(n)) => {
                    buf.truncate(n);
                    Ok(buf)
                }
                Ok(Err(e)) => Err(io("lecture du canal RFCOMM")(e)),
            }
        })
    }
}
