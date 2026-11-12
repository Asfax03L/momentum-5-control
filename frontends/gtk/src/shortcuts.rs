//! Raccourcis clavier globaux par le portail XDG `GlobalShortcuts`
//! (GNOME 48 et suivants, aussi servi par d'autres bureaux).
//!
//! Interface publique du portail : `CreateSession(a{sv}) -> o`,
//! `BindShortcuts(o, a(sa{sv}), s, a{sv}) -> o`, réponses par le signal
//! `Response(u, a{sv})` de l'objet `Request`, appuis par le signal
//! `Activated(o, s, t, a{sv})`. Le bureau demande l'accord de l'utilisateur
//! et garde ses choix ; les touches proposées ne sont qu'une préférence.
//! Les raccourcis vivent tant que ce client tourne.
//!
//! Depuis la version 2 de l'interface (relevée sur xdg-desktop-portal 1.22) :
//! `ConfigureShortcuts(o, s, a{sv})` ouvre les réglages du bureau sur les
//! raccourcis de cette application, et le signal
//! `ShortcutsChanged(o, a(sa{sv}))` annonce les touches changées.

use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use futures_core::Stream;
use momentum_core::hotkeys::HotkeyAction;
use momentum_core::i18n::tr;
use zbus::{Connection, Proxy};
use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant as Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// Identifiant de l'action « panneau rapide », propre à ce client.
pub const PANEL: &str = "panneau";

/// Ce qu'un appui demande à la fenêtre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pressed {
    Action(HotkeyAction),
    Panel,
}

/// Raccourci déclaré, tel que le bureau le décrit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub description: String,
    /// Touches choisies dans le bureau, dans ses mots ; vide : aucune.
    pub trigger: String,
}

/// Session ouverte auprès du portail ; sert à ouvrir la configuration.
#[derive(Clone)]
pub struct Session {
    connection: Connection,
    path: OwnedObjectPath,
    version: u32,
}

impl Session {
    /// `ConfigureShortcuts` existe depuis la version 2 de l'interface.
    pub fn can_configure(&self) -> bool {
        self.version >= 2
    }

    /// Ouvre les raccourcis de cette application dans les réglages du
    /// bureau. Bloquant : à appeler hors du fil de l'interface.
    pub fn configure(&self) -> zbus::Result<()> {
        zbus::block_on(async {
            let portal = Proxy::new(&self.connection, PORTAL, PATH, INTERFACE).await?;
            let options: HashMap<&str, Value> = HashMap::new();
            portal
                .call_method("ConfigureShortcuts", &(&self.path, "", options))
                .await?;
            Ok(())
        })
    }
}

/// Ce que le fil des raccourcis annonce à la fenêtre.
pub enum Report {
    Pressed(Pressed),
    /// Raccourcis acceptés par le bureau.
    Ready(Session),
    /// Liste à jour, avec les touches choisies.
    Bound(Vec<Binding>),
}

type Declared = Vec<(String, HashMap<String, OwnedValue>)>;

/// Une ligne de la liste : la description du bureau, sinon l'identifiant.
fn binding(id: &str, description: Option<String>, trigger: Option<String>) -> Binding {
    Binding {
        description: description
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| id.to_string()),
        trigger: trigger.unwrap_or_default().trim().to_string(),
    }
}

fn bindings(declared: Declared) -> Vec<Binding> {
    declared
        .into_iter()
        .map(|(id, properties)| {
            let text = |key: &str| {
                properties
                    .get(key)
                    .and_then(|v| String::try_from(v.clone()).ok())
            };
            binding(&id, text("description"), text("trigger_description"))
        })
        .collect()
}

