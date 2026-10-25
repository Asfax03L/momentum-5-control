//! Named pipe Windows du service `momentum-daemon` (protocole JSON de
//! `momentum-client`).
//!
//! Sécurité : nom propre à l'utilisateur (SID), descripteur qui n'autorise que
//! lui, clients distants refusés, et première instance exigée (si le nom est
//! déjà pris, le service renonce au canal au lieu de le partager). Les lectures
//! et écritures du serveur ont un délai : un client muet ne bloque pas le
//! canal.
//!
//! La réponse se termine par un octet nul : le client lit jusqu'à lui puis
//! ferme, et le serveur attend cette fermeture avant de déconnecter (une
//! déconnexion plus tôt jetterait la réponse non lue).

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PIPE_BUSY,
    ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{CreateEventW, GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

/// Taille maximale d'une requête, en octets.
const MAX_REQUEST_LEN: usize = 1024;

/// Délai laissé au client pour envoyer sa requête et lire la réponse.
const CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Poignée fermée automatiquement.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: poignée possédée par cette structure, fermée une seule fois.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// SID textuel de l'utilisateur du processus (« S-1-5-21-… »).
pub fn current_user_sid() -> windows::core::Result<String> {
    // SAFETY: appels Win32 documentés ; le tampon est dimensionné par le
    // premier appel et les pointeurs restent valides pendant leur usage.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let token = Handle(token);
        let mut len = 0u32;
        // Premier appel : taille seulement (échec attendu).
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut len);
        // Tampon aligné pour TOKEN_USER.
        let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            len,
            &mut len,
        )?;
        let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text)?;
        let sid = text.to_string();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        sid.map_err(|_| windows::core::Error::from(ERROR_FILE_NOT_FOUND))
    }
}

/// Serveur : une connexion à la fois, une requête par connexion.
pub struct PipeServer {
    pipe: Handle,
    event: Handle,
}

// SAFETY: les poignées Win32 sont utilisables depuis n'importe quel thread ;
// le serveur n'est utilisé que par le thread qui le possède.
unsafe impl Send for PipeServer {}

impl PipeServer {
    /// Crée un pipe de l'utilisateur courant, accessible à lui seul ;
    /// `name` donne son nom d'après le SID.
    pub fn create_named(name: fn(&str) -> String) -> windows::core::Result<Self> {
        let sid = current_user_sid()?;
        let name = HSTRING::from(name(&sid));
        // Accès complet pour l'utilisateur, rien pour les autres ; DACL
        // protégée (pas d'héritage).
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
        // SAFETY: le descripteur alloué par Windows est libéré par LocalFree
        // après la création du pipe, qui en garde sa propre copie.
        unsafe {
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let pipe = CreateNamedPipeW(
                &name,
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                4096,
                4096,
                0,
                Some(&attributes),
            );
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            if pipe.is_invalid() {
                return Err(windows::core::Error::from_thread());
            }
            let pipe = Handle(pipe);
            let event = Handle(CreateEventW(None, true, false, None)?);
            Ok(Self { pipe, event })
        }
    }

    /// Attend une opération asynchrone, au plus `timeout` (`None` : sans
    /// limite). Annule l'opération si le délai est dépassé.
    fn finish(
        &self,
        started: windows::core::Result<()>,
        overlapped: &OVERLAPPED,
        timeout: Option<Duration>,
    ) -> windows::core::Result<u32> {
        finish_io(self.pipe.0, started, overlapped, timeout)
    }

    fn overlapped(&self) -> OVERLAPPED {
        OVERLAPPED {
            hEvent: self.event.0,
            ..OVERLAPPED::default()
        }
    }

    /// Attend un client, lit sa requête (une ligne), répond avec `handler`
    /// puis ferme la connexion. Une erreur ne concerne que ce client.
    /// `handler` reçoit la requête, ou la raison de son refus (trop longue,
    /// non UTF-8) : la réponse reste ainsi dans le protocole du serveur.
    pub fn serve_one(
        &self,
        handler: impl FnOnce(Result<&str, &str>) -> String,
    ) -> windows::core::Result<()> {
        let mut overlapped = self.overlapped();
        // SAFETY: appels sur la poignée possédée ; tampons valides pendant
        // chaque opération (attendue avant de sortir de la portée).
        unsafe {
            let connected = ConnectNamedPipe(self.pipe.0, Some(&mut overlapped));
            match connected {
                Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => {}
                started => {
                    self.finish(started, &overlapped, None)?;
                }
            }
            let result = self.exchange(handler);
            if result.is_ok() {
                // Attend que le client ferme (fin de lecture), 5 s au plus.
                let mut byte = [0u8; 1];
                let mut overlapped = self.overlapped();
                let started = ReadFile(self.pipe.0, Some(&mut byte), None, Some(&mut overlapped));
                let _ = self.finish(started, &overlapped, Some(CLIENT_IO_TIMEOUT));
            }
            let _ = DisconnectNamedPipe(self.pipe.0);
            result
        }
    }

