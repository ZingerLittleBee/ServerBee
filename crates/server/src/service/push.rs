use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::mobile_push_delivery;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

#[derive(Debug, Serialize)]
struct ExpoPushMessage {
    to: String,
    title: String,
    body: String,
    data: serde_json::Value,
    sound: String,
    priority: String,
    channel_id: String,
}

#[derive(Debug, Deserialize)]
struct ExpoPushResponse {
    data: Vec<ExpoPushTicket>,
}

#[derive(Debug, Deserialize)]
struct ExpoPushTicket {
    status: String,
    id: Option<String>,
    message: Option<String>,
}

pub struct PushService;

impl PushService {
    /// Send push notification to a list of Expo push tokens.
    /// Also records delivery entries for receipt verification.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_alert_push(
        db: &DatabaseConnection,
        device_tokens: &[(String, String)], // (device_registration_id, push_token)
        server_name: &str,
        rule_name: &str,
        rule_id: &str,
        server_id: &str,
        status: &str,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if device_tokens.is_empty() {
            return Ok(());
        }

        let alert_key = format!("{rule_id}:{server_id}");
        let title = if status == "firing" {
            format!("🔴 {server_name}: {rule_name}")
        } else {
            format!("✅ {server_name}: {rule_name} resolved")
        };

        let data = serde_json::json!({
            "target_type": "alert_event",
            "alert_key": alert_key,
            "rule_id": rule_id,
            "server_id": server_id,
            "status": status,
            "event_at": Utc::now().to_rfc3339(),
        });

        let messages: Vec<ExpoPushMessage> = device_tokens
            .iter()
            .map(|(_, token)| ExpoPushMessage {
                to: token.clone(),
                title: title.clone(),
                body: message.to_string(),
                data: data.clone(),
                sound: "default".to_string(),
                priority: "high".to_string(),
                channel_id: "alerts".to_string(),
            })
            .collect();

        // Expo supports batch send (up to 100 per request)
        for (chunk_idx, chunk) in messages.chunks(100).enumerate() {
            let client = reqwest::Client::new();
            let sent_at = Utc::now();
            match client
                .post(EXPO_PUSH_URL)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .json(&chunk)
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        tracing::debug!("Expo push sent to {} devices", chunk.len());
                        // Parse response and store delivery records
                        if let Ok(push_resp) = resp.json::<ExpoPushResponse>().await {
                            let start = chunk_idx * 100;
                            for (i, ticket) in push_resp.data.iter().enumerate() {
                                if let Some((device_reg_id, _)) =
                                    device_tokens.get(start + i)
                                {
                                    let delivery = mobile_push_delivery::ActiveModel {
                                        id: Set(Uuid::new_v4().to_string()),
                                        device_registration_id: Set(
                                            device_reg_id.clone(),
                                        ),
                                        ticket_id: Set(ticket.id.clone()),
                                        ticket_status: Set(ticket.status.clone()),
                                        receipt_status: Set(None),
                                        receipt_message: Set(ticket.message.clone()),
                                        alert_key: Set(alert_key.clone()),
                                        alert_status: Set(status.to_string()),
                                        sent_at: Set(sent_at),
                                        receipt_checked_at: Set(None),
                                    };
                                    if let Err(e) =
                                        mobile_push_delivery::Entity::insert(delivery)
                                            .exec(db)
                                            .await
                                    {
                                        tracing::error!(
                                            "Failed to insert push delivery record: {e}"
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        tracing::error!("Expo push API returned {}", resp.status());
                    }
                }
                Err(e) => {
                    tracing::error!("Expo push request failed: {e}");
                }
            }
        }

        Ok(())
    }
}
