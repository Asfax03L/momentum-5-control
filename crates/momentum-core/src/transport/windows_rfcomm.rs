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
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDevice};
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

/// Service GAIA trouvé sur un casque appairé et connecté.
pub struct GaiaService {
    pub device_name: String,
    /// Identifiant Windows de l'appareil (contient son adresse : jamais
    /// affiché ni journalisé ; sert à reconnaître le casque).
    pub device_id: String,
    /// Adresse Bluetooth (même règle : jamais affichée ni journalisée).
    pub address: u64,
    pub service: RfcommDeviceService,
}

/// Appareils Bluetooth Classic appairés **et connectés** qui offrent un
/// service RFCOMM dont le nom SDP contient « GAIA », quel que soit leur nom
/// (F04 de la revue : le nom du casque peut être changé). Un appareil non
/// connecté n'est pas interrogé : la recherche ne réveille rien.
pub fn list_gaia_services() -> Result<Vec<GaiaService>, TransportError> {
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

    let mut found = Vec::new();
    for i in 0..infos.Size().map_err(os("taille de la liste"))? {
        let Ok(info) = infos.GetAt(i) else { continue };
        let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
        let Ok(id) = info.Id() else { continue };
        let device = match wait("ouverture de l'appareil", DISCOVERY_TIMEOUT, |tx| {
            BluetoothDevice::FromIdAsync(&id)?.when(move |r| {
                let _ = tx.send(r);
            })
        }) {
            Ok(d) => d,
            Err(e) => {
                debug!(%e, "appareil illisible");
                continue;
            }
        };
        if device.ConnectionStatus().ok() != Some(BluetoothConnectionStatus::Connected) {
            continue;
        }
        let services = wait("services RFCOMM", DISCOVERY_TIMEOUT, |tx| {
            device
                .GetRfcommServicesWithCacheModeAsync(BluetoothCacheMode::Uncached)?
                .when(move |r| {
                    let _ = tx.send(r);
                })
        })
        .and_then(|r| r.Services().map_err(os("liste des services RFCOMM")));
        let services = match services {
            Ok(s) => s,
            Err(e) => {
                debug!(%e, "services RFCOMM illisibles");
                continue;
            }
        };
        for j in 0..services.Size().unwrap_or(0) {
            let Ok(service) = services.GetAt(j) else {
                continue;
            };
            if sdp_service_name(&service).is_some_and(|n| n.to_lowercase().contains("gaia")) {
                found.push(GaiaService {
                    device_name: name.clone(),
                    device_id: id.to_string(),
                    address: device.BluetoothAddress().unwrap_or(0),
                    service,
                });
                break;
            }
        }
    }
    Ok(found)
}

/// Batterie annoncée par Windows (propriété « niveau de batterie » du nœud
/// mains libres) pour le casque mémorisé, s'il est connecté, sans échange
/// avec lui. Aucune recherche SDP : la liaison GAIA peut être occupée.
/// `None` sans casque mémorisé, s'il n'est pas connecté ou sans valeur.
pub fn connected_headset_battery() -> Option<u8> {
    let remembered = crate::paths::load_headset_fingerprint()?;
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true).ok()?;
    let infos = wait(
        "énumération des appareils appairés",
        DISCOVERY_TIMEOUT,
        |tx| {
            DeviceInformation::FindAllAsyncAqsFilter(&selector)?.when(move |r| {
                let _ = tx.send(r);
            })
        },
    )
    .ok()?;
    for i in 0..infos.Size().ok()? {
        let Ok(id) = infos.GetAt(i).and_then(|info| info.Id()) else {
            continue;
        };
        if super::choice::fingerprint(&id.to_string()) != remembered {
            continue;
        }
        let device = wait("ouverture de l'appareil", DISCOVERY_TIMEOUT, |tx| {
            BluetoothDevice::FromIdAsync(&id)?.when(move |r| {
                let _ = tx.send(r);
            })
        })
        .ok()?;
        if device.ConnectionStatus().ok() != Some(BluetoothConnectionStatus::Connected) {
            return None;
        }
        let address = device.BluetoothAddress().ok().filter(|a| *a != 0)?;
        return system_battery_for(&format!("{address:012x}"));
    }
    None
}

/// Parcourt les nœuds Bluetooth Classic présents (`BTHENUM`) et rend le
/// niveau de batterie de celui qui porte cette adresse (12 chiffres
/// hexadécimaux, en minuscules).
fn system_battery_for(address: &str) -> Option<u8> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CM_Get_DevNode_PropertyW,
        CM_Get_Device_ID_List_SizeW, CM_Get_Device_ID_ListW, CM_LOCATE_DEVNODE_NORMAL,
        CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows::Win32::Devices::Properties::{
        DEVPROP_TYPE_BYTE, DEVPROP_TYPE_STRING, DEVPROPTYPE,
    };
    use windows::Win32::Foundation::DEVPROPKEY;
    use windows::core::{GUID, PCWSTR, w};

    // Clés documentées par Windows : DEVPKEY_Bluetooth_DeviceAddress et
    // DEVPKEY_Bluetooth_Battery.
    const ADDRESS: DEVPROPKEY = DEVPROPKEY {
        fmtid: GUID::from_u128(0x2bd67d8b_8beb_48d5_87e0_6cda3428040a),
        pid: 1,
    };
    const BATTERY: DEVPROPKEY = DEVPROPKEY {
        fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5),
        pid: 2,
    };
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    let mut len = 0u32;
    // SAFETY: `len` est un u32 valide ; le filtre est une chaîne statique.
    if unsafe { CM_Get_Device_ID_List_SizeW(&mut len, w!("BTHENUM"), flags) } != CR_SUCCESS {
        return None;
    }
    let mut ids = vec![0u16; len as usize];
    // SAFETY: le tampon a la taille annoncée juste avant.
    if unsafe { CM_Get_Device_ID_ListW(w!("BTHENUM"), &mut ids, flags) } != CR_SUCCESS {
        return None;
    }
    let property = |node: u32, key: &DEVPROPKEY, kind: DEVPROPTYPE| -> Option<Vec<u8>> {
        let mut found = DEVPROPTYPE::default();
        let mut buffer = vec![0u8; 256];
        let mut size = buffer.len() as u32;
        // SAFETY: tampon et taille cohérents, clé et type valides.
        let r = unsafe {
            CM_Get_DevNode_PropertyW(
                node,
                key,
                &mut found,
                Some(buffer.as_mut_ptr()),
                &mut size,
                0,
            )
        };
        (r == CR_SUCCESS && found == kind).then(|| {
            buffer.truncate(size as usize);
            buffer
        })
    };
    // Liste de chaînes terminées par un zéro, close par un zéro de plus.
    for id in ids.split(|c| *c == 0).filter(|s| !s.is_empty()) {
        let mut wide = id.to_vec();
        wide.push(0);
        let mut node = 0u32;
        // SAFETY: identifiant terminé par un zéro, vivant pendant l'appel.
        if unsafe { CM_Locate_DevNodeW(&mut node, PCWSTR(wide.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) }
            != CR_SUCCESS
        {
            continue;
        }
        let Some(raw) = property(node, &ADDRESS, DEVPROP_TYPE_STRING) else {
            continue;
        };
        let text: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|c| *c != 0)
            .collect();
        if !String::from_utf16_lossy(&text).eq_ignore_ascii_case(address) {
            continue;
        }
        if let Some(&[percent]) = property(node, &BATTERY, DEVPROP_TYPE_BYTE).as_deref()
            && percent <= 100
        {
            return Some(percent);
        }
    }
    None
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
