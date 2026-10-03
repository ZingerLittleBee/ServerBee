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

/// Only the external Apple transport is replaceable. Recipient selection,
/// pre-send migration checks, payload construction and cleanup stay real.
#[async_trait::async_trait]
pub trait LegacyApnsTransport: Send + Sync {
    async fn send(
        &self,
        payload: a2::request::payload::Payload<'_>,
    ) -> Result<a2::Response, a2::Error>;
}

#[async_trait::async_trait]
impl LegacyApnsTransport for a2::Client {
    async fn send(
        &self,
        payload: a2::request::payload::Payload<'_>,
    ) -> Result<a2::Response, a2::Error> {
        a2::Client::send(self, payload).await
    }
}

struct LegacyNotification<'a> {
    title: &'a str,
    body: &'a str,
    server_id: Option<&'a str>,
    rule_id: Option<&'a str>,
}

pub struct ApnsService;

impl ApnsService {
    /// Both selection and migration are scoped to the legacy row owner. A
    /// forged installation from another account cannot silence that owner.
    pub async fn legacy_recipients(
        db: &DatabaseConnection,
    ) -> Result<Vec<device_token::Model>, AppError> {
        Ok(device_token::Entity::find().from_raw_sql(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT d.* FROM device_tokens d WHERE NOT EXISTS (SELECT 1 FROM mobile_push_migrations m WHERE m.installation_id=d.installation_id AND m.user_id=d.user_id) ORDER BY d.created_at, d.id".to_owned(),
        )).all(db).await?)
    }

    /// Send a push notification to all registered device tokens.
    ///
    /// Removes terminal 410 Unregistered tokens only if their snapshot is current.
    pub async fn send_push(
        db: &DatabaseConnection,
        config: &ApnsConfig<'_>,
        title: &str,
        body: &str,
        server_id: Option<&str>,
        rule_id: Option<&str>,
    ) -> Result<(), AppError> {
        let tokens = Self::legacy_recipients(db).await?;

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

        Self::dispatch(
            db,
            config,
            tokens,
            LegacyNotification {
                title,
                body,
                server_id,
                rule_id,
            },
            &client,
        )
        .await
    }

    /// Production dispatch with only outbound APNs substituted by callers that
    /// cannot use Apple services (the integration harness).
    pub async fn send_push_with_transport(
        db: &DatabaseConnection,
        config: &ApnsConfig<'_>,
        title: &str,
        body: &str,
        server_id: Option<&str>,
        rule_id: Option<&str>,
        transport: &dyn LegacyApnsTransport,
    ) -> Result<(), AppError> {
        let tokens = Self::legacy_recipients(db).await?;
        Self::dispatch(
            db,
            config,
            tokens,
            LegacyNotification {
                title,
                body,
                server_id,
                rule_id,
            },
            transport,
        )
        .await
    }

    async fn dispatch(
        db: &DatabaseConnection,
        config: &ApnsConfig<'_>,
        tokens: Vec<device_token::Model>,
        notification: LegacyNotification<'_>,
        transport: &dyn LegacyApnsTransport,
    ) -> Result<(), AppError> {
        let mut sent = 0u32;
        for dt in &tokens {
            // Recheck each cached recipient immediately before entering the
            // external request. Earlier sends may have awaited while this
            // installation migrated, refreshed its token or logged out.
            let eligible = device_token::Entity::find().from_raw_sql(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "SELECT d.* FROM device_tokens d WHERE d.id=? AND d.user_id=? AND d.mobile_session_id=? AND d.token=? AND d.updated_at=? AND NOT EXISTS (SELECT 1 FROM mobile_push_migrations m WHERE m.installation_id=d.installation_id AND m.user_id=d.user_id)",
                [dt.id.clone().into(), dt.user_id.clone().into(), dt.mobile_session_id.clone().into(), dt.token.clone().into(), dt.updated_at.into()],
            )).one(db).await?;
            if eligible.is_none() {
                continue;
            }

            let builder = DefaultNotificationBuilder::new()
                .set_title(notification.title)
                .set_body(notification.body)
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
            if let Some(sid) = notification.server_id {
                let _ = payload.add_custom_data("server_id", &sid);
            }
            if let Some(rid) = notification.rule_id {
                let _ = payload.add_custom_data("rule_id", &rid);
            }

            match transport.send(payload).await {
                Ok(_response) => {
                    sent += 1;
                }
                Err(a2::Error::ResponseError(response)) => {
                    if response.code == 410
                        && response
                            .error
                            .as_ref()
                            .is_some_and(|e| e.reason == a2::ErrorReason::Unregistered)
                    {
                        tracing::warn!(
                            "APNs token invalid for device {} (HTTP {}), removing",
                            dt.installation_id,
                            response.code
                        );
                        let _ = device_token::Entity::delete_many()
                            .filter(device_token::Column::Id.eq(&dt.id))
                            .filter(device_token::Column::Token.eq(&dt.token))
                            .filter(device_token::Column::UpdatedAt.eq(dt.updated_at))
                            .exec(db)
                            .await;
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

        assert!(
            result.is_ok(),
            "empty table + None args should still return Ok"
        );
    }

    #[tokio::test]
    async fn test_send_push_garbage_key_production_errors() {
        // A seeded token forces client creation; a garbage key makes a2::Client::token fail (Production endpoint).
        let (db, _tmp) = setup_test_db().await;
        seed_token(&db, "t1").await;
        let config = garbage_config(false);

        let err = ApnsService::send_push(&db, &config, "Title", "Body", Some("srv"), Some("rule"))
            .await
            .expect_err("garbage private key must fail client creation");

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
            .expect_err("garbage private key must fail client creation in sandbox mode");

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
