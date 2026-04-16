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
                        "Resend API key not configured (set SERVERBEE_RESEND__API_KEY)"
                            .to_string(),
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
        let config =
            NotificationService::parse_config("email", config_json).expect("should parse");

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
        let config =
            NotificationService::parse_config("email", config_json).expect("should parse");
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
        assert!(!html.contains(">CPU<"), "empty cpu should not render a CPU row");
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
        assert!(rendered.contains("Time:"), "english text template should say Time:");
        assert!(!rendered.contains("时间"), "english text template must not contain Chinese");
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
}
