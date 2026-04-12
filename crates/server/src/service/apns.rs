use a2::{
    ClientConfig, DefaultNotificationBuilder, Endpoint, NotificationBuilder, NotificationOptions,
    Priority,
};
use sea_orm::*;

use crate::entity::device_token;
use crate::error::AppError;

/// APNs credential bundle passed to [`ApnsService::send_push`].
pub struct ApnsConfig<'a> {
    pub key_id: &'a str,
    pub team_id: &'a str,
    pub private_key: &'a str,
    pub bundle_id: &'a str,
    pub sandbox: bool,
}

pub struct ApnsService;

impl ApnsService {
    /// Send a push notification to all registered device tokens.
    ///
    /// Automatically removes invalid tokens (410 Unregistered / 400 BadDeviceToken).
    pub async fn send_push(
        db: &DatabaseConnection,
        config: &ApnsConfig<'_>,
        title: &str,
        body: &str,
        server_id: Option<&str>,
        rule_id: Option<&str>,
    ) -> Result<(), AppError> {
        let tokens = device_token::Entity::find().all(db).await?;

        if tokens.is_empty() {
            tracing::debug!("No device tokens registered, skipping APNs push");
            return Ok(());
        }

        let endpoint = if config.sandbox {
            Endpoint::Sandbox
        } else {
            Endpoint::Production
        };

        let key_reader = std::io::Cursor::new(config.private_key.as_bytes());
        let client = a2::Client::token(
            key_reader,
            config.key_id,
            config.team_id,
            ClientConfig::new(endpoint),
        )
        .map_err(|e| AppError::Internal(format!("Failed to create APNs client: {e}")))?;

        let mut sent = 0u32;
        for dt in &tokens {
            let builder = DefaultNotificationBuilder::new()
                .set_title(title)
                .set_body(body)
                .set_sound("default")
                .set_badge(1);

            let mut payload = builder.build(
                &dt.token,
                NotificationOptions {
                    apns_topic: Some(config.bundle_id),
                    apns_priority: Some(Priority::High),
                    ..Default::default()
                },
            );

            // Add custom data for deep linking on iOS
            if let Some(sid) = server_id {
                let _ = payload.add_custom_data("server_id", &sid);
            }
            if let Some(rid) = rule_id {
                let _ = payload.add_custom_data("rule_id", &rid);
            }

            match client.send(payload).await {
                Ok(_response) => {
                    sent += 1;
                }
                Err(a2::Error::ResponseError(response)) => {
                    // 410 = Unregistered, 400 = BadDeviceToken → remove stale token
                    if response.code == 410 || response.code == 400 {
                        tracing::warn!(
                            "APNs token invalid for device {} (HTTP {}), removing",
                            dt.installation_id,
                            response.code
                        );
                        let _ = device_token::Entity::delete_by_id(&dt.id).exec(db).await;
                    } else {
                        tracing::error!(
                            "APNs rejected push for device {} (HTTP {}): {:?}",
                            dt.installation_id,
                            response.code,
                            response.error
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("APNs send failed for device {}: {e}", dt.installation_id);
                }
            }
        }

        tracing::info!("APNs push sent to {sent}/{} devices", tokens.len());
        Ok(())
    }
}
