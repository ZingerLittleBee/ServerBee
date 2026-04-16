# Resend Email Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the SMTP (`lettre`) email notification implementation with Resend's HTTP API — global API key via env, per-channel `from` + `to: string[]`, HTML + text fallback, drop `lettre` from dependencies.

**Architecture:** A new `ResendConfig` is added to `AppConfig` and threaded as `&AppConfig` through `NotificationService::{send_group, test_notification, dispatch}` and the surrounding `AlertService` functions. The `dispatch()` Email branch swaps `lettre::SmtpTransport` for a `reqwest` POST to `https://api.resend.com/emails`. A sea-orm data migration rewrites any existing email rows into the new schema or disables those it cannot convert. Frontend collapses the 6-field SMTP form into a 2-field Resend form with a tag-style recipient input.

**Tech Stack:** Rust (Axum 0.8 + sea-orm 1.x + reqwest 0.12 + figment), React 19 + TanStack Router + vitest, Resend HTTP API.

**Spec:** `docs/superpowers/specs/2026-04-16-resend-email-integration-design.md`

---

## Task 1: Add `ResendConfig` to `AppConfig`

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Write failing tests**

Append inside the existing `#[cfg(test)] mod tests` block in `crates/server/src/config.rs`:

```rust
    #[test]
    fn test_resend_config_default_is_empty() {
        let cfg = ResendConfig::default();
        assert_eq!(cfg.api_key, "");
    }

    #[test]
    fn test_resend_config_reads_env_var() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("SERVERBEE_RESEND__API_KEY", "re_test_abc123");
            let cfg: AppConfig = figment::Figment::new()
                .merge(figment::providers::Env::prefixed("SERVERBEE_").split("__"))
                .extract()?;
            assert_eq!(cfg.resend.api_key, "re_test_abc123");
            Ok(())
        });
    }

    #[test]
    fn test_app_config_default_has_empty_resend() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.resend.api_key, "");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server --lib config::tests::test_resend -- --exact`
Expected: three tests fail with "cannot find type `ResendConfig` in this scope" and "no field `resend`".

- [ ] **Step 3: Add `ResendConfig` struct and wire into `AppConfig`**

Add this struct near the other config structs in `crates/server/src/config.rs`, e.g. right after `MobileConfig`:

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ResendConfig {
    #[serde(default)]
    pub api_key: String,
}
```

Add a `resend` field to `AppConfig` (insert after `mobile`):

```rust
    #[serde(default)]
    pub mobile: MobileConfig,
    #[serde(default)]
    pub resend: ResendConfig,
}
```

Add `resend: ResendConfig::default(),` to the `Default` impl for `AppConfig` (after the `mobile:` line):

```rust
            mobile: MobileConfig::default(),
            resend: ResendConfig::default(),
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server --lib config::tests`
Expected: all config tests pass. Run `cargo build -p serverbee-server` to confirm there are no compile regressions.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): add ResendConfig for resend api key"
```

---

## Task 2: Change `ChannelConfig::Email` schema and update parse-time tests

**Files:**
- Modify: `crates/server/src/service/notification.rs`

- [ ] **Step 1: Replace the `Email` variant and default helper**

In `crates/server/src/service/notification.rs`, delete these members of the `Email` variant in the `ChannelConfig` enum:

```rust
    Email {
        smtp_host: String,
        #[serde(default = "default_smtp_port")]
        smtp_port: u16,
        username: String,
        password: String,
        from: String,
        to: String,
    },
```

Replace with:

```rust
    Email {
        from: String,
        to: Vec<String>,
    },
```

Delete the now-unused `default_smtp_port` helper:

```rust
fn default_smtp_port() -> u16 {
    587
}
```

- [ ] **Step 2: Add non-empty `to` validation to `parse_config`**

Modify `NotificationService::parse_config` so that after the `serde_json::from_value` call it enforces Email-specific invariants. Replace the tail of `parse_config` with:

```rust
        let config: ChannelConfig = serde_json::from_value(val)
            .map_err(|e| AppError::Validation(format!("Invalid {notify_type} config: {e}")))?;

        if let ChannelConfig::Email { to, .. } = &config {
            if to.is_empty() {
                return Err(AppError::Validation(
                    "Email notification requires at least one 'to' address".to_string(),
                ));
            }
        }

        Ok(config)
```

- [ ] **Step 3: Delete obsolete Email tests and add new ones**

In the `#[cfg(test)] mod tests` block at the bottom of `notification.rs`:

Delete these two tests entirely:
- `test_parse_config_email`
- `test_parse_config_email_default_port`

Add these four tests in the same location:

```rust
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
```

- [ ] **Step 4: Temporarily stub the SMTP dispatch branch so the file compiles**

The `dispatch()` function's `ChannelConfig::Email { smtp_host, smtp_port, username, password, from, to }` branch (inside the match block) no longer pattern-matches. Replace that entire arm with this placeholder (Task 7 will fill it in with the real Resend implementation):

```rust
            ChannelConfig::Email { from: _, to: _ } => {
                return Err(AppError::Internal(
                    "Email dispatch not yet rewired (Task 7)".to_string(),
                ));
            }
```

- [ ] **Step 5: Run tests to verify the new set passes**

