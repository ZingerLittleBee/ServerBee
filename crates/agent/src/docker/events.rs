use std::collections::HashMap;

use bollard::Docker;
use bollard::system::EventsOptions;
use futures_util::StreamExt;
use serverbee_common::docker_types::DockerEventInfo;
use serverbee_common::protocol::AgentMessage;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

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

        let info = DockerEventInfo {
            timestamp,
            event_type,
            action,
            actor_id,
            actor_name,
            attributes,
        };

        let msg = AgentMessage::DockerEvent { event: info };
        if agent_tx.send(msg).await.is_err() {
            tracing::debug!("Event stream: agent channel closed");
            break;
        }
    }

    Ok(())
}