/// Raccourcis proposés : identifiant, description, touches préférées
/// (format de la spécification des raccourcis XDG).
fn proposals(scenes: &[String]) -> Vec<(String, String, Option<&'static str>)> {
    let mut list = vec![
        (
            HotkeyAction::CycleNoise.key(),
            HotkeyAction::CycleNoise.label(),
            Some("CTRL+ALT+n"),
        ),
        (
            HotkeyAction::ToggleProfile.key(),
            HotkeyAction::ToggleProfile.label(),
            Some("CTRL+ALT+p"),
        ),
        (
            HotkeyAction::OpenWindow.key(),
            HotkeyAction::OpenWindow.label(),
            Some("CTRL+ALT+m"),
        ),
        (
            PANEL.to_string(),
            tr("Panneau rapide").to_string(),
            Some("CTRL+ALT+o"),
        ),
    ];
    // Scènes : sans touche proposée, à choisir dans le bureau.
    list.extend(scenes.iter().map(|name| {
        let action = HotkeyAction::Scene(name.clone());
        (action.key(), action.label(), None)
    }));
    list
}

/// Identifiant reçu → action.
fn pressed(id: &str) -> Option<Pressed> {
    if id == PANEL {
        return Some(Pressed::Panel);
    }
    HotkeyAction::from_key(id).map(Pressed::Action)
}

/// Chemin de l'objet `Request` que le portail créera pour ce jeton
/// (règle documentée : nom unique sans « : », « . » remplacés par « _ »).
fn request_path(unique_name: &str, token: &str) -> String {
    let sender = unique_name.trim_start_matches(':').replace('.', "_");
    format!("{PATH}/request/{sender}/{token}")
}

