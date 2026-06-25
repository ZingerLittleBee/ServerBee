use std::collections::HashMap;

use bollard::Docker;
use bollard::models::EventMessage;
use bollard::system::EventsOptions;
use futures_util::StreamExt;
use serverbee_common::docker_types::DockerEventInfo;
use serverbee_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Pure mapping from a bollard `EventMessage` to our `DockerEventInfo` DTO.
/// Extracted so the transformation can be unit-tested without a live daemon.
fn map_event(event: EventMessage) -> DockerEventInfo {
    let event_type = event
        .typ
        .map(|t| format!("{t:?}").to_lowercase())
        .unwrap_or_else(|| "unknown".into());

    let action = event.action.unwrap_or_default();

    let (actor_id, actor_name, attributes) = match event.actor {
        Some(actor) => {
            let id = actor.id.unwrap_or_default();
            let attrs = actor.attributes.unwrap_or_default();
            let name = attrs.get("name").cloned();
            (id, name, attrs)
        }
        None => (String::new(), None, HashMap::new()),
    };

    let timestamp = event.time.unwrap_or(0);

    DockerEventInfo {
        timestamp,
        event_type,
        action,
        actor_id,
        actor_name,
        attributes,
    }
}

/// Spawn a background task that streams Docker events with auto-reconnect.
pub fn spawn_event_stream(docker: Docker, agent_tx: mpsc::Sender<AgentMessage>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tracing::debug!("Starting Docker event stream...");
            match run_event_stream(&docker, &agent_tx).await {
                Ok(()) => {
                    tracing::debug!("Docker event stream ended normally");
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        "Docker event stream error: {e}, reconnecting in {RECONNECT_DELAY:?}..."
                    );
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
        }
    })
}

async fn run_event_stream(
    docker: &Docker,
    agent_tx: &mpsc::Sender<AgentMessage>,
) -> anyhow::Result<()> {
    let options = EventsOptions::<String> {
        ..Default::default()
    };

    let mut stream = docker.events(Some(options));

    while let Some(event_result) = stream.next().await {
        let event = event_result?;
        let info = map_event(event);

        let msg = AgentMessage::DockerEvent { event: info };
        if agent_tx.send(msg).await.is_err() {
            tracing::debug!("Event stream: agent channel closed");
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::{EventActor, EventMessageTypeEnum};

    #[test]
    fn map_event_defaults_when_all_fields_missing() {
        let event = EventMessage::default();
        let info = map_event(event);
        assert_eq!(info.timestamp, 0);
        assert_eq!(info.event_type, "unknown");
        assert_eq!(info.action, "");
        assert_eq!(info.actor_id, "");
        assert!(info.actor_name.is_none());
        assert!(info.attributes.is_empty());
    }

    #[test]
    fn map_event_extracts_type_action_and_actor() {
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), "web".to_string());
        attributes.insert("image".to_string(), "nginx".to_string());
        let event = EventMessage {
            typ: Some(EventMessageTypeEnum::CONTAINER),
            action: Some("start".to_string()),
            actor: Some(EventActor {
                id: Some("abc123".to_string()),
                attributes: Some(attributes),
            }),
            time: Some(1_700_000_000),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.timestamp, 1_700_000_000);
        // The enum Debug is lowercased: CONTAINER -> "container".
        assert_eq!(info.event_type, "container");
        assert_eq!(info.action, "start");
        assert_eq!(info.actor_id, "abc123");
        assert_eq!(info.actor_name.as_deref(), Some("web"));
        assert_eq!(info.attributes.get("image"), Some(&"nginx".to_string()));
    }

    #[test]
    fn map_event_actor_without_name_yields_none() {
        let event = EventMessage {
            typ: Some(EventMessageTypeEnum::NETWORK),
            action: Some("connect".to_string()),
            actor: Some(EventActor {
                id: Some("net1".to_string()),
                attributes: Some(HashMap::new()),
            }),
            time: Some(5),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.event_type, "network");
        assert_eq!(info.actor_id, "net1");
        assert!(info.actor_name.is_none());
    }

    #[test]
    fn map_event_empty_type_enum_lowercases_debug_name() {
        // EMPTY variant Debug-formats to "EMPTY" -> lowercased "empty" (not "").
        let event = EventMessage {
            typ: Some(EventMessageTypeEnum::EMPTY),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.event_type, "empty");
    }

    #[test]
    fn map_event_maps_volume_image_and_daemon_types() {
        // Each enum variant's Debug name is lowercased verbatim.
        for (variant, expected) in [
            (EventMessageTypeEnum::VOLUME, "volume"),
            (EventMessageTypeEnum::IMAGE, "image"),
            (EventMessageTypeEnum::DAEMON, "daemon"),
        ] {
            let event = EventMessage {
                typ: Some(variant),
                ..Default::default()
            };
            let info = map_event(event);
            assert_eq!(info.event_type, expected);
        }
    }

    #[test]
    fn map_event_missing_type_yields_unknown_but_keeps_other_fields() {
        // typ is None -> "unknown", while action/actor are still mapped.
        let event = EventMessage {
            typ: None,
            action: Some("destroy".to_string()),
            actor: Some(EventActor {
                id: Some("vol9".to_string()),
                attributes: None,
            }),
            time: Some(42),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.event_type, "unknown");
        assert_eq!(info.action, "destroy");
        assert_eq!(info.actor_id, "vol9");
        // attributes None -> empty map, so no name.
        assert!(info.actor_name.is_none());
        assert!(info.attributes.is_empty());
        assert_eq!(info.timestamp, 42);
    }

    #[test]
    fn map_event_actor_present_without_id_yields_empty_actor_id() {
        // actor.id None -> actor_id "" while attributes are still extracted.
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), "redis".to_string());
        let event = EventMessage {
            typ: Some(EventMessageTypeEnum::CONTAINER),
            action: Some("die".to_string()),
            actor: Some(EventActor {
                id: None,
                attributes: Some(attributes),
            }),
            time: Some(7),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.actor_id, "");
        assert_eq!(info.actor_name.as_deref(), Some("redis"));
        // The "name" attribute remains in the full attributes map too.
        assert_eq!(info.attributes.get("name"), Some(&"redis".to_string()));
    }

    #[test]
    fn map_event_absent_action_defaults_to_empty_string() {
        // action None -> "" via unwrap_or_default, regardless of type.
        let event = EventMessage {
            typ: Some(EventMessageTypeEnum::IMAGE),
            action: None,
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.event_type, "image");
        assert_eq!(info.action, "");
    }

    #[test]
    fn map_event_negative_timestamp_is_preserved() {
        // time is an i64 and is copied through verbatim, including negatives.
        let event = EventMessage {
            time: Some(-1),
            ..Default::default()
        };
        let info = map_event(event);
        assert_eq!(info.timestamp, -1);
    }
}
