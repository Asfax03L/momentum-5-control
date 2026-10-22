//! Instance unique du client, par un nom sur le bus de session :
//! `org.openheadset.ControlWindow`. Une deuxième instance (menu, lanceur)
//! appelle `Show()` sur la première, qui montre sa fenêtre, puis s'arrête.

use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use zbus::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::autostart::HIDDEN_FLAG;
use crate::bridge::qobject::Headset;

const NAME: &str = "org.openheadset.ControlWindow";
const PATH: &str = "/org/openheadset/ControlWindow";
const INTERFACE: &str = "org.openheadset.ControlWindow1";

/// Fil Qt de l'objet `Headset`, connu une fois la fenêtre créée.
static QT_THREAD: OnceLock<CxxQtThread<Headset>> = OnceLock::new();

pub enum Instance {
    /// Première instance : la connexion garde le nom tant qu'elle vit.
    First(Connection),
    AlreadyRunning,
}

struct Window;

#[zbus::interface(name = "org.openheadset.ControlWindow1")]
impl Window {
    /// Montre la fenêtre (deuxième lancement).
    fn show(&self) {
        if let Some(qt_thread) = QT_THREAD.get() {
            let _ = qt_thread.queue(|headset| headset.show_requested());
        }
    }
}

/// À appeler par l'objet `Headset` à sa création.
pub fn attach(qt_thread: CxxQtThread<Headset>) {
    let _ = QT_THREAD.set(qt_thread);
}

pub fn acquire() -> zbus::Result<Instance> {
    zbus::block_on(async {
        let connection = Connection::session().await?;
        connection.object_server().at(PATH, Window).await?;
        // zbus rend un nom déjà pris par l'erreur `NameTaken`.
        match connection
            .request_name_with_flags(NAME, RequestNameFlags::DO_NOT_QUEUE)
            .await
        {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
                Ok(Instance::First(connection))
            }
            Ok(RequestNameReply::Exists | RequestNameReply::InQueue)
            | Err(zbus::Error::NameTaken) => {
                // Lancement automatique en double : rien à montrer.
                if !std::env::args().any(|a| a == HIDDEN_FLAG) {
                    connection
                        .call_method(Some(NAME), PATH, Some(INTERFACE), "Show", &())
                        .await?;
                }
                Ok(Instance::AlreadyRunning)
            }
            Err(e) => Err(e),
        }
    })
}