/// Attend la réponse d'une demande au portail ; `None` si refusée.
async fn response(
    connection: &Connection,
    path: &str,
    call: impl std::future::Future<Output = zbus::Result<OwnedObjectPath>>,
) -> zbus::Result<Option<HashMap<String, OwnedValue>>> {
    // Abonnement avant l'appel : la réponse peut arriver tout de suite.
    let request = Proxy::new(connection, PORTAL, path, "org.freedesktop.portal.Request").await?;
    let mut answers = request.receive_signal("Response").await?;
    call.await?;
    let Some(message) = std::future::poll_fn(|cx| Pin::new(&mut answers).poll_next(cx)).await
    else {
        return Ok(None);
    };
    let (code, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize()?;
    Ok((code == 0).then_some(results))
}

/// Déclare les raccourcis puis appelle `on_pressed` à chaque appui, jusqu'à
/// la fin de la session D-Bus. À lancer dans un fil dédié.
pub fn run(app_id: &str, scenes: &[String], mut report: impl FnMut(Report)) -> zbus::Result<()> {
    zbus::block_on(async {
        let connection = Connection::session().await?;
        // Portails récents : rattache ce programme hors bac à sable à son
        // identifiant. Absent des portails plus anciens : sans conséquence.
        if let Ok(registry) = Proxy::new(
            &connection,
            PORTAL,
            PATH,
            "org.freedesktop.host.portal.Registry",
        )
        .await
        {
            let none: HashMap<&str, Value> = HashMap::new();
            if let Err(e) = registry.call_method("Register", &(app_id, none)).await {
                tracing::info!(%e, "registre du portail indisponible");
            }
        }
        let portal = Proxy::new(&connection, PORTAL, PATH, INTERFACE).await?;
        let version = portal.get_property::<u32>("version").await.unwrap_or(1);
        let unique = connection
            .unique_name()
            .map(|n| n.to_string())
            .unwrap_or_default();

        let token = "openheadset_session";
        let mut options: HashMap<&str, Value> = HashMap::new();
        options.insert("handle_token", Value::from(token));
        options.insert("session_handle_token", Value::from(token));
        let Some(results) = response(
            &connection,
            &request_path(&unique, token),
            portal.call("CreateSession", &(options,)),
        )
        .await?
        else {
            tracing::info!("session de raccourcis refusée par le bureau");
            return Ok(());
        };
        let session = results
            .get("session_handle")
            .and_then(|v| String::try_from(v.clone()).ok())
            .ok_or_else(|| zbus::Error::Failure("session_handle absent".into()))?;
        let session: OwnedObjectPath = zbus::ObjectPath::try_from(session.as_str())?.into();

        let list = proposals(scenes);
        let shortcuts: Vec<(&str, HashMap<&str, Value>)> = list
            .iter()
            .map(|(id, description, trigger)| {
                let mut props: HashMap<&str, Value> = HashMap::new();
                props.insert("description", Value::from(description.as_str()));
                if let Some(trigger) = trigger {
                    props.insert("preferred_trigger", Value::from(*trigger));
                }
                (id.as_str(), props)
            })
            .collect();
        let bind_token = "openheadset_bind";
        let mut options: HashMap<&str, Value> = HashMap::new();
        options.insert("handle_token", Value::from(bind_token));
        // Abonnements avant la déclaration : aucun signal n'est manqué.
        let mut activated = portal.receive_signal("Activated").await?;
        let mut changed = portal.receive_signal("ShortcutsChanged").await?;
        let Some(results) = response(
            &connection,
            &request_path(&unique, bind_token),
            portal.call("BindShortcuts", &(&session, shortcuts, "", options)),
        )
        .await?
        else {
            tracing::info!("raccourcis refusés par l'utilisateur ou le bureau");
            return Ok(());
        };
        tracing::info!("raccourcis déclarés au portail");
        report(Report::Ready(Session {
            connection: connection.clone(),
            path: session.clone(),
            version,
        }));
        // Sans liste dans la réponse : celle proposée, sans touches connues.
        let declared = results
            .get("shortcuts")
            .and_then(|v| Declared::try_from(v.clone()).ok())
            .map(bindings)
            .unwrap_or_else(|| {
                list.iter()
                    .map(|(id, description, _)| binding(id, Some(description.clone()), None))
                    .collect()
            });
        report(Report::Bound(declared));

        enum Signal {
            Activated(zbus::Message),
            Changed(zbus::Message),
        }
        loop {
            let next = std::future::poll_fn(|cx| {
                match Pin::new(&mut activated).poll_next(cx) {
                    Poll::Ready(Some(m)) => return Poll::Ready(Some(Signal::Activated(m))),
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Pending => {}
                }
                match Pin::new(&mut changed).poll_next(cx) {
                    Poll::Ready(Some(m)) => Poll::Ready(Some(Signal::Changed(m))),
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                }
            })
            .await;
            match next {
                None => break,
                Some(Signal::Activated(message)) => {
                    let Ok((from, id, _time, _options)) = message.body().deserialize::<(
                        OwnedObjectPath,
                        String,
                        u64,
                        HashMap<String, OwnedValue>,
                    )>() else {
                        continue;
                    };
                    if from != session {
                        continue;
                    }
                    tracing::info!(action = %id, "raccourci");
                    if let Some(action) = pressed(&id) {
                        report(Report::Pressed(action));
                    }
                }
                Some(Signal::Changed(message)) => {
                    let Ok((from, declared)) =
                        message.body().deserialize::<(OwnedObjectPath, Declared)>()
                    else {
                        continue;
                    };
                    if from == session {
                        report(Report::Bound(bindings(declared)));
                    }
                }
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_path_follows_the_portal_rule() {
        assert_eq!(
            request_path(":1.42", "t"),
            "/org/freedesktop/portal/desktop/request/1_42/t"
        );
    }

    #[test]
    fn every_proposal_maps_back_to_an_action() {
        let scenes = vec!["Train".to_string()];
        for (id, _, _) in proposals(&scenes) {
            assert!(pressed(&id).is_some(), "{id}");
        }
        assert_eq!(pressed(PANEL), Some(Pressed::Panel));
        assert_eq!(pressed("inconnu"), None);
    }

    #[test]
    fn a_binding_keeps_the_desktop_words_and_falls_back_to_the_identifier() {
        let full = binding(
            "bruit",
            Some("Mode suivant".into()),
            Some(" Ctrl+Alt+N ".into()),
        );
        assert_eq!(full.description, "Mode suivant");
        assert_eq!(full.trigger, "Ctrl+Alt+N");
        let bare = binding("scene:Train", Some("  ".into()), None);
        assert_eq!(bare.description, "scene:Train");
        assert_eq!(bare.trigger, "");
    }
}
