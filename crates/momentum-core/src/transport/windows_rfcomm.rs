//! Transport RFCOMM (Bluetooth Classic) via l'API WinRT `StreamSocket`.
//!
//! Pas de Tokio : chaque opération asynchrone WinRT est terminée par un
//! callback `.when()` qui pousse le résultat dans un canal `std::sync::mpsc`,
//! attendu avec un délai maximal. Ainsi une pile Bluetooth qui ne répond
//! plus (casque éteint en pleine connexion…) ne bloque jamais le programme.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use tracing::debug;
use windows::Devices::Bluetooth::Rfcomm::RfcommDeviceService;
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Networking::Sockets::StreamSocket;
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer, InputStreamOptions};

use super::{Transport, TransportError};
use crate::sdp::{self, DataElement};

/// Délai maximal d'une énumération (appareils, services, attributs SDP).
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// Délai maximal de l'ouverture du socket RFCOMM.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Délai maximal de l'envoi d'une trame.
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Attend au plus `timeout` une opération WinRT. `start` branche le
/// callback `.when()` de l'opération sur l'émetteur fourni. Au-delà du
/// délai, l'opération est abandonnée (son résultat tardif est ignoré).
fn wait<T: Send + 'static>(
    context: &'static str,
    timeout: Duration,
    start: impl FnOnce(mpsc::Sender<windows::core::Result<T>>) -> windows::core::Result<()>,
) -> Result<T, TransportError> {
    let (tx, rx) = mpsc::channel();
    start(tx).map_err(|e| TransportError::os(context, &e))?;
    match rx.recv_timeout(timeout) {
        Ok(result) => result.map_err(|e| TransportError::os(context, &e)),
        Err(RecvTimeoutError::Timeout) => Err(TransportError::Deadline { context, timeout }),
        Err(RecvTimeoutError::Disconnected) => Err(TransportError::Closed),
    }
}

/// Service GAIA trouvé sur un casque appairé.
pub struct GaiaService {
    pub device_name: String,
    pub service: RfcommDeviceService,
}

/// Cherche, parmi les appareils Bluetooth Classic appairés dont le nom
/// contient l'un des `name_filters` (insensible à la casse), le premier
/// service RFCOMM dont le nom SDP contient « GAIA ».
pub fn find_gaia_service(name_filters: &[&str]) -> Result<Option<GaiaService>, TransportError> {
    let os = |ctx| move |e: windows::core::Error| TransportError::os(ctx, &e);
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)
        .map_err(os("sélecteur d'appareils appairés"))?;
    let infos = wait(
        "énumération des appareils appairés",
        DISCOVERY_TIMEOUT,
        |tx| {
            DeviceInformation::FindAllAsyncAqsFilter(&selector)?.when(move |r| {
                let _ = tx.send(r);
            })
        },
    )?;

    for i in 0..infos.Size().map_err(os("taille de la liste"))? {
        let Ok(info) = infos.GetAt(i) else { continue };
        let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
        let lower = name.to_lowercase();
        if !name_filters
            .iter()
            .any(|f| lower.contains(&f.to_lowercase()))
        {
            continue;
        }
        let services = info
            .Id()
            .map_err(os("identifiant de l'appareil"))
            .and_then(|id| {
                wait("ouverture de l'appareil", DISCOVERY_TIMEOUT, |tx| {
                    BluetoothDevice::FromIdAsync(&id)?.when(move |r| {
                        let _ = tx.send(r);
                    })
                })
            })
            .and_then(|device| {
                wait("services RFCOMM", DISCOVERY_TIMEOUT, |tx| {
                    device
                        .GetRfcommServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                        .when(move |r| {
                            let _ = tx.send(r);
                        })
                })
            })
            .and_then(|r| r.Services().map_err(os("liste des services RFCOMM")));
        let services = match services {
            Ok(s) => s,
            Err(e) => {
                debug!(%name, %e, "services RFCOMM illisibles");
                continue;
            }
        };
        for j in 0..services.Size().unwrap_or(0) {
            let Ok(service) = services.GetAt(j) else {
                continue;
            };
            if sdp_service_name(&service).is_some_and(|n| n.to_lowercase().contains("gaia")) {
                return Ok(Some(GaiaService {
                    device_name: name,
                    service,
                }));
            }
        }
    }
    Ok(None)
}

/// Nom SDP (attribut 0x0100) d'un service RFCOMM, s'il est lisible.
fn sdp_service_name(service: &RfcommDeviceService) -> Option<String> {
    // La table d'attributs n'est pas `Send` : l'attribut est extrait dans le
    // callback et seuls ses octets traversent le canal.
    let bytes = wait("attributs SDP", DISCOVERY_TIMEOUT, |tx| {
        service
            .GetSdpRawAttributesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
            .when(move |r| {
                let _ =
                    tx.send(r.and_then(|map| buffer_bytes(&map.Lookup(sdp::ATTR_SERVICE_NAME)?)));
            })
    })
    .ok()?;
    DataElement::parse(&bytes).ok()?.as_text()
}

