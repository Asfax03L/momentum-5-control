//! Vrai adaptateur bluer contre un BlueZ fictif sur bus privé.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use momentum_core::transport::bluez_rfcomm::reconnect_remembered;
use momentum_core::transport::choice::fingerprint;
use momentum_core::transport::reconnect::ReconnectError;

struct Adapter;
#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        true
    }
}

struct Device {
    paired: bool,
    fails: bool,
    calls: Arc<AtomicUsize>,
}
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn paired(&self) -> bool {
        self.paired
    }
    // Un casque déconnecté doit tout de même être retrouvé.
    #[zbus(property)]
    fn connected(&self) -> bool {
        false
    }
    fn connect(&self) -> zbus::fdo::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fails {
            Err(zbus::fdo::Error::Failed(
                "erreur fictive avec adresse 02:00:00:00:00:01".into(),
            ))
        } else {
            Ok(())
        }
    }
}

struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn private_bluez_calls_device_connect_only_for_the_remembered_paired_headset() {
    for scenario in ["success", "unpaired", "missing", "failed", "ambiguous"] {
        let mut bus = Bus(Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap());
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let other_calls = Arc::new(AtomicUsize::new(0));
        let connection = zbus::block_on(async {
            let builder = zbus::connection::Builder::address(address.trim())
                .name("org.bluez")
                .serve_at("/", zbus::fdo::ObjectManager)
                .serve_at("/org/bluez/hci0", Adapter)
                .serve_at("/org/bluez/hci1", Adapter)
                .serve_at(
                    "/org/bluez/hci0/dev_02_00_00_00_00_02",
                    Device {
                        paired: true,
                        fails: false,
                        calls: other_calls.clone(),
                    },
                );
            let builder = if scenario != "missing" {
                builder.serve_at(
                    "/org/bluez/hci1/dev_02_00_00_00_00_01",
                    Device {
                        paired: scenario != "unpaired",
                        fails: scenario == "failed",
                        calls: calls.clone(),
                    },
                )
            } else {
                builder
            };
            let builder = if scenario == "ambiguous" {
                builder.serve_at(
                    "/org/bluez/hci0/dev_02_00_00_00_00_01",
                    Device {
                        paired: true,
                        fails: false,
                        calls: calls.clone(),
                    },
                )
            } else {
                builder
            };
            builder.build().await.unwrap()
        });
        // Processus séparé : aucun changement de l'environnement global des tests,
        // et aucune possibilité de joindre le bus système réel.
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "bluez_client_on_private_bus", "--nocapture"])
            .env("OPENHEADSET_TEST_BLUEZ", scenario)
            .env("OPENHEADSET_TEST_BUS", address.trim())
            .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
            .output()
            .unwrap();
        assert!(output.status.success(), "{scenario}: {output:?}");
        assert_eq!(other_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            usize::from(matches!(scenario, "success" | "failed"))
        );
        drop(connection);
    }
}

#[test]
fn bluez_client_on_private_bus() {
    let Ok(scenario) = std::env::var("OPENHEADSET_TEST_BLUEZ") else {
        return;
    };
    let private_bus = std::env::var("OPENHEADSET_TEST_BUS").expect("bus privé obligatoire");
    assert!(!private_bus.is_empty());
    assert_eq!(
        std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(),
        private_bus
    );
    let result = reconnect_remembered(&fingerprint("02:00:00:00:00:01"));
    let expected = match scenario.as_str() {
        "success" => Ok(()),
        "unpaired" | "missing" => Err(ReconnectError::Missing),
        "failed" => Err(ReconnectError::Failed),
        "ambiguous" => Err(ReconnectError::Ambiguous),
        _ => panic!("scénario inconnu"),
    };
    assert_eq!(result, expected);
    if let Err(error) = result {
        assert!(!error.to_string().contains("02:00:00:00:00:01"));
    }
}
