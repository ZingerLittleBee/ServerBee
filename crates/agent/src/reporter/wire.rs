//! Outbound wire helper: one place that owns "agent message → JSON text
//! frame" so handlers never touch serde or the WS sink shape directly.

use futures_util::SinkExt;
use serverbee_common::protocol::AgentMessage;
use tokio_tungstenite::tungstenite::Message;

/// Serialize an agent message and send it as a WS text frame.
pub(crate) async fn send_msg<S>(write: &mut S, msg: &AgentMessage) -> anyhow::Result<()>
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let json = serde_json::to_string(msg)?;
    write.send(Message::Text(json.into())).await?;
    Ok(())
}
