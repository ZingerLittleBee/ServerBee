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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use chrono::{TimeZone, Utc};

    /// Build a config that always fails client creation (garbage PEM key).
    fn garbage_config(sandbox: bool) -> ApnsConfig<'static> {
        ApnsConfig {
            key_id: "ABC123DEFG",
            team_id: "TEAM123456",
            private_key: "not-a-valid-p8-private-key",
            bundle_id: "com.example.app",
            sandbox,
        }
    }

    /// Seed one device token row with fixed timestamps. `device_tokens` has
    /// NOT NULL FKs to `users` and `mobile_sessions` (which itself FKs `users`),
    /// so the parent rows are seeded first via idempotent inserts.
    async fn seed_token(db: &DatabaseConnection, id: &str) {
        db.execute_unprepared(
            "INSERT OR IGNORE INTO users (id, username, password_hash, role, must_change_password, created_at, updated_at) \
             VALUES ('user-1', 'apns-user', 'x', 'admin', 0, '2026-01-02 03:04:05', '2026-01-02 03:04:05')",
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "INSERT OR IGNORE INTO mobile_sessions (id, user_id, refresh_token_hash, installation_id, device_name, created_at, expires_at, last_used_at) \
             VALUES ('session-1', 'user-1', 'hash', 'install-1', 'dev', '2026-01-02 03:04:05', '2027-01-02 03:04:05', '2026-01-02 03:04:05')",
        )
        .await
        .unwrap();

        let ts = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        device_token::ActiveModel {
            id: Set(id.to_string()),
            user_id: Set("user-1".to_string()),
            mobile_session_id: Set("session-1".to_string()),
            installation_id: Set(format!("install-{id}")),
            token: Set(format!("token-{id}")),
            created_at: Set(ts),
            updated_at: Set(ts),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_send_push_no_tokens_returns_ok_early() {
        // With an empty device_tokens table, send_push should short-circuit to Ok without touching APNs.
        let (db, _tmp) = setup_test_db().await;
        let config = garbage_config(false);

        let result =
            ApnsService::send_push(&db, &config, "Title", "Body", Some("srv-1"), Some("rule-1"))
                .await;

        assert!(
            result.is_ok(),
            "empty token table should return Ok early without creating a client"
        );
    }

    #[tokio::test]
    async fn test_send_push_no_tokens_with_none_args() {
        // The empty-table early return also holds when optional server_id/rule_id are None.
        let (db, _tmp) = setup_test_db().await;
        let config = garbage_config(true);

        let result = ApnsService::send_push(&db, &config, "T", "B", None, None).await;

        assert!(result.is_ok(), "empty table + None args should still return Ok");
    }

    #[tokio::test]
    async fn test_send_push_garbage_key_production_errors() {
        // A seeded token forces client creation; a garbage key makes a2::Client::token fail (Production endpoint).
        let (db, _tmp) = setup_test_db().await;
        seed_token(&db, "t1").await;
        let config = garbage_config(false);

        let err = ApnsService::send_push(&db, &config, "Title", "Body", Some("srv"), Some("rule"))
            .await
            .err()
            .expect("garbage private key must fail client creation");

        match err {
            AppError::Internal(msg) => {
                assert!(
                    msg.contains("Failed to create APNs client"),
                    "error should come from the client-creation map_err branch, got: {msg}"
                );
            }
            other => panic!("expected AppError::Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_send_push_garbage_key_sandbox_errors() {
        // Same failure path but with sandbox=true exercises the Endpoint::Sandbox branch.
        let (db, _tmp) = setup_test_db().await;
        seed_token(&db, "t2").await;
        let config = garbage_config(true);

        let err = ApnsService::send_push(&db, &config, "T", "B", None, None)
            .await
            .err()
            .expect("garbage private key must fail client creation in sandbox mode");

        assert!(
            matches!(err, AppError::Internal(_)),
            "sandbox client creation with garbage key should yield AppError::Internal"
        );
    }

    #[tokio::test]
    async fn test_send_push_does_not_remove_token_on_client_error() {
        // When client creation fails, no token deletion should occur (deletion only happens inside the send loop).
        let (db, _tmp) = setup_test_db().await;
        seed_token(&db, "t3").await;
        let config = garbage_config(false);

        let _ = ApnsService::send_push(&db, &config, "T", "B", None, None).await;

        let remaining = device_token::Entity::find().all(&db).await.unwrap();
        assert_eq!(
            remaining.len(),
            1,
            "token must remain since the failure happens before the send/delete loop"
        );
    }
}