    fn exchange(
        &self,
        handler: impl FnOnce(Result<&str, &str>) -> String,
    ) -> windows::core::Result<()> {
        let mut request = Vec::new();
        let mut chunk = [0u8; 256];
        while !request.contains(&b'\n') && request.len() <= MAX_REQUEST_LEN {
            let mut overlapped = self.overlapped();
            // SAFETY: `chunk` et `overlapped` vivent jusqu'à la fin de la lecture.
            let started =
                unsafe { ReadFile(self.pipe.0, Some(&mut chunk), None, Some(&mut overlapped)) };
            let n = self.finish(started, &overlapped, Some(CLIENT_IO_TIMEOUT))?;
            if n == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..n as usize]);
        }
        let reply = handler(request_line(&request));
        let reply = reply.replace(' ', "") + " ";
        let mut overlapped = self.overlapped();
        // SAFETY: `reply` et `overlapped` vivent jusqu'à la fin de l'écriture.
        let started = unsafe {
            WriteFile(
                self.pipe.0,
                Some(reply.as_bytes()),
                None,
                Some(&mut overlapped),
            )
        };
        self.finish(started, &overlapped, Some(CLIENT_IO_TIMEOUT))?;
        Ok(())
    }
}

/// Attend une opération asynchrone sur `handle`, au plus `timeout` (`None` :
/// sans limite). Délai dépassé : l'opération est annulée puis attendue, ses
/// tampons peuvent donc être libérés au retour.
fn finish_io(
    handle: HANDLE,
    started: windows::core::Result<()>,
    overlapped: &OVERLAPPED,
    timeout: Option<Duration>,
) -> windows::core::Result<u32> {
    let mut done = 0u32;
    match started {
        Ok(()) => {}
        Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {}
        Err(e) => return Err(e),
    }
    let millis = timeout.map_or(u32::MAX, |t| {
        u32::try_from(t.as_millis()).unwrap_or(u32::MAX)
    });
    // SAFETY: `overlapped` vit jusqu'à la fin de l'opération (attendue ou
    // annulée puis attendue ci-dessous).
    unsafe {
        let result = GetOverlappedResultEx(handle, overlapped, &mut done, millis, false);
        if let Err(e) = result {
            let _ = CancelIoEx(handle, Some(overlapped));
            let _ = GetOverlappedResult(handle, overlapped, &mut done, true);
            return Err(e);
        }
    }
    Ok(done)
}

/// Première ligne d'une requête reçue, ou la raison de son refus.
fn request_line(request: &[u8]) -> Result<&str, &'static str> {
    if request.len() > MAX_REQUEST_LEN {
        return Err("requête trop longue");
    }
    let text = std::str::from_utf8(request).map_err(|_| "requête non UTF-8")?;
    Ok(text.lines().next().unwrap_or(""))
}

/// Résultat d'une requête côté client.
pub enum ClientOutcome {
    /// Pas de serveur à l'écoute.
    NoServer,
    /// Réponse brute du serveur.
    Reply(String),
}

/// Envoie une requête au pipe `name` (d'après le SID) de l'utilisateur et
/// attend sa réponse au plus `timeout`. Délai dépassé : l'opération en cours
/// est annulée et le canal fermé avant de rendre l'erreur (A09 de l'audit
/// du 24/10/2026) ; aucun thread ni poignée ne reste en attente.
pub fn request_named(
    name: fn(&str) -> String,
    line: &str,
    timeout: Duration,
) -> Result<ClientOutcome, String> {
    let sid = current_user_sid().map_err(|e| format!("SID de l'utilisateur : {e}"))?;
    exchange(&name(&sid), line, timeout)
}