Run: `cargo test -p serverbee-server --lib service::notification`
Expected: the four new Email parse tests pass; the two deleted tests no longer exist; all other `notification` tests still pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/notification.rs
git commit -m "refactor(server): reshape ChannelConfig::Email for resend"
```

---

## Task 3: Thread `&AppConfig` through the notification and alert call chain

This is a mechanical, compile-driven refactor. Every signature change below must land in a single commit so the tree stays `cargo check`-green.

**Files:**
- Modify: `crates/server/src/service/notification.rs`
- Modify: `crates/server/src/service/alert.rs`
- Modify: `crates/server/src/task/alert_evaluator.rs`
- Modify: `crates/server/src/task/service_monitor_checker.rs`
- Modify: `crates/server/src/router/api/notification.rs`
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Extend the three `NotificationService` signatures**

In `crates/server/src/service/notification.rs`, change these function signatures:

```rust
    pub async fn send_group(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        group_id: &str,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
```

```rust
    pub async fn test_notification(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        id: &str,
    ) -> Result<(), AppError> {
```

```rust
    async fn dispatch(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        n: &notification::Model,
        ctx: &NotifyContext,
    ) -> Result<(), AppError> {
```

Inside `send_group`, propagate `config` to `dispatch`:

```rust
                    if let Err(e) = Self::dispatch(db, config, &n, ctx).await {
```

Inside `test_notification`, propagate `config` to `dispatch`:

```rust
        Self::dispatch(db, config, &n, &ctx).await
```

- [ ] **Step 2: Extend the four `AlertService` signatures**

In `crates/server/src/service/alert.rs`, change these four function signatures (add `config: &crate::config::AppConfig` as the second parameter after `db`):

```rust
    pub async fn evaluate_all(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        agent_manager: &AgentManager,
        state_manager: &AlertStateManager,
    ) -> Result<(), AppError> {
```

```rust
    async fn evaluate_rule(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        agent_manager: &AgentManager,
        state_manager: &AlertStateManager,
        rule: &alert_rule::Model,
    ) -> Result<(), AppError> {
```

```rust
    async fn handle_triggered(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        state_manager: &AlertStateManager,
        rule: &alert_rule::Model,
        server_id: &str,
        server_name: &str,
    ) -> Result<(), AppError> {
```

```rust
    pub async fn check_event_rules(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        state_manager: &AlertStateManager,
        server_id: &str,
        event_type: &str,
```

Inside these functions, update every internal call so `config` is threaded through:

- `evaluate_all` → `Self::evaluate_rule(db, config, agent_manager, state_manager, &rule)` (the existing `if let Err(e) = Self::evaluate_rule(...)` call).
- `evaluate_rule` → `Self::handle_triggered(db, config, state_manager, rule, &srv.id, &srv.name).await?` and any other internal calls to `handle_triggered`.
- `handle_triggered` → `NotificationService::send_group(db, config, group_id, &ctx).await`.
- `check_event_rules` → `Self::handle_triggered(db, config, state_manager, rule, server_id, &server_name).await?` (at the end of the function).

- [ ] **Step 3: Update every caller to supply `config`**

`crates/server/src/task/alert_evaluator.rs`:

```rust
    if let Err(e) = AlertService::evaluate_all(
        &state.db,
        &state.config,
        &state.agent_manager,
        &state.alert_state_manager,
    )
    .await
    {
        tracing::error!("Alert evaluation error: {e}");
    }
```

`crates/server/src/task/service_monitor_checker.rs` — both `send_group` call sites. Replace:

```rust
        if let Err(e) = NotificationService::send_group(&state.db, group_id, &ctx).await {
```

with:

```rust
        if let Err(e) =
            NotificationService::send_group(&state.db, &state.config, group_id, &ctx).await
        {
```

(Apply the same edit to both sites — the failure-notification site and the recovery-notification site.)

`crates/server/src/router/api/notification.rs` — the `test_notification` handler:

```rust
async fn test_notification(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    NotificationService::test_notification(&state.db, &state.config, &id).await?;
    ok("ok")
}
```

`crates/server/src/router/ws/agent.rs` — both `AlertService::check_event_rules` call sites (around lines 423 and 1021). Add `&state.config` as the second argument. The exact calls will look like:

```rust
                    if let Err(e) = AlertService::check_event_rules(
                        &state.db,
                        &state.config,
                        &state.alert_state_manager,
                        /* … existing args … */
                    )
                    .await
                    {
```

Grep for `check_event_rules(` inside `ws/agent.rs` to find both sites and apply the same insertion.

- [ ] **Step 4: Compile-check**

Run: `cargo check --workspace`
Expected: clean compile. If any caller was missed, the compiler will name it.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: all tests pass (Email dispatch still returns the Task 2 placeholder error, but no test covers that path yet).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/notification.rs \
  crates/server/src/service/alert.rs \
  crates/server/src/task/alert_evaluator.rs \
  crates/server/src/task/service_monitor_checker.rs \
  crates/server/src/router/api/notification.rs \
  crates/server/src/router/ws/agent.rs
git commit -m "refactor(server): thread AppConfig through notification dispatch chain"
```

---

## Task 4: Re-validate `(notify_type, config_json)` on the update path

**Files:**
- Modify: `crates/server/src/service/notification.rs`

- [ ] **Step 1: Write failing tests**

Append these tests to the `#[cfg(test)] mod tests` block in `notification.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify failure — wait, they already pass**

These tests exercise `parse_config` directly; they should already pass given Task 2. That is intentional — they lock in the behaviour the update path will rely on. The *new* behaviour (update actually calling parse_config) is what Step 3 and 4 verify at the code level.

Run: `cargo test -p serverbee-server --lib service::notification::tests::test_update_candidate`
Expected: both pass.

- [ ] **Step 3: Update `NotificationService::update` to re-parse**

Replace the body of `NotificationService::update` in `notification.rs` with:

```rust
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
```

- [ ] **Step 4: Compile + run all notification tests**

Run: `cargo test -p serverbee-server --lib service::notification`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/notification.rs
git commit -m "fix(server): re-validate notification config on update"
```

---

## Task 5: Add `html-escape` dep, `EMAIL_TEXT_TEMPLATE`, and `render_html`

**Files:**
- Modify: `crates/server/Cargo.toml`
- Modify: `crates/server/src/service/notification.rs`

- [ ] **Step 1: Add the `html-escape` dependency**

In `crates/server/Cargo.toml`, inside `[dependencies]`, add one line (keep the existing list alphabetised-ish, near the other lightweight utility crates):

```toml
html-escape = "0.2"
```

- [ ] **Step 2: Write failing tests**

Append these tests inside `#[cfg(test)] mod tests` at the bottom of `notification.rs`:

```rust
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p serverbee-server --lib service::notification::tests::test_render_html`
Expected: tests fail with "cannot find function `render_html`" and "cannot find value `EMAIL_TEXT_TEMPLATE`".

- [ ] **Step 4: Implement `EMAIL_TEXT_TEMPLATE` and `render_html`**

Near the top of `notification.rs`, next to the existing `DEFAULT_TEMPLATE` declaration, add:

```rust
const EMAIL_TEXT_TEMPLATE: &str =
    "[ServerBee] {{server_name}} {{event}}\n{{message}}\nTime: {{time}}";

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
```

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test -p serverbee-server --lib service::notification::tests`
Expected: all six new `render_html` / `EMAIL_TEXT_TEMPLATE` tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/Cargo.toml crates/server/src/service/notification.rs
git commit -m "feat(server): add html email template for resend"
```

---

## Task 6: Replace the `dispatch()` Email branch with Resend HTTP

**Files:**
- Modify: `crates/server/src/service/notification.rs`

- [ ] **Step 1: Remove `lettre` imports from `notification.rs`**

Search `notification.rs` for any `use lettre::…` lines and delete them. The Task 2 placeholder already removed the in-function `use lettre::…` imports; confirm none remain at module scope.

Run: `grep -n lettre crates/server/src/service/notification.rs`
Expected: no matches.

- [ ] **Step 2: Replace the placeholder Email branch**

Replace the entire Email arm that Task 2 left as a placeholder:

```rust
            ChannelConfig::Email { from: _, to: _ } => {
                return Err(AppError::Internal(
                    "Email dispatch not yet rewired (Task 7)".to_string(),
                ));
            }
```

with the real Resend implementation:

```rust
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
```

- [ ] **Step 3: Add a unit test for the missing-key path**

Add to the `#[cfg(test)] mod tests` block:

```rust
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
```

(If `dispatch` is private, temporarily widen it to `pub(super)` or `pub(crate)` for the test — the surrounding test module already sits inside the service module, so `dispatch` should be callable directly without changing visibility. If not, prefix with `#[allow(dead_code)]` on a thin `pub(crate)` wrapper only used in tests.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server --lib service::notification`
Expected: all tests pass, including the new missing-key test.

- [ ] **Step 5: Full workspace compile**

Run: `cargo build -p serverbee-server`
Expected: clean build, no `lettre` warnings (it is still a declared dep, so this will link it until Task 7 removes it).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/notification.rs
git commit -m "feat(server): send email notifications via resend http api"
```

---

## Task 7: Remove `lettre` from `Cargo.toml`

**Files:**
- Modify: `crates/server/Cargo.toml`
- Modify: `Cargo.lock` (auto-updated by cargo)

- [ ] **Step 1: Delete the `lettre` line**

In `crates/server/Cargo.toml`, remove the line:

```toml
lettre = { version = "0.11", default-features = false, features = ["tokio1-rustls-tls", "smtp-transport", "builder", "hostname"] }
```

- [ ] **Step 2: Rebuild**

Run: `cargo build --workspace`
Expected: success. `Cargo.lock` is updated automatically.

- [ ] **Step 3: Confirm zero references remain**

Run: `grep -r "lettre" crates/`
Expected: no matches.

Run: `cargo clippy --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 4: Run full test suite**

Run: `cargo test --workspace`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/Cargo.toml Cargo.lock
git commit -m "chore(server): drop lettre dependency"
```

---

## Task 8: Data migration for legacy SMTP email rows

**Files:**
- Create: `crates/server/src/migration/m20260416_000017_migrate_email_to_resend.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create the migration file with tests for `convert_email_config`**

Create `crates/server/src/migration/m20260416_000017_migrate_email_to_resend.rs` with the following contents:

```rust
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, FromQueryResult, Statement};
use serde_json::Value;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260416_000017_migrate_email_to_resend"
    }
}

#[derive(FromQueryResult)]
struct EmailRow {
    id: String,
    name: String,
    config_json: String,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let rows: Vec<EmailRow> = EmailRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id, name, config_json FROM notification WHERE notify_type = 'email'",
            [],
        ))
        .all(db)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }

        for row in rows {
            match convert_email_config(&row.config_json) {
                Ok(new_json) => {
                    db.execute(Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "UPDATE notification SET config_json = ? WHERE id = ?",
                        [new_json.into(), row.id.clone().into()],
                    ))
                    .await?;
                }
                Err(reason) => {
                    tracing::warn!(
                        "Disabling email notification {} ({}): unconvertable legacy config ({reason})",
                        row.id,
                        row.name,
                    );
                    let new_name = format!("{} (needs reconfiguration)", row.name);
                    db.execute(Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "UPDATE notification SET name = ?, enabled = 0 WHERE id = ?",
                        [new_name.into(), row.id.clone().into()],
                    ))
                    .await?;
                }
            }
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

fn convert_email_config(old_json: &str) -> Result<String, String> {
    let val: Value = serde_json::from_str(old_json).map_err(|e| format!("invalid JSON: {e}"))?;

    let from = val
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'from' field".to_string())?;
    let to = val
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'to' field".to_string())?;

    if from.is_empty() || to.is_empty() {
        return Err("empty from/to".to_string());
    }

    Ok(serde_json::json!({
        "from": from,
        "to": [to],
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_email_config_happy_path() {
        let old = r#"{"smtp_host":"smtp.gmail.com","smtp_port":587,"username":"u","password":"p","from":"a@b.com","to":"c@d.com"}"#;
        let new = convert_email_config(old).expect("should convert");
        let v: Value = serde_json::from_str(&new).unwrap();
        assert_eq!(v["from"], "a@b.com");
        assert_eq!(v["to"][0], "c@d.com");
        assert_eq!(v["to"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_convert_email_config_missing_from() {
        let old = r#"{"to":"c@d.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_missing_to() {
        let old = r#"{"from":"a@b.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_empty_from() {
        let old = r#"{"from":"","to":"c@d.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_garbage_json() {
        assert!(convert_email_config("not json").is_err());
    }
}
```

- [ ] **Step 2: Register the migration in `mod.rs`**

In `crates/server/src/migration/mod.rs`, add the module declaration (after the last existing one):

```rust
mod m20260416_000017_migrate_email_to_resend;
```

Add to the `migrations()` vector (at the end, preserving order):

```rust
            Box::new(m20260416_000017_migrate_email_to_resend::Migration),
```

- [ ] **Step 3: Run the unit tests**

Run: `cargo test -p serverbee-server --lib migration::m20260416_000017`
Expected: all five `convert_email_config` tests pass.

- [ ] **Step 4: Verify the migration runs on a fresh DB**

Run: `cargo build -p serverbee-server && rm -f /tmp/serverbee-plan-test.db && SERVERBEE_DATABASE__PATH=/tmp/serverbee-plan-test.db cargo run -p serverbee-server -- --help 2>/dev/null || true`

Then run the server briefly (Ctrl-C after the migration log line appears):

```bash
SERVERBEE_DATABASE__PATH=/tmp/serverbee-plan-test.db cargo run -p serverbee-server
```

Expected: migration log line `Applying migration 'm20260416_000017_migrate_email_to_resend'` appears, no errors. Kill the process.

Clean up:

```bash
rm -f /tmp/serverbee-plan-test.db
```

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): migrate legacy smtp email notifications to resend schema"
```

---

## Task 9: Migration integration test against an in-memory sqlite DB

**Files:**
- Create: `crates/server/tests/email_migration_integration.rs`

- [ ] **Step 1: Check the existing integration test style**

Run: `ls crates/server/tests/`
Look at one existing `*.rs` file to confirm the project convention (imports, tokio test macro, sqlite in-memory URL pattern).

- [ ] **Step 2: Create the integration test**

Create `crates/server/tests/email_migration_integration.rs`:

```rust
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serverbee_server::migration::Migrator;

async fn fresh_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("connect sqlite");
    // Run everything up through the email-resend migration.
    Migrator::up(&db, None).await.expect("run migrations");
    db
}

async fn exec(db: &sea_orm::DatabaseConnection, sql: &str) {
    db.execute(Statement::from_string(DatabaseBackend::Sqlite, sql.to_string()))
        .await
        .expect("exec");
}

async fn config_json_of(db: &sea_orm::DatabaseConnection, id: &str) -> String {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        config_json: String,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT config_json FROM notification WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .config_json
}

async fn name_of(db: &sea_orm::DatabaseConnection, id: &str) -> String {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        name: String,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT name FROM notification WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .name
}

async fn enabled_of(db: &sea_orm::DatabaseConnection, id: &str) -> bool {
    use sea_orm::FromQueryResult;

    #[derive(FromQueryResult)]
    struct Row {
        enabled: bool,
    }

    Row::find_by_statement(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "SELECT enabled FROM notification WHERE id = ?",
        [id.into()],
    ))
    .one(db)
    .await
    .expect("query")
    .expect("row")
    .enabled
}

#[tokio::test]
async fn migrates_valid_smtp_row_to_resend_schema() {
    // Fresh DB with all migrations already run — insert a legacy-shaped row,
    // then roll our migration back and re-apply to exercise it on the row.
    let db = Database::connect("sqlite::memory:").await.unwrap();

    // Apply all migrations except the last one.
    Migrator::up(&db, Some(Migrator::migrations().len() as u32 - 1))
        .await
        .unwrap();

    exec(
        &db,
        "INSERT INTO notification (id, name, notify_type, config_json, enabled, created_at) \
         VALUES ('row-1', 'ops email', 'email', \
         '{\"smtp_host\":\"smtp.gmail.com\",\"smtp_port\":587,\"username\":\"u\",\"password\":\"p\",\"from\":\"alerts@x.com\",\"to\":\"ops@y.com\"}', \
         1, '2026-04-16T00:00:00+00:00')",
    )
    .await;

    // Run the final (email-resend) migration.
    Migrator::up(&db, None).await.unwrap();

    let new_json = config_json_of(&db, "row-1").await;
    let v: serde_json::Value = serde_json::from_str(&new_json).unwrap();
    assert_eq!(v["from"], "alerts@x.com");
    assert_eq!(v["to"][0], "ops@y.com");
    assert!(v.get("smtp_host").is_none());
    assert!(enabled_of(&db, "row-1").await, "enabled preserved");
    assert_eq!(name_of(&db, "row-1").await, "ops email");
}

#[tokio::test]
async fn disables_unconvertable_email_row() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, Some(Migrator::migrations().len() as u32 - 1))
        .await
        .unwrap();

    // Legacy row missing the `from` field.
    exec(
        &db,
        "INSERT INTO notification (id, name, notify_type, config_json, enabled, created_at) \
         VALUES ('row-2', 'broken email', 'email', \
         '{\"smtp_host\":\"smtp.gmail.com\",\"to\":\"ops@y.com\"}', \
         1, '2026-04-16T00:00:00+00:00')",
    )
    .await;

    Migrator::up(&db, None).await.unwrap();

    assert!(!enabled_of(&db, "row-2").await, "row should be disabled");
    assert_eq!(
        name_of(&db, "row-2").await,
        "broken email (needs reconfiguration)"
    );
}

#[tokio::test]
async fn empty_table_migrates_without_error() {
    let _ = fresh_db().await;
    // Applying all migrations on an empty DB is a no-op for the email-resend
    // migration; reaching this point without panicking is the assertion.
}
```

The above depends on `Migrator` being re-exported from the `serverbee_server` crate. If it is not, add `pub use migration::Migrator;` to `crates/server/src/lib.rs` (or the main module that integration tests consume).

- [ ] **Step 3: Check whether `Migrator` is re-exported**

Run: `grep -n "pub use migration" crates/server/src/lib.rs crates/server/src/main.rs 2>/dev/null`

If `Migrator` is not publicly reachable from an integration test, add this line to `crates/server/src/lib.rs` (or create `lib.rs` if it doesn't exist — check first with `ls crates/server/src/lib.rs`):

```rust
pub use crate::migration::Migrator;
```

If no `lib.rs` exists, the crate is a pure binary and integration tests can't reach internal modules. In that case, move the three migration tests inline into `m20260416_000017_migrate_email_to_resend.rs` under a `#[cfg(test)] mod integration_tests` block that opens its own sqlite in-memory DB and invokes the migration's `up()` directly with a mocked `SchemaManager`. (Simpler: skip the integration test and rely on the unit tests from Task 8 plus manual E2E item 6.)

- [ ] **Step 4: Run the integration test**

Run: `cargo test -p serverbee-server --test email_migration_integration`
Expected: all three tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/tests/email_migration_integration.rs
# Include lib.rs only if it was added/modified in Step 3.
git commit -m "test(server): cover email->resend migration end-to-end"
```

---

## Task 10: Frontend — new i18n keys, remove SMTP copy

**Files:**
- Modify: `apps/web/src/locales/en/settings.json`
- Modify: `apps/web/src/locales/zh/settings.json`

- [ ] **Step 1: English locale**

In `apps/web/src/locales/en/settings.json`, delete these four keys from the `notifications.*` block:

```json
  "notifications.smtp_host": "SMTP Host",
  "notifications.smtp_port": "SMTP Port",
  "notifications.smtp_username": "Username",
  "notifications.smtp_password": "Password",
```

Add these keys in the same block:

```json
  "notifications.email_help_text": "Email delivery uses Resend. Set SERVERBEE_RESEND__API_KEY on the server and verify your sender domain at resend.com/domains before sending.",
  "notifications.add_recipient": "Add",
  "notifications.recipient_placeholder": "someone@example.com",
  "notifications.recipients_label": "Recipients",
  "notifications.remove_recipient_aria": "Remove {{address}}",
```

- [ ] **Step 2: Chinese locale**

In `apps/web/src/locales/zh/settings.json`, apply the same deletions and add Chinese translations:

```json
  "notifications.email_help_text": "邮件发送使用 Resend。发送前请在服务器上设置 SERVERBEE_RESEND__API_KEY，并在 resend.com/domains 验证发件域名。",
  "notifications.add_recipient": "添加",
  "notifications.recipient_placeholder": "someone@example.com",
  "notifications.recipients_label": "收件人",
  "notifications.remove_recipient_aria": "移除 {{address}}",
```

- [ ] **Step 3: Verify JSON parses**

Run: `bun x ultracite check apps/web/src/locales`
Expected: no issues. If it reports trailing-comma issues, follow the existing style of each file.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/settings.json apps/web/src/locales/zh/settings.json
git commit -m "i18n(web): replace smtp notification strings with resend copy"
```

---

## Task 11: Frontend — refactor the Email form to the new shape

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/notifications.tsx`

- [ ] **Step 1: Extract a pure helper for testing**

Create the helper inside the same file (near the top, after the `NotifyType` definition) so Task 13 can unit-test it:

```tsx
export function buildEmailPayload(
  from: string,
  toAddresses: string[]
): { from: string; to: string[] } {
  return { from, to: toAddresses }
}
```

- [ ] **Step 2: Widen mutation input type and form state**

In `notifications.tsx`, change the `createMutation` `mutationFn` type to accept `string[]` values:

```tsx
  const createMutation = useMutation({
    mutationFn: (input: {
      config_json: Record<string, string | string[]>
      name: string
      notify_type: string
    }) => api.post<Notification>('/api/notifications', input),
```

Add new state slots above `configFields`:

```tsx
  const [toAddresses, setToAddresses] = useState<string[]>([])
  const [toInput, setToInput] = useState('')
```

- [ ] **Step 3: Update `handleTypeChange` for the email branch**

Replace the `case 'email':` block in `handleTypeChange`:

```tsx
      case 'email':
        setConfigFields({ from: '' })
        setToAddresses([])
        setToInput('')
        break
```

- [ ] **Step 4: Update `resetForm`**

Add these two lines to `resetForm` (before `setShowForm(false)`):

```tsx
    setToAddresses([])
    setToInput('')
```

- [ ] **Step 5: Add the recipient-management helpers**

Add right below `handleApnsFileUpload`:

```tsx
  const handleAddRecipient = () => {
    const trimmed = toInput.trim()
    if (trimmed === '' || toAddresses.includes(trimmed)) {
      return
    }
    setToAddresses((prev) => [...prev, trimmed])
    setToInput('')
  }

  const handleRemoveRecipient = (addr: string) => {
    setToAddresses((prev) => prev.filter((a) => a !== addr))
  }
```

- [ ] **Step 6: Update `handleCreate` to build the Email payload**

Replace the body of `handleCreate` with:

```tsx
  const handleCreate = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim().length === 0) {
      return
    }
    let payload: Record<string, string | string[]> = configFields
    if (notifyType === 'email') {
      if (toAddresses.length === 0) {
        return
      }
      payload = buildEmailPayload(configFields.from ?? '', toAddresses)
    }
    createMutation.mutate({
      name: name.trim(),
      notify_type: notifyType,
      config_json: payload,
    })
  }
```

- [ ] **Step 7: Remove the SMTP labels from `configFieldLabels`**

Replace the `email:` entry inside `configFieldLabels`:

```tsx
    email: {
      from: t('notifications.from_address'),
    }
```

- [ ] **Step 8: Add a dedicated Email render branch**

Currently the `notifyType === 'apns'` ternary has only two arms (apns vs generic). Extend it to three arms so Email gets its own rendering. Replace the `{notifyType === 'apns' ? (...) : ( ... )}` block with:

```tsx
              {notifyType === 'apns' ? (
                <>
                  {/* existing apns block — leave unchanged */}
                </>
              ) : notifyType === 'email' ? (
                <>
                  <p className="text-muted-foreground text-xs">{t('notifications.email_help_text')}</p>
                  <Input
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, from: e.target.value }))}
                    placeholder={t('notifications.from_address')}
                    required
                    type="email"
                    value={configFields.from ?? ''}
                  />
                  <div className="space-y-2">
                    <Label className="text-sm">{t('notifications.recipients_label')}</Label>
                    <div className="flex gap-2">
                      <Input
                        onChange={(e) => setToInput(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault()
                            handleAddRecipient()
                          }
                        }}
                        placeholder={t('notifications.recipient_placeholder')}
                        type="email"
                        value={toInput}
                      />
                      <Button onClick={handleAddRecipient} size="sm" type="button">
                        {t('notifications.add_recipient')}
                      </Button>
                    </div>
                    {toAddresses.length > 0 && (
                      <div className="flex flex-wrap gap-1">
                        {toAddresses.map((addr) => (
                          <span
                            className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs"
                            key={addr}
                          >
                            {addr}
                            <button
                              aria-label={t('notifications.remove_recipient_aria', { address: addr })}
                              className="text-muted-foreground hover:text-foreground"
                              onClick={() => handleRemoveRecipient(addr)}
                              type="button"
                            >
                              ×
                            </button>
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </>
              ) : (
                Object.entries(configFieldLabels[notifyType] ?? {}).map(([key, label]) => (
                  <Input
                    key={key}
                    onChange={(e) => setConfigFields((prev) => ({ ...prev, [key]: e.target.value }))}
                    placeholder={label}
                    required
                    type={SENSITIVE_FIELDS.has(key) ? 'password' : 'text'}
                    value={configFields[key] ?? ''}
                  />
                ))
              )}
```

- [ ] **Step 9: Extend the test-button toast duration**

Change `testMutation.onError` to:

```tsx
    onError: (err) => {
      toast.error(
        err instanceof Error ? err.message : t('notifications.test_failed'),
        { duration: 8000 }
      )
    }
```

- [ ] **Step 10: Compile check**

Run: `bun run typecheck`
Expected: clean.

Run: `bun x ultracite check apps/web/src/routes/_authed/settings/notifications.tsx`
Expected: clean (fix style issues with `bun x ultracite fix` if any).

- [ ] **Step 11: Manual smoke (optional but recommended)**

Start the dev server (`make web-dev-prod` or `cd apps/web && bun run dev`) and confirm:

- Selecting "Email" type shows only `from`, the recipient input, and help text.
- Typing an address and pressing Enter adds a chip. Clicking × removes it.
- Submit is blocked if no recipients are added.

- [ ] **Step 12: Commit**

```bash
git add apps/web/src/routes/_authed/settings/notifications.tsx
git commit -m "feat(web): redesign email notification form around resend"
```

---

## Task 12: Frontend — vitest for the Email payload helper

**Files:**
- Create: `apps/web/src/routes/_authed/settings/notifications.test.tsx`

- [ ] **Step 1: Write the tests**

Create `apps/web/src/routes/_authed/settings/notifications.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { buildEmailPayload } from './notifications'

describe('buildEmailPayload', () => {
  it('wraps a single recipient as a string array', () => {
    const payload = buildEmailPayload('alerts@example.com', ['ops@example.com'])
    expect(payload).toEqual({ from: 'alerts@example.com', to: ['ops@example.com'] })
  })

  it('preserves multiple recipients in order', () => {
    const payload = buildEmailPayload('alerts@example.com', [
      'a@x.com',
      'b@y.com',
      'c@z.com',
    ])
    expect(payload.to).toEqual(['a@x.com', 'b@y.com', 'c@z.com'])
  })

  it('allows an empty from (validation happens at submit time)', () => {
    const payload = buildEmailPayload('', ['ops@example.com'])
    expect(payload.from).toBe('')
  })
})
```

- [ ] **Step 2: Run the tests**

Run: `cd apps/web && bun run test -- notifications.test`
Expected: three tests pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/notifications.test.tsx
git commit -m "test(web): cover email notification payload builder"
```

---

## Task 13: Docs — `ENV.md` and `configuration.mdx` (en + cn)

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`

- [ ] **Step 1: `ENV.md`**

In `ENV.md`, add a new section after the existing `GeoIP (Optional)` or in the most natural alphabetical-ish slot. The section:

```markdown
### Resend (Email Notifications)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_RESEND__API_KEY` | `resend.api_key` | string | `""` | Resend API key (https://resend.com/api-keys). Required to use the Email notification channel. The sender address (`from`) configured on each email channel must belong to a domain verified at https://resend.com/domains. |
```

- [ ] **Step 2: `configuration.mdx` (en)**

In `apps/docs/content/docs/en/configuration.mdx`, find the section listing env vars (grep for `SERVERBEE_ADMIN__PASSWORD` to locate) and add a new subsection or table row mirroring the `ENV.md` style. If the file uses a table, append:

```markdown
| `SERVERBEE_RESEND__API_KEY` | `resend.api_key` | string | `""` | Resend API key. Required for Email notifications. Sender domain must be verified at resend.com/domains. |
```

If there is a prose section per config group, add a new "Resend" subsection with an equivalent description and a TOML snippet:

````markdown
### Resend (Email Notifications)

```toml
[resend]
api_key = "re_xxx"
```

Set via `SERVERBEE_RESEND__API_KEY`. Required to use the Email notification channel. The `from` address on each email channel must belong to a domain you have verified in Resend.
````

- [ ] **Step 3: `configuration.mdx` (cn)**

Apply the same change to `apps/docs/content/docs/cn/configuration.mdx` with Chinese copy. Example prose block:

````markdown
### Resend（邮件通知）

```toml
[resend]
api_key = "re_xxx"
```

通过 `SERVERBEE_RESEND__API_KEY` 配置。使用邮件通知通道时必填。各个邮件通道的 `from` 发件地址必须属于你在 Resend 已验证的域名。
````

- [ ] **Step 4: Verify docs build**

Run: `cd apps/docs && bun run build 2>&1 | tail -40`
Expected: no MDX compile errors. If the docs site is not part of the default build, skip this step and rely on the next typecheck.

- [ ] **Step 5: Commit**

```bash
git add ENV.md apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx
git commit -m "docs: document SERVERBEE_RESEND__API_KEY env var"
```

---

## Task 14: Docs — `alerts.mdx` (en + cn) Email channel section

**Files:**
- Modify: `apps/docs/content/docs/en/alerts.mdx`
- Modify: `apps/docs/content/docs/cn/alerts.mdx`

- [ ] **Step 1: English — replace the Email section**

In `apps/docs/content/docs/en/alerts.mdx`, find the `### Email` section (around line 189). Replace its entire body (from the `### Email` heading through the end of the example JSON and the paragraph ending with `[ServerBee] {server_name} {event}`) with:

````markdown
### Email (via Resend)

Email notifications are delivered through [Resend](https://resend.com/). Two steps before use:

1. Set `SERVERBEE_RESEND__API_KEY` on the server (see the [Configuration](/docs/en/configuration) page).
2. Add and verify your sender domain at [resend.com/domains](https://resend.com/domains). The `from` address on each channel must belong to a verified domain.

Channel config:

```json
{
  "from": "alerts@yourdomain.com",
  "to": ["ops@example.com", "oncall@example.com"]
}
```

`to` is an array — a single channel can deliver to multiple recipients in one API call. The subject follows the format `[ServerBee] {server_name} {event}`; the body is HTML with a plain-text fallback.
````

- [ ] **Step 2: Chinese — apply the same rewrite**

In `apps/docs/content/docs/cn/alerts.mdx`, find and replace the equivalent Email section:

````markdown
### 邮件（通过 Resend）

邮件通知通过 [Resend](https://resend.com/) 发送。使用前两步准备：

1. 在服务器设置 `SERVERBEE_RESEND__API_KEY`（参考[配置](/docs/cn/configuration)页面）。
2. 在 [resend.com/domains](https://resend.com/domains) 添加并验证发件域名。各通道的 `from` 必须属于已验证的域名。

通道配置：

```json
{
  "from": "alerts@yourdomain.com",
  "to": ["ops@example.com", "oncall@example.com"]
}
```

`to` 是数组——单个通道可以一次投递给多个收件人。主题格式为 `[ServerBee] {server_name} {event}`，正文使用 HTML 并附纯文本兜底。
````

- [ ] **Step 3: Verify docs build**

Run: `cd apps/docs && bun run build 2>&1 | tail -40`
Expected: no MDX errors.

- [ ] **Step 4: Commit**

```bash
git add apps/docs/content/docs/en/alerts.mdx apps/docs/content/docs/cn/alerts.mdx
git commit -m "docs: rewrite email channel docs around resend"
```

---

## Task 15: Manual E2E checklist

**Files:**
- Modify: `tests/alerts-notifications.md` (append) OR create `tests/notifications-email-resend.md`

- [ ] **Step 1: Decide on location**

Run: `ls tests/`
If `alerts-notifications.md` exists and already covers notification scenarios, append a new section to it. Otherwise create `tests/notifications-email-resend.md`.

- [ ] **Step 2: Write the checklist**

Append to (or create) the file with:

```markdown
## Resend email channel

Prereqs: a Resend account with a verified domain; `SERVERBEE_RESEND__API_KEY` set on the dev server before startup.

1. **Happy path** — create an Email channel (`from = alerts@<verified-domain>`, one recipient), click "Test notification". Receiving inbox shows an email with a colour-coded header row; "View raw" shows both HTML and plain-text parts.
2. **Missing API key** — unset the env var, restart the server, click "Test notification" on the saved channel. Error toast contains `Resend API key not configured (set SERVERBEE_RESEND__API_KEY)`. The create-form help text is still visible regardless of env var state.
3. **Unverified domain** — create a channel with `from` on an unverified domain, click "Test notification". Error toast surfaces Resend's `Domain not verified` message verbatim.
4. **Multiple recipients** — create a channel with two recipients, click "Test notification". Both inboxes receive the email. In the Resend dashboard Log view, exactly one API call is recorded.
5. **Update-path validation** — PUT `/api/notifications/{id}` with `config_json = {"from":"a@b.com","to":[]}`. Server returns `422` with a validation error. Change `notify_type` from `email` to `telegram` without updating `config_json` — also `422`.
6. **Legacy migration** — start from a DB containing an old SMTP email row (pre-migration snapshot). On server restart, that row is disabled and renamed with ` (needs reconfiguration)` suffix. The notifications settings page reflects this.
```

- [ ] **Step 3: Commit**

```bash
git add tests/
git commit -m "docs(tests): add resend email manual e2e checklist"
```

---

## Task 16: Final verification — full build, lint, test

- [ ] **Step 1: Rust checks**

Run in parallel:
```bash
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Expected: all three clean/green.

- [ ] **Step 2: Frontend checks**

```bash
bun run typecheck
bun x ultracite check
bun run test
```

Expected: all three clean/green.

- [ ] **Step 3: Confirm dependency diff**

```bash
grep -n "lettre\|html-escape\|resend" crates/server/Cargo.toml
```

Expected: `html-escape = "0.2"` present; no `lettre`; no `resend-rs`.

- [ ] **Step 4: Summarise and hand off**

No commit here — summarise to the user what was shipped, point at the changelog entry (if any was updated during the docs task), and list the branch's next steps (e.g., PR, changelog blurb, release bump).

---

## Self-review notes

- **Spec coverage** — Every spec section has at least one task: config struct (Task 1), schema (Task 2), config propagation (Task 3), update validation (Task 4), HTML template + dep (Task 5), dispatch swap (Task 6), lettre removal (Task 7), migration (Task 8), migration integration test (Task 9), frontend i18n (Task 10), form refactor (Task 11), frontend vitest (Task 12), env docs (Task 13), alerts docs (Task 14), manual E2E (Task 15), verification gates (Task 16).
- **Placeholder scan** — No "TBD", "implement later", or vague instructions. Every code block is concrete. Task 9 Step 3 has a contingency (`lib.rs` re-export may be needed) because the outcome depends on current crate structure — the contingency is spelled out, not left as "figure it out".
- **Type / name consistency** — `EMAIL_TEXT_TEMPLATE`, `render_html`, `email_header_color`, `buildEmailPayload`, `toAddresses`, `toInput`, `handleAddRecipient`, `handleRemoveRecipient`, `convert_email_config`, `m20260416_000017_migrate_email_to_resend` appear identically everywhere they are referenced.
- **Commit structure** — Each task ends with a conventional-commit message in English (per the user's repo convention).
