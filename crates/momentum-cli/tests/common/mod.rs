//! Bus D-Bus privé des essais. Sa configuration ne déclare aucun dossier
//! d'activation : un service installé sur le poste (OpenHeadset Control,
//! BlueZ…) ne peut pas y être démarré, même quand un essai libère le nom.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

pub struct PrivateBus {
    daemon: Child,
    config: PathBuf,
    pub address: String,
}

impl PrivateBus {
    pub fn start() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let config = std::env::temp_dir().join(format!(
            "bus-essai-{}-{}.conf",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&config, CONFIG).expect("configuration du bus d'essai");
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("bus de test privé");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().expect("sortie du bus"))
            .read_line(&mut address)
            .expect("adresse du bus");
        Self {
            daemon,
            config,
            address: address.trim().to_string(),
        }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_file(&self.config);
    }
}