fn buffer_bytes(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let mut bytes = vec![0u8; buffer.Length()? as usize];
    DataReader::FromBuffer(buffer)?.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

/// Taille maximale demandée par lecture (une trame GAIA tient largement).
const READ_CHUNK: u32 = 1024;

impl TransportError {
    /// Convertit une erreur Windows en gardant le HRESULT pour le rapport.
    pub fn os(context: &'static str, err: &windows::core::Error) -> Self {
        TransportError::Os {
            context,
            // Le HRESULT est un i32 ; on l'affiche comme les outils Windows (0x8007…).
            hresult: err.code().0 as u32,
            message: err.message(),
        }
    }
}

pub struct RfcommTransport {
    socket: StreamSocket,
    reader: DataReader,
    writer: DataWriter,
    /// Lecture WinRT en cours. Une seule `LoadAsync` peut être active à la
    /// fois : si le délai expire, on la garde pour le prochain `receive`.
    pending: Option<Receiver<windows::core::Result<u32>>>,
}

impl RfcommTransport {
    /// Ouvre une connexion RFCOMM vers le service donné.
    pub fn connect(service: &RfcommDeviceService) -> Result<Self, TransportError> {
        let os = |ctx| move |e: windows::core::Error| TransportError::os(ctx, &e);

        let host = service
            .ConnectionHostName()
            .map_err(os("lecture du nom d'hôte du service"))?;
        let name = service
            .ConnectionServiceName()
            .map_err(os("lecture du nom de service RFCOMM"))?;
        // L'identifiant du service contient les adresses Bluetooth : masquées.
        debug!(
            service = %crate::diagnostic::mask_addresses(&name.to_string_lossy()),
            "connexion RFCOMM"
        );

        let socket = StreamSocket::new().map_err(os("création du StreamSocket"))?;
        wait(
            "connexion RFCOMM (StreamSocket::ConnectAsync)",
            CONNECT_TIMEOUT,
            |tx| {
                socket.ConnectAsync(&host, &name)?.when(move |r| {
                    let _ = tx.send(r);
                })
            },
        )?;

        let reader = DataReader::CreateDataReader(
            &socket
                .InputStream()
                .map_err(os("ouverture du flux d'entrée"))?,
        )
        .map_err(os("création du DataReader"))?;
        // Partial : LoadAsync rend la main dès qu'au moins un octet est arrivé.
        reader
            .SetInputStreamOptions(InputStreamOptions::Partial)
            .map_err(os("configuration du DataReader"))?;
        let writer = DataWriter::CreateDataWriter(
            &socket
                .OutputStream()
                .map_err(os("ouverture du flux de sortie"))?,
        )
        .map_err(os("création du DataWriter"))?;

        Ok(Self {
            socket,
            reader,
            writer,
            pending: None,
        })
    }
}

impl Transport for RfcommTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.writer
            .WriteBytes(bytes)
            .map_err(|e| TransportError::os("écriture dans le DataWriter", &e))?;
        wait("envoi RFCOMM (StoreAsync)", SEND_TIMEOUT, |tx| {
            self.writer.StoreAsync()?.when(move |r| {
                let _ = tx.send(r);
            })
        })?;
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        let rx = match self.pending.take() {
            Some(rx) => rx,
            None => {
                let (tx, rx) = mpsc::channel();
                let op = self
                    .reader
                    .LoadAsync(READ_CHUNK)
                    .map_err(|e| TransportError::os("lecture RFCOMM (LoadAsync)", &e))?;
                op.when(move |result| {
                    // Le récepteur a pu être abandonné (transport fermé) : sans gravité.
                    let _ = tx.send(result);
                })
                .map_err(|e| TransportError::os("attente de la lecture RFCOMM", &e))?;
                rx
            }
        };

        match rx.recv_timeout(timeout) {
            Ok(Ok(0)) => Err(TransportError::Closed),
            Ok(Ok(count)) => {
                let mut buf = vec![0u8; count as usize];
                self.reader
                    .ReadBytes(&mut buf)
                    .map_err(|e| TransportError::os("lecture du DataReader", &e))?;
                Ok(buf)
            }
            Ok(Err(e)) => Err(TransportError::os("lecture RFCOMM", &e)),
            Err(RecvTimeoutError::Timeout) => {
                self.pending = Some(rx);
                Err(TransportError::Timeout(timeout))
            }
            Err(RecvTimeoutError::Disconnected) => Err(TransportError::Closed),
        }
    }
}

impl Drop for RfcommTransport {
    fn drop(&mut self) {
        // Fermer le socket annule une éventuelle lecture en attente.
        if let Err(e) = self.socket.Close() {
            debug!(%e, "fermeture du socket RFCOMM");
        }
    }
}
