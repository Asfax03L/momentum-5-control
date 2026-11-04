//! Raccourcis clavier globaux par le portail XDG `GlobalShortcuts`
//! (GNOME 48 et suivants, aussi servi par d'autres bureaux).
//!
//! Interface publique du portail : `CreateSession(a{sv}) -> o`,
//! `BindShortcuts(o, a(sa{sv}), s, a{sv}) -> o`, réponses par le signal
//! `Response(u, a{sv})` de l'objet `Request`, appuis par le signal
//! `Activated(o, s, t, a{sv})`. Le bureau demande l'accord de l'utilisateur
//! et garde ses choix ; les touches proposées ne sont qu'une préférence.
//! Les raccourcis vivent tant que ce client tourne.

use std::collections::HashMap;
use std::pin::Pin;

use futures_core::Stream;
use momentum_core::hotkeys::HotkeyAction;
use momentum_core::i18n::tr;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

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
pub fn run(
    app_id: &str,
    scenes: &[String],
    mut on_pressed: impl FnMut(Pressed),
) -> zbus::Result<()> {
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
        let session = zbus::zvariant::ObjectPath::try_from(session.as_str())?.into_owned();

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
        if response(
            &connection,
            &request_path(&unique, bind_token),
            portal.call("BindShortcuts", &(&session, shortcuts, "", options)),
        )
        .await?
        .is_none()
        {
            tracing::info!("raccourcis refusés par l'utilisateur ou le bureau");
            return Ok(());
        }
        tracing::info!("raccourcis déclarés au portail");

        let mut activated = portal.receive_signal("Activated").await?;
        while let Some(message) =
            std::future::poll_fn(|cx| Pin::new(&mut activated).poll_next(cx)).await
        {
            let Ok((from, id, _time, _options)) =
                message
                    .body()
                    .deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()
            else {
                continue;
            };
            if from != session {
                continue;
            }
            tracing::info!(action = %id, "raccourci");
            if let Some(action) = pressed(&id) {
                on_pressed(action);
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
}