fn exchange(name: &str, line: &str, timeout: Duration) -> Result<ClientOutcome, String> {
    let deadline = Instant::now() + timeout;
    let late = || format!("pas de réponse du service après {timeout:?}");
    let left = || deadline.saturating_duration_since(Instant::now());
    // Délai écoulé : code WAIT_TIMEOUT de GetOverlappedResultEx (l'attente
    // est arrondie à la milliseconde : l'horloge seule ne suffit pas).
    let timed_out = |e: &windows::core::Error| {
        e.code() == windows::core::HRESULT::from_win32(WAIT_TIMEOUT.0) || left().is_zero()
    };
    let name = HSTRING::from(name);
    let mut attempts = 0;
    let pipe = loop {
        // SAFETY: ouverture d'un pipe existant ; poignée fermée par `Handle`.
        let opened = unsafe {
            CreateFileW(
                &name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        };
        match opened {
            Ok(h) => break Handle(h),
            Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {
                return Ok(ClientOutcome::NoServer);
            }
            // Un autre client est servi : on patiente un peu.
            Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() && attempts < 5 => {
                attempts += 1;
                let wait = u32::try_from(left().as_millis().min(2000)).unwrap_or(2000);
                if wait == 0 {
                    return Err(late());
                }
                // SAFETY: simple attente sur le nom du pipe.
                let _ = unsafe { WaitNamedPipeW(&name, wait) };
            }
            Err(e) => return Err(format!("ouverture du canal du service : {e}")),
        }
    };
    // SAFETY: poignée et événement possédés ; chaque opération est attendue
    // (ou annulée puis attendue) avant que ses tampons sortent de portée.
    unsafe {
        let event = Handle(CreateEventW(None, true, false, None).map_err(|e| e.to_string())?);
        let overlapped = || OVERLAPPED {
            hEvent: event.0,
            ..OVERLAPPED::default()
        };
        let mut write = overlapped();
        let started = WriteFile(pipe.0, Some(line.as_bytes()), None, Some(&mut write));
        finish_io(pipe.0, started, &write, Some(left())).map_err(|e| {
            if timed_out(&e) {
                late()
            } else {
                format!("envoi au canal : {e}")
            }
        })?;
        let mut reply = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut read = overlapped();
            let started = ReadFile(pipe.0, Some(&mut chunk), None, Some(&mut read));
            match finish_io(pipe.0, started, &read, Some(left())) {
                Ok(0) => break,
                Ok(n) => reply.extend_from_slice(&chunk[..n as usize]),
                Err(e) if e.code() == ERROR_BROKEN_PIPE.to_hresult() => break,
                Err(e) if timed_out(&e) => return Err(late()),
                Err(e) => return Err(format!("lecture du canal : {e}")),
            }
            // Fin de la réponse : on ferme, ce qui libère le serveur.
            if let Some(end) = reply.iter().position(|&b| b == 0) {
                reply.truncate(end);
                break;
            }
            if reply.len() > 1 << 20 {
                return Err("réponse trop longue".to_string());
            }
        }
        Ok(ClientOutcome::Reply(
            String::from_utf8_lossy(&reply).into_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nom d'un canal d'essai propre à ce processus.
    fn silent_name(_sid: &str) -> String {
        format!(r"\\.\pipe\momentum-essai-muet-{}", std::process::id())
    }

    #[test]
    fn a_silent_server_times_out_and_the_client_releases_the_pipe() {
        // A09 : serveur qui accepte, lit la requête et ne répond jamais. À
        // l'échéance, le client annule et ferme : le serveur le voit (sa
        // lecture suivante se termine), au lieu d'un thread resté bloqué.
        let (closed_tx, closed_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let name = HSTRING::from(silent_name(""));
            loop {
                // SAFETY: canal d'essai local ; poignée fermée par `Handle`.
                unsafe {
                    let pipe = Handle(CreateNamedPipeW(
                        &name,
                        PIPE_ACCESS_DUPLEX,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                        1,
                        4096,
                        4096,
                        0,
                        None,
                    ));
                    let _ = ready_tx.send(());
                    let _ = ConnectNamedPipe(pipe.0, None);
                    let mut request = [0u8; 64];
                    let mut n = 0u32;
                    let _ = ReadFile(pipe.0, Some(&mut request), Some(&mut n), None);
                    // Attend la fermeture côté client (fin de fichier ou erreur).
                    let _ = ReadFile(pipe.0, Some(&mut request), Some(&mut n), None);
                    let _ = closed_tx.send(());
                }
            }
        });
        for round in 0..3 {
            ready_rx.recv().unwrap();
            let started = Instant::now();
            let result = request_named(silent_name, "{}\n", Duration::from_millis(300));
            assert!(
                matches!(&result, Err(e) if e.contains("pas de réponse")),
                "tour {round} : {:?}",
                result.as_ref().err()
            );
            assert!(started.elapsed() < Duration::from_secs(2), "tour {round}");
            closed_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|_| panic!("tour {round} : canal resté ouvert côté client"));
        }
    }

    #[test]
    fn request_line_keeps_the_first_line_and_names_refusals() {
        assert_eq!(request_line(b"{\"Snapshot\"}\nreste"), Ok("{\"Snapshot\"}"));
        assert_eq!(request_line(b""), Ok(""));
        assert_eq!(request_line(&[0xff, b'\n']), Err("requête non UTF-8"));
        let long = vec![b'a'; MAX_REQUEST_LEN + 1];
        assert_eq!(request_line(&long), Err("requête trop longue"));
    }
}
