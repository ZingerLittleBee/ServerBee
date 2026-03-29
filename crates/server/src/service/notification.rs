use std::collections::HashMap;

use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{notification, notification_group};
use crate::error::AppError;

pub struct NotificationService;

/// Channel-specific configuration stored as JSON in `config_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChannelConfig {
    Webhook {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        body_template: Option<String>,
    },
    Telegram {
        bot_token: String,
        chat_id: String,
    },
    Bark {
        server_url: String,
        device_key: String,
    },
    Email {
        smtp_host: String,
        #[serde(default = "default_smtp_port")]
        smtp_port: u16,
        username: String,
        password: String,
        from: String,
        to: String,
    },
    Apns {
        key_id: String,
        team_id: String,
        private_key: String,
        bundle_id: String,
        #[serde(default)]
        sandbox: bool,
    },
}

fn default_method() -> String {
    "POST".to_string()
}

fn default_smtp_port() -> u16 {
    587
}

/// Template context for notification messages.
#[derive(Debug, Clone, Default)]
pub struct NotifyContext {
    pub server_name: String,
    pub server_id: String,
    pub rule_name: String,
    pub rule_id: String,
    pub event: String,
    pub message: String,
    pub time: String,
    pub cpu: String,
    pub memory: String,
}

impl NotifyContext {
    fn render(&self, template: &str) -> String {
        template
            .replace("{{server_name}}", &self.server_name)
            .replace("{{server_id}}", &self.server_id)
            .replace("{{rule_name}}", &self.rule_name)
            .replace("{{event}}", &self.event)
            .replace("{{message}}", &self.message)
            .replace("{{time}}", &self.time)
            .replace("{{cpu}}", &self.cpu)
            .replace("{{memory}}", &self.memory)
    }
}

