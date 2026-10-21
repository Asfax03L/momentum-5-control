//! Named pipe Windows du canal local `ctl` ↔ icône (protocole : [`crate::ipc`]),
//! et du service `momentum-daemon` (même mécanique, autre nom et protocole
//! JSON de `momentum-client`).
//!
//! Sécurité : nom propre à l'utilisateur (SID), descripteur qui n'autorise que
//! lui, clients distants refusés, et première instance exigée (si le nom est
//! déjà pris, l'icône renonce au canal au lieu de le partager). Les lectures
//! et écritures du serveur ont un délai : un client muet ne bloque pas le
//! canal.
//!
//! La réponse se termine par un octet nul : le client lit jusqu'à lui puis
//! ferme, et le serveur attend cette fermeture avant de déconnecter (une
//! déconnexion plus tôt jetterait la réponse non lue).

use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_PIPE_BUSY,
    ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
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

use crate::ipc::{MAX_REQUEST_LEN, pipe_name};

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

/// Nom du pipe de l'utilisateur courant.
pub fn user_pipe_name() -> windows::core::Result<String> {
    Ok(pipe_name(&current_user_sid()?))
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
    /// Crée le pipe de l'icône pour l'utilisateur courant, accessible à
    /// lui seul.
    pub fn create() -> windows::core::Result<Self> {
        Self::create_named(pipe_name)
    }

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
        let mut done = 0u32;
        match started {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {}
            Err(e) => return Err(e),
        }
        let millis = timeout.map_or(u32::MAX, |t| {
            u32::try_from(t.as_millis()).unwrap_or(u32::MAX)
        });
        // SAFETY: `overlapped` vit jusqu'à la fin de l'opération (attendue
        // ou annulée puis attendue ci-dessous).
        unsafe {
            let result = GetOverlappedResultEx(self.pipe.0, overlapped, &mut done, millis, false);
            if let Err(e) = result {
                let _ = CancelIoEx(self.pipe.0, Some(overlapped));
                let _ = GetOverlappedResult(self.pipe.0, overlapped, &mut done, true);
                return Err(e);
            }
        }
        Ok(done)
    }

    fn overlapped(&self) -> OVERLAPPED {
        OVERLAPPED {
            hEvent: self.event.0,
            ..OVERLAPPED::default()
        }
    }

    /// Attend un client, lit sa requête (une ligne), répond avec `handler`
    /// puis ferme la connexion. Une erreur ne concerne que ce client.
    pub fn serve_one(&self, handler: impl FnOnce(&str) -> String) -> windows::core::Result<()> {
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

    fn exchange(&self, handler: impl FnOnce(&str) -> String) -> windows::core::Result<()> {
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
        let reply = if request.len() > MAX_REQUEST_LEN {
            crate::ipc::Reply::Error("requête trop longue".to_string()).encode()
        } else {
            match std::str::from_utf8(&request) {
                Ok(text) => handler(text.lines().next().unwrap_or("")),
                Err(_) => crate::ipc::Reply::Error("requête non UTF-8".to_string()).encode(),
            }
        };
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

/// Résultat d'une requête côté client.
pub enum ClientOutcome {
    /// Pas d'icône à l'écoute.
    NoServer,
    /// Réponse brute de l'icône.
    Reply(String),
}

/// Envoie une requête à l'icône et attend sa réponse au plus `timeout`.
pub fn request(line: &str, timeout: Duration) -> Result<ClientOutcome, String> {
    request_named(pipe_name, line, timeout)
}

/// Envoie une requête au pipe `name` (d'après le SID) de l'utilisateur et
/// attend sa réponse au plus `timeout`.
pub fn request_named(
    name: fn(&str) -> String,
    line: &str,
    timeout: Duration,
) -> Result<ClientOutcome, String> {
    let sid = current_user_sid().map_err(|e| format!("SID de l'utilisateur : {e}"))?;
    let name = name(&sid);
    let line = line.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    // Lecture bloquante dans un thread : le délai reste maîtrisé ici.
    std::thread::spawn(move || {
        let _ = tx.send(exchange(&name, &line));
    });
    rx.recv_timeout(timeout)
        .map_err(|_| format!("pas de réponse de l'icône après {timeout:?}"))?
}

fn exchange(name: &str, line: &str) -> Result<ClientOutcome, String> {
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
                FILE_FLAGS_AND_ATTRIBUTES(0),
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
                // SAFETY: simple attente sur le nom du pipe.
                let _ = unsafe { WaitNamedPipeW(&name, 2000) };
            }
            Err(e) => return Err(format!("ouverture du canal de l'icône : {e}")),
        }
    };
    // SAFETY: poignée ouverte ci-dessus ; tampons valides pendant les appels.
    unsafe {
        WriteFile(pipe.0, Some(line.as_bytes()), None, None)
            .map_err(|e| format!("envoi au canal : {e}"))?;
        let mut reply = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut n = 0u32;
            match ReadFile(pipe.0, Some(&mut chunk), Some(&mut n), None) {
                Ok(()) if n == 0 => break,
                Ok(()) => reply.extend_from_slice(&chunk[..n as usize]),
                Err(e) if e.code() == ERROR_BROKEN_PIPE.to_hresult() => break,
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
