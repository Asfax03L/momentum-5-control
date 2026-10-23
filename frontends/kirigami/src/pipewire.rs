//! Lecture bornée du graphe : aucune commande de routage, profil ou volume.
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use momentum_core::pipewire_audio::{self, AudioReadError, PipeWireAudio};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn read(headset_id: &str) -> Result<PipeWireAudio, AudioReadError> {
    read_with(
        Path::new("pw-dump"),
        headset_id,
        Duration::from_secs(3),
        pipewire_audio::MAX_DUMP_BYTES,
    )
}

pub fn is_current(snapshot: &momentum_client::Snapshot, session: u64, id: &str) -> bool {
    snapshot.state.is_some()
        && snapshot.session == session
        && snapshot.headset_id.as_deref() == Some(id)
}

fn read_with(
    program: &Path,
    headset_id: &str,
    timeout: Duration,
    limit: usize,
) -> Result<PipeWireAudio, AudioReadError> {
    let child = Command::new(program)
        .arg("-N")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AudioReadError::Missing
            } else {
                AudioReadError::Unavailable
            }
        })?;
    let mut child = Process(child);
    let stdout = child.0.stdout.take().ok_or(AudioReadError::Unavailable)?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("lecture-pw-dump".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send(result);
        })
        .map_err(|_| AudioReadError::Unavailable)?;
    let deadline = Instant::now() + timeout;
    let mut data = None;
    loop {
        if data.is_none() {
            match rx.try_recv() {
                Ok(Ok(bytes)) if bytes.len() > limit => return Err(AudioReadError::TooLarge),
                Ok(Ok(bytes)) => data = Some(bytes),
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(AudioReadError::Unavailable);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|_| AudioReadError::Unavailable)?
        {
            if !status.success() {
                return Err(AudioReadError::Unavailable);
            }
            if let Some(data) = &data {
                let text = std::str::from_utf8(data).map_err(|_| AudioReadError::Invalid)?;
                return pipewire_audio::parse_dump(text, Some(headset_id));
            }
        }
        if Instant::now() >= deadline {
            return Err(AudioReadError::Timeout);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Script(PathBuf);
    impl Script {
        fn new(text: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "pw-dump-fictif-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&path, format!("#!/bin/sh\n{text}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Script {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn check(script: &Script) -> Result<PipeWireAudio, AudioReadError> {
        read_with(
            &script.0,
            "0123456789abcdef",
            Duration::from_millis(200),
            4096,
        )
    }
    #[test]
    fn missing_failed_invalid_and_limited_commands_are_reported_without_stderr() {
        let absent = Script::new("exit 0");
        fs::remove_file(&absent.0).unwrap();
        assert_eq!(check(&absent), Err(AudioReadError::Missing));
        assert_eq!(
            check(&Script::new(
                "echo 'information privee fictive' >&2; exit 1"
            )),
            Err(AudioReadError::Unavailable)
        );
        assert_eq!(
            check(&Script::new("printf '{corrompu}'")),
            Err(AudioReadError::Invalid)
        );
        assert_eq!(
            check(&Script::new("head -c 8192 /dev/zero")),
            Err(AudioReadError::TooLarge)
        );
        assert!(check(&Script::new("printf '[]' ")).is_ok());
    }
    #[test]
    fn blocked_command_times_out_and_is_reaped() {
        let before = Instant::now();
        assert_eq!(
            check(&Script::new("exec sleep 5")),
            Err(AudioReadError::Timeout)
        );
        assert!(before.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn results_from_a_lost_or_different_connection_are_rejected() {
        let mut snapshot = momentum_client::Snapshot {
            state: Some(momentum_core::device::HeadsetState::default()),
            headset_id: Some("0123456789abcdef".into()),
            session: 4,
            ..Default::default()
        };
        assert!(is_current(&snapshot, 4, "0123456789abcdef"));
        assert!(!is_current(&snapshot, 3, "0123456789abcdef"));
        assert!(!is_current(&snapshot, 4, "fedcba9876543210"));
        snapshot.state = None;
        assert!(!is_current(&snapshot, 4, "0123456789abcdef"));
        snapshot.state = Some(Default::default());
        snapshot.headset_id = None;
        assert!(!is_current(&snapshot, 4, "0123456789abcdef"));
    }
}