const DEFAULT_TEMPLATE: &str =
    "[ServerBee] {{server_name}} {{event}}\n{{message}}\n时间: {{time}}";

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNotification {
    pub name: String,
    pub notify_type: String,
    pub config_json: serde_json::Value,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNotification {
    pub name: Option<String>,
    pub notify_type: Option<String>,
    pub config_json: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNotificationGroup {
    pub name: String,
    pub notification_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNotificationGroup {
    pub name: Option<String>,
    pub notification_ids: Option<Vec<String>>,
}

impl NotificationService {
    // ── Notification CRUD ──

    pub async fn list(db: &DatabaseConnection) -> Result<Vec<notification::Model>, AppError> {
        Ok(notification::Entity::find().all(db).await?)
    }

    pub async fn get(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<notification::Model, AppError> {
        notification::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Notification {id} not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateNotification,
    ) -> Result<notification::Model, AppError> {
        // Validate config
        let config_str = serde_json::to_string(&input.config_json)
            .map_err(|e| AppError::Validation(format!("Invalid config: {e}")))?;
        Self::parse_config(&input.notify_type, &config_str)?;

        let model = notification::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            notify_type: Set(input.notify_type),
            config_json: Set(config_str),
            enabled: Set(input.enabled),
            created_at: Set(Utc::now()),
        };
        Ok(model.insert(db).await?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateNotification,
    ) -> Result<notification::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: notification::ActiveModel = existing.into();

        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(notify_type) = input.notify_type {
            model.notify_type = Set(notify_type);
        }
        if let Some(config_json) = input.config_json {
            let config_str = serde_json::to_string(&config_json)
                .map_err(|e| AppError::Validation(format!("Invalid config: {e}")))?;
            model.config_json = Set(config_str);
        }
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }

        Ok(model.update(db).await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = notification::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Notification {id} not found")));
        }
        Ok(())
    }

    // ── Notification Group CRUD ──

    pub async fn list_groups(
        db: &DatabaseConnection,
    ) -> Result<Vec<notification_group::Model>, AppError> {
        Ok(notification_group::Entity::find().all(db).await?)
    }

    pub async fn get_group(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<notification_group::Model, AppError> {
        notification_group::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Notification group {id} not found")))
    }

    pub async fn create_group(
        db: &DatabaseConnection,
        input: CreateNotificationGroup,
    ) -> Result<notification_group::Model, AppError> {
        let ids_json = serde_json::to_string(&input.notification_ids)
            .map_err(|e| AppError::Validation(format!("Invalid notification_ids: {e}")))?;

        let model = notification_group::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            notification_ids_json: Set(ids_json),
            created_at: Set(Utc::now()),
        };
        Ok(model.insert(db).await?)
    }

    pub async fn update_group(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateNotificationGroup,
    ) -> Result<notification_group::Model, AppError> {
        let existing = Self::get_group(db, id).await?;
        let mut model: notification_group::ActiveModel = existing.into();

        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(notification_ids) = input.notification_ids {
            let ids_json = serde_json::to_string(&notification_ids)
                .map_err(|e| AppError::Validation(format!("Invalid notification_ids: {e}")))?;
            model.notification_ids_json = Set(ids_json);
        }

        Ok(model.update(db).await?)
    }

    pub async fn delete_group(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = notification_group::Entity::delete_by_id(id)
            .exec(db)
            .await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!(
                "Notification group {id} not found"
            )));
        }
        Ok(())
    }

    // ── Dispatch ──

    /// Send notifications for a group, given a template context.
    pub async fn send_group(
        db: &DatabaseConnection,
        group_id: &str,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
        let group = Self::get_group(db, group_id).await?;
        let ids: Vec<String> = serde_json::from_str(&group.notification_ids_json)
            .unwrap_or_default();

        for nid in ids {
            match Self::get(db, &nid).await {
                Ok(n) if n.enabled => {
                    if let Err(e) = Self::dispatch(db, &n, ctx).await {
                        tracing::error!(
                            "Failed to send notification {} ({}): {e}",
                            n.name,
                            n.notify_type
                        );
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Send a single notification (used for testing).
    pub async fn test_notification(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<(), AppError> {
        let n = Self::get(db, id).await?;
        let ctx = NotifyContext {
            server_name: "Test Server".to_string(),
            server_id: "test-server-id".to_string(),
            rule_id: "test-rule-id".to_string(),
            rule_name: "Test Rule".to_string(),
            event: "triggered".to_string(),
            message: "This is a test notification from ServerBee.".to_string(),
            time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            cpu: "50.0%".to_string(),
            memory: "60.0%".to_string(),
        };
        Self::dispatch(db, &n, &ctx).await
    }

    async fn dispatch(
        db: &DatabaseConnection,
        n: &notification::Model,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
        let config = Self::parse_config(&n.notify_type, &n.config_json)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

        match config {
            ChannelConfig::Webhook {
                url,
                method,
                headers,
                body_template,
            } => {
                let template = body_template.as_deref().unwrap_or(DEFAULT_TEMPLATE);
                let body = ctx.render(template);

                let mut req = match method.to_uppercase().as_str() {
                    "GET" => client.get(&url),
                    "PUT" => client.put(&url),
                    _ => client.post(&url),
                };

                for (k, v) in &headers {
                    req = req.header(k.as_str(), v.as_str());
                }

                // If no content-type header, default to application/json
                if !headers.keys().any(|k| k.eq_ignore_ascii_case("content-type")) {
                    req = req.header("Content-Type", "application/json");
                }

                let resp = req.body(body).send().await.map_err(|e| {
                    AppError::Internal(format!("Webhook request failed: {e}"))
                })?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return Err(AppError::Internal(format!(
                        "Webhook returned {status}: {text}"
                    )));
                }
            }
            ChannelConfig::Telegram { bot_token, chat_id } => {
                let text = ctx.render(DEFAULT_TEMPLATE);
                let url = format!(
                    "https://api.telegram.org/bot{bot_token}/sendMessage"
                );
                let resp = client
                    .post(&url)
                    .json(&serde_json::json!({
                        "chat_id": chat_id,
                        "text": text,
                        "parse_mode": "HTML",
                    }))
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("Telegram request failed: {e}")))?;

                if !resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(AppError::Internal(format!(
                        "Telegram API error: {text}"
                    )));
                }
            }
            ChannelConfig::Bark {
                server_url,
                device_key,
            } => {
                let title = format!("[ServerBee] {} {}", ctx.server_name, ctx.event);
                let body = ctx.render("{{message}}\n时间: {{time}}");
                let url = format!(
                    "{}/{}/{}/{}",
                    server_url.trim_end_matches('/'),
                    device_key,
                    urlencoding(&title),
                    urlencoding(&body),
                );
                let resp = client.get(&url).send().await.map_err(|e| {
                    AppError::Internal(format!("Bark request failed: {e}"))
                })?;

                if !resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(AppError::Internal(format!("Bark error: {text}")));
                }
            }
            ChannelConfig::Email {
                smtp_host,
                smtp_port,
                username,
                password,
                from,
                to,
            } => {
                use lettre::message::header::ContentType;
                use lettre::transport::smtp::authentication::Credentials;
                use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

                let subject = format!("[ServerBee] {} {}", ctx.server_name, ctx.event);
                let body = ctx.render(DEFAULT_TEMPLATE);

                let email = Message::builder()
                    .from(from.parse().map_err(|e| {
                        AppError::Validation(format!("Invalid from address: {e}"))
                    })?)
                    .to(to.parse().map_err(|e| {
                        AppError::Validation(format!("Invalid to address: {e}"))
                    })?)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(body)
                    .map_err(|e| AppError::Internal(format!("Failed to build email: {e}")))?;

                let creds = Credentials::new(username, password);

                let mailer =
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_host)
                        .map_err(|e| {
                            AppError::Internal(format!("SMTP connection error: {e}"))
                        })?
                        .port(smtp_port)
                        .credentials(creds)
                        .build();

                mailer.send(email).await.map_err(|e| {
                    AppError::Internal(format!("Failed to send email: {e}"))
                })?;
            }
            ChannelConfig::Apns {
                key_id,
                team_id,
                private_key,
                bundle_id,
                sandbox,
            } => {
                let title = format!("[ServerBee] {} {}", ctx.server_name, ctx.event);
                let body = ctx.render("{{message}}\nTime: {{time}}");

                let apns_config = crate::service::apns::ApnsConfig {
                    key_id: &key_id,
                    team_id: &team_id,
                    private_key: &private_key,
                    bundle_id: &bundle_id,
                    sandbox,
                };
                crate::service::apns::ApnsService::send_push(
                    db,
                    &apns_config,
                    &title,
                    &body,
                    Some(&ctx.server_id),
                    Some(&ctx.rule_id),
                )
                .await?;
            }
        }

        tracing::info!("Notification sent: {} ({})", n.name, n.notify_type);
        Ok(())
    }

    fn parse_config(notify_type: &str, config_json: &str) -> Result<ChannelConfig, AppError> {
        // Prepend the type tag so serde can deserialize the tagged enum
        let mut val: serde_json::Value = serde_json::from_str(config_json)
            .map_err(|e| AppError::Validation(format!("Invalid config JSON: {e}")))?;

        if let Some(obj) = val.as_object_mut() {
            obj.insert("type".to_string(), serde_json::Value::String(notify_type.to_string()));
        }

        serde_json::from_value(val)
            .map_err(|e| AppError::Validation(format!("Invalid {notify_type} config: {e}")))
    }
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── T3-1: Template substitution ──

    #[test]
    fn test_template_substitution_all_variables() {
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            server_id: "srv-abc-123".to_string(),
            rule_name: "High CPU".to_string(),
            rule_id: "rule-abc-123".to_string(),
            event: "triggered".to_string(),
            message: "CPU exceeded 90%".to_string(),
            time: "2026-03-13 12:00:00 UTC".to_string(),
            cpu: "92.5%".to_string(),
            memory: "78.3%".to_string(),
        };

        let template = "Server: {{server_name}} ({{server_id}}), Rule: {{rule_name}}, Event: {{event}}, Msg: {{message}}, Time: {{time}}, CPU: {{cpu}}, Mem: {{memory}}";
        let rendered = ctx.render(template);

        assert_eq!(
            rendered,
            "Server: web-01 (srv-abc-123), Rule: High CPU, Event: triggered, Msg: CPU exceeded 90%, Time: 2026-03-13 12:00:00 UTC, CPU: 92.5%, Mem: 78.3%"
        );
    }

    #[test]
    fn test_template_substitution_default_template() {
        let ctx = NotifyContext {
            server_name: "db-server".to_string(),
            server_id: "id-1".to_string(),
            rule_name: "Disk Full".to_string(),
            event: "triggered".to_string(),
            message: "Disk usage above 95%".to_string(),
            time: "2026-03-13 08:30:00 UTC".to_string(),
            ..Default::default()
        };

        let rendered = ctx.render(DEFAULT_TEMPLATE);

        assert!(
            rendered.contains("db-server"),
            "rendered template should contain server name"
        );
        assert!(
            rendered.contains("triggered"),
            "rendered template should contain event"
        );
        assert!(
            rendered.contains("Disk usage above 95%"),
            "rendered template should contain message"
        );
        assert!(
            rendered.contains("2026-03-13 08:30:00 UTC"),
            "rendered template should contain time"
        );
    }

    #[test]
    fn test_template_substitution_no_placeholders() {
        let ctx = NotifyContext::default();
        let template = "Static text with no variables.";
        let rendered = ctx.render(template);
        assert_eq!(rendered, "Static text with no variables.");
    }

    #[test]
    fn test_template_substitution_empty_context() {
        let ctx = NotifyContext::default();
        let rendered = ctx.render("Name: {{server_name}}, ID: {{server_id}}");
        assert_eq!(rendered, "Name: , ID: ");
    }

    // ── T3-2: Webhook payload format (parse_config) ──

    #[test]
    fn test_parse_config_webhook() {
        let config_json = r#"{"url": "https://example.com/hook", "method": "POST"}"#;
        let config =
            NotificationService::parse_config("webhook", config_json).expect("should parse");

        match config {
            ChannelConfig::Webhook { url, method, .. } => {
                assert_eq!(url, "https://example.com/hook");
                assert_eq!(method, "POST");
            }
            _ => panic!("expected Webhook variant"),
        }
    }

    #[test]
    fn test_parse_config_webhook_with_body_template() {
        let config_json = r#"{
            "url": "https://hooks.slack.com/services/xxx",
            "body_template": "{\"text\": \"{{server_name}} {{event}}\"}"
        }"#;
        let config =
            NotificationService::parse_config("webhook", config_json).expect("should parse");

        match config {
            ChannelConfig::Webhook {
                body_template, url, ..
            } => {
                assert_eq!(url, "https://hooks.slack.com/services/xxx");
                assert!(body_template.is_some());
                assert!(body_template.unwrap().contains("{{server_name}}"));
            }
            _ => panic!("expected Webhook variant"),
        }
    }

    #[test]
    fn test_parse_config_telegram() {
        let config_json = r#"{"bot_token": "123:ABC", "chat_id": "-1001234"}"#;
        let config =
            NotificationService::parse_config("telegram", config_json).expect("should parse");

        match config {
            ChannelConfig::Telegram { bot_token, chat_id } => {
                assert_eq!(bot_token, "123:ABC");
                assert_eq!(chat_id, "-1001234");
            }
            _ => panic!("expected Telegram variant"),
        }
    }

    #[test]
    fn test_parse_config_bark() {
        let config_json = r#"{"server_url": "https://bark.example.com", "device_key": "mykey"}"#;
        let config =
            NotificationService::parse_config("bark", config_json).expect("should parse");

        match config {
            ChannelConfig::Bark {
                server_url,
                device_key,
            } => {
                assert_eq!(server_url, "https://bark.example.com");
                assert_eq!(device_key, "mykey");
            }
            _ => panic!("expected Bark variant"),
        }
    }

    #[test]
    fn test_parse_config_email() {
        let config_json = r#"{
            "smtp_host": "smtp.gmail.com",
            "smtp_port": 587,
            "username": "user@gmail.com",
            "password": "secret",
            "from": "user@gmail.com",
            "to": "admin@example.com"
        }"#;
        let config =
            NotificationService::parse_config("email", config_json).expect("should parse");

        match config {
            ChannelConfig::Email {
                smtp_host,
                smtp_port,
                from,
                to,
                ..
            } => {
                assert_eq!(smtp_host, "smtp.gmail.com");
                assert_eq!(smtp_port, 587);
                assert_eq!(from, "user@gmail.com");
                assert_eq!(to, "admin@example.com");
            }
            _ => panic!("expected Email variant"),
        }
    }

    #[test]
    fn test_parse_config_email_default_port() {
        let config_json = r#"{
            "smtp_host": "smtp.example.com",
            "username": "u",
            "password": "p",
            "from": "a@b.com",
            "to": "c@d.com"
        }"#;
        let config =
            NotificationService::parse_config("email", config_json).expect("should parse");

        match config {
            ChannelConfig::Email { smtp_port, .. } => {
                assert_eq!(smtp_port, 587, "default SMTP port should be 587");
            }
            _ => panic!("expected Email variant"),
        }
    }

    #[test]
    fn test_parse_config_apns() {
        let config_json = r#"{
            "key_id": "ABC123DEFG",
            "team_id": "TEAM999888",
            "private_key": "-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----",
            "bundle_id": "com.example.serverbee",
            "sandbox": true
        }"#;
        let config =
            NotificationService::parse_config("apns", config_json).expect("should parse");

        match config {
            ChannelConfig::Apns {
                key_id,
                team_id,
                bundle_id,
                sandbox,
                ..
            } => {
                assert_eq!(key_id, "ABC123DEFG");
                assert_eq!(team_id, "TEAM999888");
                assert_eq!(bundle_id, "com.example.serverbee");
                assert!(sandbox);
            }
            _ => panic!("expected Apns variant"),
        }
    }

    #[test]
    fn test_parse_config_apns_default_sandbox() {
        let config_json = r#"{
            "key_id": "K",
            "team_id": "T",
            "private_key": "pk",
            "bundle_id": "com.example.app"
        }"#;
        let config =
            NotificationService::parse_config("apns", config_json).expect("should parse");

        match config {
            ChannelConfig::Apns { sandbox, .. } => {
                assert!(!sandbox, "sandbox should default to false");
            }
            _ => panic!("expected Apns variant"),
        }
    }

    #[test]
    fn test_parse_config_invalid_json() {
        let result = NotificationService::parse_config("webhook", "not json");
        assert!(result.is_err(), "invalid JSON should return error");
    }

    #[test]
    fn test_parse_config_missing_required_fields() {
        // Webhook requires `url`
        let result = NotificationService::parse_config("webhook", r#"{"method": "GET"}"#);
        assert!(
            result.is_err(),
            "missing required field should return error"
        );
    }

    // ── T3-3: URL encoding ──

    #[test]
    fn test_urlencoding_plain_ascii() {
        assert_eq!(urlencoding("hello"), "hello");
        assert_eq!(urlencoding("test-value_1.0~ok"), "test-value_1.0~ok");
    }

    #[test]
    fn test_urlencoding_special_characters() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencoding("100%"), "100%25");
    }

    #[test]
    fn test_urlencoding_empty_string() {
        assert_eq!(urlencoding(""), "");
    }

    // ── T3-4: ChannelConfig serialization round-trip ──

    #[test]
    fn test_channel_config_webhook_roundtrip() {
        let config = ChannelConfig::Webhook {
            url: "https://example.com".to_string(),
            method: "POST".to_string(),
            headers: HashMap::from([("Authorization".to_string(), "Bearer token".to_string())]),
            body_template: Some("{{message}}".to_string()),
        };

        let json = serde_json::to_string(&config).expect("serialize");
        let parsed: ChannelConfig = serde_json::from_str(&json).expect("deserialize");

        match parsed {
            ChannelConfig::Webhook { url, method, headers, body_template } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(method, "POST");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer token");
                assert_eq!(body_template.as_deref(), Some("{{message}}"));
            }
            _ => panic!("expected Webhook"),
        }
    }
}
