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
        from: String,
        to: Vec<String>,
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

const DEFAULT_TEMPLATE: &str = "[ServerBee] {{server_name}} {{event}}\n{{message}}\n时间: {{time}}";

const EMAIL_TEXT_TEMPLATE: &str =
    "[ServerBee] {{server_name}} {{event}}\n{{message}}\nTime: {{time}}";

fn is_plausible_email(s: &str) -> bool {
    let (local, domain) = match s.split_once('@') {
        Some(pair) => pair,
        None => return false,
    };
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

fn email_header_color(event: &str) -> &'static str {
    match event {
        "triggered" => "#ea580c",
        "resolved" | "recovered" => "#16a34a",
        _ => "#6b7280",
    }
}

fn render_html(ctx: &NotifyContext) -> String {
    let color = email_header_color(&ctx.event);
    let title = format!(
        "[ServerBee] {} {}",
        html_escape::encode_text(&ctx.server_name),
        html_escape::encode_text(&ctx.event),
    );

    let mut rows = String::new();
    let mut add_row = |label: &str, value: &str| {
        if value.is_empty() {
            return;
        }
        rows.push_str(&format!(
            "<tr><td style=\"padding:6px 12px;color:#6b7280;font-size:13px;width:110px\">{}</td>\
             <td style=\"padding:6px 12px;font-size:14px\">{}</td></tr>",
            label,
            html_escape::encode_text(value),
        ));
    };
    add_row("Server", &ctx.server_name);
    add_row("Rule", &ctx.rule_name);
    add_row("Event", &ctx.event);
    add_row("Time", &ctx.time);
    add_row("CPU", &ctx.cpu);
    add_row("Memory", &ctx.memory);
    add_row("Message", &ctx.message);

    format!(
        r#"<!DOCTYPE html>
<html><body style="margin:0;padding:24px;background:#f3f4f6;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif">
<table style="max-width:600px;margin:0 auto;background:#ffffff;border-radius:8px;overflow:hidden;border-collapse:collapse;width:100%">
<tr><td style="background:{color};color:#ffffff;padding:16px 20px;font-size:16px;font-weight:600">{title}</td></tr>
<tr><td style="padding:12px 8px"><table style="width:100%;border-collapse:collapse">{rows}</table></td></tr>
<tr><td style="padding:12px 20px;color:#9ca3af;font-size:12px;text-align:center">Sent by ServerBee</td></tr>
</table>
</body></html>"#,
        color = color,
        title = title,
        rows = rows,
    )
}

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

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<notification::Model, AppError> {
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

        let candidate_type = input
            .notify_type
            .clone()
            .unwrap_or_else(|| existing.notify_type.clone());
        let candidate_json = match &input.config_json {
            Some(cj) => serde_json::to_string(cj)
                .map_err(|e| AppError::Validation(format!("Invalid config: {e}")))?,
            None => existing.config_json.clone(),
        };
        Self::parse_config(&candidate_type, &candidate_json)?;

        let mut model: notification::ActiveModel = existing.into();
        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(notify_type) = input.notify_type {
            model.notify_type = Set(notify_type);
        }
        if input.config_json.is_some() {
            model.config_json = Set(candidate_json);
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
        config: &crate::config::AppConfig,
        group_id: &str,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
        let group = Self::get_group(db, group_id).await?;
        let ids: Vec<String> =
            serde_json::from_str(&group.notification_ids_json).unwrap_or_default();

        for nid in ids {
            match Self::get(db, &nid).await {
                Ok(n) if n.enabled => {
                    if let Err(e) = Self::dispatch(db, config, &n, ctx).await {
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
        config: &crate::config::AppConfig,
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
        Self::dispatch(db, config, &n, &ctx).await
    }

    async fn dispatch(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        n: &notification::Model,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
        let channel_config = Self::parse_config(&n.notify_type, &n.config_json)?;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::Internal(format!("HTTP client error: {e}")))?;

        match channel_config {
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
                if !headers
                    .keys()
                    .any(|k| k.eq_ignore_ascii_case("content-type"))
                {
                    req = req.header("Content-Type", "application/json");
                }

                let resp = req
                    .body(body)
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("Webhook request failed: {e}")))?;

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
                let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
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
                    return Err(AppError::Internal(format!("Telegram API error: {text}")));
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
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("Bark request failed: {e}")))?;

                if !resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(AppError::Internal(format!("Bark error: {text}")));
                }
            }
            ChannelConfig::Email { from, to } => {
                let api_key = config.resend.api_key.trim();
                if api_key.is_empty() {
                    return Err(AppError::Validation(
                        "Resend API key not configured (set SERVERBEE_RESEND__API_KEY)".to_string(),
                    ));
                }

                let subject = format!("[ServerBee] {} {}", ctx.server_name, ctx.event);
                let html_body = render_html(ctx);
                let text_body = ctx.render(EMAIL_TEXT_TEMPLATE);

                let body = serde_json::json!({
                    "from": from,
                    "to": to,
                    "subject": subject,
                    "html": html_body,
                    "text": text_body,
                });

                let resp = client
                    .post("https://api.resend.com/emails")
                    .bearer_auth(api_key)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| AppError::Internal(format!("Resend request failed: {e}")))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let raw = resp.text().await.unwrap_or_default();
                    let message = serde_json::from_str::<serde_json::Value>(&raw)
                        .ok()
                        .and_then(|v| {
                            v.get("message")
                                .and_then(|m| m.as_str())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| raw.clone());
                    return Err(AppError::Internal(format!(
                        "Resend API error ({status}): {message}"
                    )));
                }
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
            obj.insert(
                "type".to_string(),
                serde_json::Value::String(notify_type.to_string()),
            );
        }

        let config: ChannelConfig = serde_json::from_value(val)
            .map_err(|e| AppError::Validation(format!("Invalid {notify_type} config: {e}")))?;

        if let ChannelConfig::Email { from, to } = &config {
            if to.is_empty() {
                return Err(AppError::Validation(
                    "Email notification requires at least one 'to' address".to_string(),
                ));
            }
            if !is_plausible_email(from) {
                return Err(AppError::Validation(format!(
                    "Invalid 'from' address: {from}"
                )));
            }
            for addr in to {
                if !is_plausible_email(addr) {
                    return Err(AppError::Validation(format!(
                        "Invalid 'to' address: {addr}"
                    )));
                }
            }
        }

        Ok(config)
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
        let config = NotificationService::parse_config("bark", config_json).expect("should parse");

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
    fn test_parse_config_email_new_schema() {
        let config_json = r#"{"from":"alerts@example.com","to":["a@x.com","b@y.com"]}"#;
        let config = NotificationService::parse_config("email", config_json).expect("should parse");

        match config {
            ChannelConfig::Email { from, to } => {
                assert_eq!(from, "alerts@example.com");
                assert_eq!(to, vec!["a@x.com".to_string(), "b@y.com".to_string()]);
            }
            _ => panic!("expected Email variant"),
        }
    }

    #[test]
    fn test_parse_config_email_empty_to_rejected() {
        let config_json = r#"{"from":"a@b.com","to":[]}"#;
        let result = NotificationService::parse_config("email", config_json);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "empty to[] should be rejected"
        );
    }

    #[test]
    fn test_parse_config_email_missing_to_rejected() {
        let config_json = r#"{"from":"a@b.com"}"#;
        let result = NotificationService::parse_config("email", config_json);
        assert!(result.is_err(), "missing to should be rejected");
    }

    #[test]
    fn test_parse_config_email_single_recipient() {
        let config_json = r#"{"from":"a@b.com","to":["only@x.com"]}"#;
        let config = NotificationService::parse_config("email", config_json).expect("should parse");
        match config {
            ChannelConfig::Email { to, .. } => assert_eq!(to.len(), 1),
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
        let config = NotificationService::parse_config("apns", config_json).expect("should parse");

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
        let config = NotificationService::parse_config("apns", config_json).expect("should parse");

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
    fn test_update_candidate_email_empty_to_rejected() {
        // Update path re-parses the effective (type, json) pair.
        // Simulate: existing row is email, update sets config_json to {to:[]}.
        let candidate_type = "email";
        let candidate_json = r#"{"from":"a@b.com","to":[]}"#;
        let result = NotificationService::parse_config(candidate_type, candidate_json);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn test_update_candidate_type_mismatch_rejected() {
        // Simulate: existing row is email with valid email JSON.
        // Update changes notify_type to "telegram" without updating config_json.
        let candidate_type = "telegram";
        let candidate_json = r#"{"from":"a@b.com","to":["c@d.com"]}"#;
        let result = NotificationService::parse_config(candidate_type, candidate_json);
        assert!(result.is_err(), "email json must not parse as telegram");
    }

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
            ChannelConfig::Webhook {
                url,
                method,
                headers,
                body_template,
            } => {
                assert_eq!(url, "https://example.com");
                assert_eq!(method, "POST");
                assert_eq!(headers.get("Authorization").unwrap(), "Bearer token");
                assert_eq!(body_template.as_deref(), Some("{{message}}"));
            }
            _ => panic!("expected Webhook"),
        }
    }

    #[test]
    fn test_render_html_triggered_color() {
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            event: "triggered".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        assert!(
            html.contains("#ea580c"),
            "triggered header should use orange-600 (#ea580c)"
        );
    }

    #[test]
    fn test_render_html_resolved_color() {
        let ctx = NotifyContext {
            event: "resolved".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        assert!(
            html.contains("#16a34a"),
            "resolved header should use green-600 (#16a34a)"
        );
    }

    #[test]
    fn test_render_html_neutral_color_for_other_events() {
        let ctx = NotifyContext {
            event: "ip_changed".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        assert!(html.contains("#6b7280"));
    }

    #[test]
    fn test_render_html_escapes_user_input() {
        let ctx = NotifyContext {
            server_name: "<script>alert(1)</script>".to_string(),
            event: "triggered".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        assert!(
            !html.contains("<script>alert(1)</script>"),
            "raw script tag must not appear in output"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_render_html_skips_empty_fields() {
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            event: "triggered".to_string(),
            cpu: "".to_string(),
            memory: "".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        assert!(
            !html.contains(">CPU<"),
            "empty cpu should not render a CPU row"
        );
        assert!(
            !html.contains(">Memory<"),
            "empty memory should not render a Memory row"
        );
    }

    #[test]
    fn test_email_text_template_is_english() {
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            event: "triggered".to_string(),
            message: "boom".to_string(),
            time: "2026-04-16 12:00:00 UTC".to_string(),
            ..Default::default()
        };
        let rendered = ctx.render(EMAIL_TEXT_TEMPLATE);
        assert!(
            rendered.contains("Time:"),
            "english text template should say Time:"
        );
        assert!(
            !rendered.contains("时间"),
            "english text template must not contain Chinese"
        );
    }

    #[test]
    fn test_parse_config_email_invalid_from_rejected() {
        let cj = r#"{"from":"not-an-email","to":["a@x.com"]}"#;
        let result = NotificationService::parse_config("email", cj);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn test_parse_config_email_invalid_to_entry_rejected() {
        let cj = r#"{"from":"a@x.com","to":["ok@x.com","bogus"]}"#;
        let result = NotificationService::parse_config("email", cj);
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[test]
    fn test_is_plausible_email_simple_cases() {
        assert!(is_plausible_email("a@b.co"));
        assert!(!is_plausible_email("no-at-sign"));
        assert!(!is_plausible_email("@b.co"));
        assert!(!is_plausible_email("a@"));
        assert!(!is_plausible_email("a@nodot"));
    }

    #[tokio::test]
    async fn test_dispatch_email_rejects_missing_api_key() {
        use crate::config::AppConfig;
        use sea_orm::{Database, DatabaseConnection};

        let cfg = AppConfig::default(); // resend.api_key is ""
        let db: DatabaseConnection = Database::connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");

        let n = notification::Model {
            id: "test-id".to_string(),
            name: "test".to_string(),
            notify_type: "email".to_string(),
            config_json: r#"{"from":"a@b.com","to":["c@d.com"]}"#.to_string(),
            enabled: true,
            created_at: Utc::now(),
        };
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            event: "triggered".to_string(),
            ..Default::default()
        };

        let result = NotificationService::dispatch(&db, &cfg, &n, &ctx).await;
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("SERVERBEE_RESEND__API_KEY"));
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    // ── Notification CRUD (DB-backed) ──

    fn email_config_value() -> serde_json::Value {
        serde_json::json!({ "from": "alerts@example.com", "to": ["ops@example.com"] })
    }

    // create() should persist a valid notification and echo the input fields back.
    #[tokio::test]
    async fn test_create_persists_notification() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "Ops Email".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .expect("create should succeed");

        assert_eq!(created.name, "Ops Email");
        assert_eq!(created.notify_type, "email");
        assert!(created.enabled);
        // The row is queryable by its generated id.
        let fetched = NotificationService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
    }

    // create() must reject input whose config fails validation (empty 'to').
    #[tokio::test]
    async fn test_create_rejects_invalid_config() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::create(
            &db,
            CreateNotification {
                name: "bad".to_string(),
                notify_type: "email".to_string(),
                config_json: serde_json::json!({ "from": "a@b.com", "to": [] }),
                enabled: true,
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::Validation(_))));
        // Nothing was persisted on the validation failure path.
        assert!(NotificationService::list(&db).await.unwrap().is_empty());
    }

    // list() should return every created notification.
    #[tokio::test]
    async fn test_list_returns_all_notifications() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        assert!(NotificationService::list(&db).await.unwrap().is_empty());

        for i in 0..3 {
            NotificationService::create(
                &db,
                CreateNotification {
                    name: format!("ch-{i}"),
                    notify_type: "email".to_string(),
                    config_json: email_config_value(),
                    enabled: true,
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(NotificationService::list(&db).await.unwrap().len(), 3);
    }

    // get() should return NotFound for an unknown id.
    #[tokio::test]
    async fn test_get_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::get(&db, "does-not-exist").await;
        match result {
            Err(AppError::NotFound(msg)) => assert!(msg.contains("does-not-exist")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // update() should change individual fields while leaving untouched ones intact.
    #[tokio::test]
    async fn test_update_partial_fields() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "orig".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        // Only rename + disable; type and config remain unchanged.
        let updated = NotificationService::update(
            &db,
            &created.id,
            UpdateNotification {
                name: Some("renamed".to_string()),
                notify_type: None,
                config_json: None,
                enabled: Some(false),
            },
        )
        .await
        .expect("update should succeed");

        assert_eq!(updated.name, "renamed");
        assert!(!updated.enabled);
        assert_eq!(updated.notify_type, "email");
        assert_eq!(updated.config_json, created.config_json);
    }

    // update() should accept a new type+config pair that validates together.
    #[tokio::test]
    async fn test_update_changes_type_and_config() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "ch".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        let updated = NotificationService::update(
            &db,
            &created.id,
            UpdateNotification {
                name: None,
                notify_type: Some("telegram".to_string()),
                config_json: Some(serde_json::json!({ "bot_token": "1:abc", "chat_id": "42" })),
                enabled: None,
            },
        )
        .await
        .expect("update should succeed");

        assert_eq!(updated.notify_type, "telegram");
        assert!(updated.config_json.contains("1:abc"));
    }

    // update() must reject changing notify_type to one incompatible with the kept config.
    #[tokio::test]
    async fn test_update_type_change_without_config_rejected() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "ch".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        // Switching to telegram while keeping the email config must fail validation.
        let result = NotificationService::update(
            &db,
            &created.id,
            UpdateNotification {
                name: None,
                notify_type: Some("telegram".to_string()),
                config_json: None,
                enabled: None,
            },
        )
        .await;
        assert!(result.is_err(), "incompatible type change must be rejected");
        // The stored row remains the original email type.
        assert_eq!(
            NotificationService::get(&db, &created.id)
                .await
                .unwrap()
                .notify_type,
            "email"
        );
    }

    // update() should error with NotFound when the target id does not exist.
    #[tokio::test]
    async fn test_update_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::update(
            &db,
            "missing",
            UpdateNotification {
                name: Some("x".to_string()),
                notify_type: None,
                config_json: None,
                enabled: None,
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    // delete() should remove the row and return Ok.
    #[tokio::test]
    async fn test_delete_existing_notification() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "ch".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        NotificationService::delete(&db, &created.id)
            .await
            .expect("delete should succeed");
        assert!(NotificationService::get(&db, &created.id).await.is_err());
    }

    // delete() on a non-existent id should return NotFound (rows_affected == 0).
    #[tokio::test]
    async fn test_delete_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::delete(&db, "nope").await;
        match result {
            Err(AppError::NotFound(msg)) => assert!(msg.contains("nope")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // ── Notification Group CRUD (DB-backed) ──

    // create_group() should persist the ids as a JSON array and be retrievable.
    #[tokio::test]
    async fn test_create_and_get_group() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "Critical".to_string(),
                notification_ids: vec!["a".to_string(), "b".to_string()],
            },
        )
        .await
        .expect("create_group should succeed");

        assert_eq!(group.name, "Critical");
        let parsed: Vec<String> = serde_json::from_str(&group.notification_ids_json).unwrap();
        assert_eq!(parsed, vec!["a".to_string(), "b".to_string()]);

        let fetched = NotificationService::get_group(&db, &group.id).await.unwrap();
        assert_eq!(fetched.id, group.id);
    }

    // list_groups() should return every created group.
    #[tokio::test]
    async fn test_list_groups_returns_all() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        assert!(NotificationService::list_groups(&db).await.unwrap().is_empty());

        NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "g1".to_string(),
                notification_ids: vec![],
            },
        )
        .await
        .unwrap();
        NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "g2".to_string(),
                notification_ids: vec![],
            },
        )
        .await
        .unwrap();

        assert_eq!(NotificationService::list_groups(&db).await.unwrap().len(), 2);
    }

    // get_group() should return NotFound for an unknown id.
    #[tokio::test]
    async fn test_get_group_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::get_group(&db, "ghost").await;
        match result {
            Err(AppError::NotFound(msg)) => assert!(msg.contains("ghost")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // update_group() should rename and replace the id list.
    #[tokio::test]
    async fn test_update_group_name_and_ids() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "orig".to_string(),
                notification_ids: vec!["x".to_string()],
            },
        )
        .await
        .unwrap();

        let updated = NotificationService::update_group(
            &db,
            &group.id,
            UpdateNotificationGroup {
                name: Some("renamed".to_string()),
                notification_ids: Some(vec!["y".to_string(), "z".to_string()]),
            },
        )
        .await
        .expect("update_group should succeed");

        assert_eq!(updated.name, "renamed");
        let parsed: Vec<String> = serde_json::from_str(&updated.notification_ids_json).unwrap();
        assert_eq!(parsed, vec!["y".to_string(), "z".to_string()]);
    }

    // update_group() with all-None input should leave the row unchanged.
    #[tokio::test]
    async fn test_update_group_noop_keeps_values() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "keep".to_string(),
                notification_ids: vec!["a".to_string()],
            },
        )
        .await
        .unwrap();

        let updated = NotificationService::update_group(
            &db,
            &group.id,
            UpdateNotificationGroup {
                name: None,
                notification_ids: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(updated.name, "keep");
        assert_eq!(updated.notification_ids_json, group.notification_ids_json);
    }

    // update_group() should error NotFound when the group id is unknown.
    #[tokio::test]
    async fn test_update_group_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::update_group(
            &db,
            "missing",
            UpdateNotificationGroup {
                name: Some("x".to_string()),
                notification_ids: None,
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    // delete_group() should remove an existing group.
    #[tokio::test]
    async fn test_delete_group_existing() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "g".to_string(),
                notification_ids: vec![],
            },
        )
        .await
        .unwrap();

        NotificationService::delete_group(&db, &group.id)
            .await
            .expect("delete_group should succeed");
        assert!(NotificationService::get_group(&db, &group.id).await.is_err());
    }

    // delete_group() on a missing id should return NotFound.
    #[tokio::test]
    async fn test_delete_group_missing_returns_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let result = NotificationService::delete_group(&db, "nope").await;
        match result {
            Err(AppError::NotFound(msg)) => assert!(msg.contains("nope")),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // ── Dispatch routing (no-network branches) ──

    // dispatch() for APNs returns Ok early when no device tokens are registered,
    // exercising the Apns arm without any real APNs/network call.
    #[tokio::test]
    async fn test_dispatch_apns_no_devices_is_ok() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        let n = notification::Model {
            id: "apns-1".to_string(),
            name: "apns".to_string(),
            notify_type: "apns".to_string(),
            config_json: r#"{"key_id":"K","team_id":"T","private_key":"-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----","bundle_id":"com.example.app","sandbox":true}"#.to_string(),
            created_at: chrono::Utc::now(),
            enabled: true,
        };
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            server_id: "srv-1".to_string(),
            rule_id: "rule-1".to_string(),
            event: "triggered".to_string(),
            ..Default::default()
        };

        // With an empty device_token table, send_push short-circuits to Ok(()).
        NotificationService::dispatch(&db, &cfg, &n, &ctx)
            .await
            .expect("apns dispatch with no devices should be Ok");
    }

    // dispatch() must surface parse_config errors for an unknown notify_type.
    #[tokio::test]
    async fn test_dispatch_unknown_type_is_validation_error() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        let n = notification::Model {
            id: "bad-1".to_string(),
            name: "bad".to_string(),
            notify_type: "carrier_pigeon".to_string(),
            config_json: r#"{"foo":"bar"}"#.to_string(),
            created_at: chrono::Utc::now(),
            enabled: true,
        };
        let ctx = NotifyContext::default();

        let result = NotificationService::dispatch(&db, &cfg, &n, &ctx).await;
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    // send_group() returns Ok for an empty / non-existent member list (no dispatch attempted).
    #[tokio::test]
    async fn test_send_group_with_empty_members() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "empty".to_string(),
                notification_ids: vec![],
            },
        )
        .await
        .unwrap();

        let ctx = NotifyContext::default();
        NotificationService::send_group(&db, &cfg, &group.id, &ctx)
            .await
            .expect("send_group over no members should be Ok");
    }

    // send_group() skips disabled channels: a disabled member is never dispatched, so Ok.
    #[tokio::test]
    async fn test_send_group_skips_disabled_member() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        // A disabled email channel would otherwise fail (no Resend key);
        // because it is disabled, send_group skips it and returns Ok.
        let disabled = NotificationService::create(
            &db,
            CreateNotification {
                name: "disabled-email".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: false,
            },
        )
        .await
        .unwrap();

        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "grp".to_string(),
                notification_ids: vec![disabled.id.clone()],
            },
        )
        .await
        .unwrap();

        let ctx = NotifyContext::default();
        NotificationService::send_group(&db, &cfg, &group.id, &ctx)
            .await
            .expect("disabled member should be skipped");
    }

    // send_group() swallows per-channel dispatch errors and still returns Ok overall.
    #[tokio::test]
    async fn test_send_group_swallows_dispatch_errors() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default(); // empty Resend key → email dispatch errors

        // Enabled email channel; dispatch will fail (missing API key) but is logged, not propagated.
        let enabled = NotificationService::create(
            &db,
            CreateNotification {
                name: "enabled-email".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "grp".to_string(),
                notification_ids: vec![enabled.id.clone()],
            },
        )
        .await
        .unwrap();

        let ctx = NotifyContext::default();
        // The internal dispatch error is logged; send_group still resolves to Ok.
        NotificationService::send_group(&db, &cfg, &group.id, &ctx)
            .await
            .expect("send_group should swallow per-channel errors");
    }

    // send_group() returns NotFound when the group itself is missing.
    #[tokio::test]
    async fn test_send_group_missing_group_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();
        let ctx = NotifyContext::default();
        let result = NotificationService::send_group(&db, &cfg, "no-group", &ctx).await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    // test_notification() looks up an id and dispatches; for APNs with no devices it is Ok.
    #[tokio::test]
    async fn test_test_notification_apns_no_devices_ok() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "apns".to_string(),
                notify_type: "apns".to_string(),
                config_json: serde_json::json!({
                    "key_id": "K",
                    "team_id": "T",
                    "private_key": "-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----",
                    "bundle_id": "com.example.app",
                    "sandbox": true
                }),
                enabled: true,
            },
        )
        .await
        .unwrap();

        NotificationService::test_notification(&db, &cfg, &created.id)
            .await
            .expect("test_notification for apns with no devices should be Ok");
    }

    // test_notification() returns NotFound when the channel id does not exist.
    #[tokio::test]
    async fn test_test_notification_missing_id_not_found() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();
        let result = NotificationService::test_notification(&db, &cfg, "ghost").await;
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }

    // ── default helpers ──

    // default_method() should yield POST; default_true() should yield true.
    #[test]
    fn test_serde_defaults() {
        assert_eq!(default_method(), "POST");
        assert!(default_true());
    }

    // CreateNotification should default `enabled` to true when the field is omitted.
    #[test]
    fn test_create_notification_enabled_defaults_true() {
        let input: CreateNotification =
            serde_json::from_str(r#"{"name":"n","notify_type":"webhook","config_json":{}}"#)
                .expect("deserialize");
        assert!(input.enabled);
    }

    // Webhook method should default to POST when omitted in the config JSON.
    #[test]
    fn test_parse_config_webhook_default_method() {
        let config =
            NotificationService::parse_config("webhook", r#"{"url":"https://x.test"}"#).unwrap();
        match config {
            ChannelConfig::Webhook { method, .. } => assert_eq!(method, "POST"),
            _ => panic!("expected Webhook"),
        }
    }

    // email_header_color() should map "recovered" to green like "resolved".
    #[test]
    fn test_email_header_color_recovered() {
        assert_eq!(email_header_color("recovered"), "#16a34a");
        assert_eq!(email_header_color("triggered"), "#ea580c");
        assert_eq!(email_header_color("anything-else"), "#6b7280");
    }

    // ── parse_config: per-type missing-field error arms ──

    // Webhook config with custom headers should parse and preserve them, and `method`
    // defaults are kept untouched.
    #[test]
    fn test_parse_config_webhook_with_headers() {
        let cj = r#"{"url":"https://x.test","headers":{"X-Token":"abc","Content-Type":"text/plain"}}"#;
        let config = NotificationService::parse_config("webhook", cj).expect("should parse");
        match config {
            ChannelConfig::Webhook { headers, .. } => {
                assert_eq!(headers.get("X-Token").map(String::as_str), Some("abc"));
                assert_eq!(headers.get("Content-Type").map(String::as_str), Some("text/plain"));
            }
            _ => panic!("expected Webhook variant"),
        }
    }

    // Telegram config missing `chat_id` must fail validation (missing required field).
    #[test]
    fn test_parse_config_telegram_missing_field_rejected() {
        let result = NotificationService::parse_config("telegram", r#"{"bot_token":"123:ABC"}"#);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "telegram without chat_id must be rejected"
        );
    }

    // Bark config missing `device_key` must fail validation.
    #[test]
    fn test_parse_config_bark_missing_field_rejected() {
        let result =
            NotificationService::parse_config("bark", r#"{"server_url":"https://bark.test"}"#);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "bark without device_key must be rejected"
        );
    }

    // Apns config missing `private_key` must fail validation.
    #[test]
    fn test_parse_config_apns_missing_field_rejected() {
        let cj = r#"{"key_id":"K","team_id":"T","bundle_id":"com.example.app"}"#;
        let result = NotificationService::parse_config("apns", cj);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "apns without private_key must be rejected"
        );
    }

    // An unknown notify_type with otherwise valid object JSON must fail at the enum tag step.
    #[test]
    fn test_parse_config_unknown_type_rejected() {
        let result = NotificationService::parse_config("pigeon", r#"{"url":"https://x.test"}"#);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "unknown notify_type must be rejected"
        );
    }

    // When config JSON is valid but NOT an object (e.g. an array), the type tag cannot be
    // injected, so deserialization into the tagged enum still fails with Validation.
    #[test]
    fn test_parse_config_non_object_json_rejected() {
        let result = NotificationService::parse_config("webhook", r#"["not","an","object"]"#);
        assert!(
            matches!(result, Err(AppError::Validation(_))),
            "non-object JSON must be rejected"
        );
    }

    // ── is_plausible_email: extra boundary branches ──

    // An address with multiple '@' splits on the first one; the trailing '@x.com' lands in
    // the domain, which has no '.' before the next '@', but split_once keeps it whole.
    #[test]
    fn test_is_plausible_email_extra_boundaries() {
        // Leading dot in domain still counts as containing '.'
        assert!(is_plausible_email("a@.com"));
        // Domain with no dot is rejected even with a non-empty local part.
        assert!(!is_plausible_email("user@localhost"));
        // Empty string has no '@' and is rejected.
        assert!(!is_plausible_email(""));
        // split_once('@') uses the FIRST '@', so "a@b@c.com" -> local "a", domain "b@c.com".
        assert!(is_plausible_email("a@b@c.com"));
    }

    // ── render_html: full-field rendering ──

    // render_html() should emit a row for every non-empty field including Rule and Message.
    #[test]
    fn test_render_html_renders_all_rows() {
        let ctx = NotifyContext {
            server_name: "web-01".to_string(),
            rule_name: "High CPU".to_string(),
            event: "triggered".to_string(),
            message: "boom".to_string(),
            time: "2026-04-16 12:00:00 UTC".to_string(),
            cpu: "92%".to_string(),
            memory: "70%".to_string(),
            ..Default::default()
        };
        let html = render_html(&ctx);
        // Each populated field renders its label cell.
        for label in ["Server", "Rule", "Event", "Time", "CPU", "Memory", "Message"] {
            assert!(html.contains(label), "html should contain {label} row");
        }
        // The footer attribution is always present.
        assert!(html.contains("Sent by ServerBee"));
    }

    // ── Bark template render ──

    // The Bark body template substitutes message + time and keeps the literal "时间:" label.
    #[test]
    fn test_bark_body_template_render() {
        let ctx = NotifyContext {
            message: "disk full".to_string(),
            time: "2026-04-16 12:00:00 UTC".to_string(),
            ..Default::default()
        };
        let body = ctx.render("{{message}}\n时间: {{time}}");
        assert_eq!(body, "disk full\n时间: 2026-04-16 12:00:00 UTC");
    }

    // ── send_group: member-resolution branches (no network) ──

    // send_group() with a member id that does not resolve to a row hits the `_ => {}` arm
    // (get() returns Err) and still completes Ok without attempting any dispatch.
    #[tokio::test]
    async fn test_send_group_skips_unresolved_member() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "grp".to_string(),
                notification_ids: vec!["ghost-id".to_string()],
            },
        )
        .await
        .unwrap();

        let ctx = NotifyContext::default();
        // The non-existent member is silently skipped; no dispatch, overall Ok.
        NotificationService::send_group(&db, &cfg, &group.id, &ctx)
            .await
            .expect("unresolved member should be skipped");
    }

    // send_group() tolerates a corrupt notification_ids_json (unwrap_or_default -> empty vec)
    // and returns Ok without iterating any members.
    #[tokio::test]
    async fn test_send_group_corrupt_ids_json_is_ok() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let cfg = crate::config::AppConfig::default();

        // Insert a group row directly with malformed ids JSON to exercise unwrap_or_default().
        let model = notification_group::ActiveModel {
            id: Set("corrupt-grp".to_string()),
            name: Set("corrupt".to_string()),
            notification_ids_json: Set("{not valid json".to_string()),
            created_at: Set(Utc::now()),
        };
        model.insert(&db).await.unwrap();

        let ctx = NotifyContext::default();
        NotificationService::send_group(&db, &cfg, "corrupt-grp", &ctx)
            .await
            .expect("corrupt ids json should degrade to empty member list");
    }

    // ── update: enabled-only flip + group id replacement with empty list ──

    // update() may flip only `enabled` from true to true without changing anything else.
    #[tokio::test]
    async fn test_update_enabled_only_toggle() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let created = NotificationService::create(
            &db,
            CreateNotification {
                name: "ch".to_string(),
                notify_type: "email".to_string(),
                config_json: email_config_value(),
                enabled: true,
            },
        )
        .await
        .unwrap();

        let updated = NotificationService::update(
            &db,
            &created.id,
            UpdateNotification {
                name: None,
                notify_type: None,
                config_json: None,
                enabled: Some(false),
            },
        )
        .await
        .expect("update should succeed");
        // Only enabled changed; name/type/config are preserved.
        assert!(!updated.enabled);
        assert_eq!(updated.name, "ch");
        assert_eq!(updated.config_json, created.config_json);
    }

    // update_group() can replace the membership with an empty list (clears the group).
    #[tokio::test]
    async fn test_update_group_to_empty_membership() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let group = NotificationService::create_group(
            &db,
            CreateNotificationGroup {
                name: "g".to_string(),
                notification_ids: vec!["a".to_string(), "b".to_string()],
            },
        )
        .await
        .unwrap();

        let updated = NotificationService::update_group(
            &db,
            &group.id,
            UpdateNotificationGroup {
                name: None,
                notification_ids: Some(vec![]),
            },
        )
        .await
        .expect("update_group should succeed");
        let parsed: Vec<String> = serde_json::from_str(&updated.notification_ids_json).unwrap();
        assert!(parsed.is_empty(), "membership should now be empty");
    }
}
