# Resend Email Integration Design

**Date**: 2026-04-16
**Branch**: `email-noti`
**Status**: Spec — awaiting implementation plan

## Goal

Replace the current SMTP-based email notification implementation with Resend's HTTP API. Eliminate `lettre` and its TLS/SMTP dependency chain, simplify user configuration (API key + from + to, no SMTP host/port/username/password), and upgrade the email body from plain text to HTML + text fallback.

## Non-Goals

- Multi-provider abstraction (Resend only; SMTP is gone, not hidden behind a toggle).
- In-app email body customisation (users cannot edit the HTML template).
- Per-locale email content (single English template for now).
- Deep links from the email back to the ServerBee UI (would require a publicly configured base URL — out of scope).
- Attachments, tracking pixels, or CC/BCC.

## Architecture

### Configuration split

- **Global** (Figment / env): `SERVERBEE_RESEND__API_KEY` — sensitive credential, managed the same way as `admin.password`.

  Requires a new `ResendConfig` struct in `crates/server/src/config.rs` and a `pub resend: ResendConfig` field on `AppConfig` (with `#[serde(default)]` and a matching `ResendConfig::default()` returning an empty key). Without this wiring, Figment silently ignores the env var and the dispatcher behaves as if the key is unset.

  ```rust
  #[derive(Debug, Clone, Deserialize, Default)]
  pub struct ResendConfig {
      #[serde(default)]
      pub api_key: String,
  }
  ```

  Read at dispatch time via `state.config.resend.api_key` (treat empty string as "not configured").

- **Per-channel** (`config_json` in the `notification` row):
  ```json
  { "from": "alerts@yourdomain.com", "to": ["a@x.com", "b@y.com"] }
  ```
  `from` stays per-channel so different alert contexts can use different sender addresses. It is not sensitive, so storing it in DB is fine.

### HTTP client

Use the existing `reqwest` client (already in `dispatch()`). Do **not** pull in `resend-rs`:

- Resend's relevant surface is a single endpoint (`POST https://api.resend.com/emails`). A JSON body plus `Authorization: Bearer <key>` is ~10 lines.
- Keeps the channel implementations consistent (Telegram / Bark / Webhook all call reqwest directly).
- Avoids added transitive dependencies.

### Dependency changes

- **Remove**: `lettre` (with `tokio1-rustls-tls`, `smtp-transport`, `builder`, `hostname` features) from `crates/server/Cargo.toml`.
- **Add**: `html-escape = "0.2"` for safely escaping user-supplied fields into the HTML body.
- Net effect: compile times and binary size shrink.

### Request body

```json
{
  "from": "<from>",
  "to": ["<to1>", "<to2>"],
  "subject": "[ServerBee] {server_name} {event}",
  "html": "<rendered HTML template>",
  "text": "<rendered plain text (EMAIL_TEXT_TEMPLATE)>"
}
```

Non-2xx response: attempt to deserialise `{ "message": "...", "name": "..." }` from the body and surface `message` as `AppError::Internal`. On parse failure, fall back to the raw body text. This ensures the UI's "Test notification" button shows the real cause (e.g. `Domain not verified`, `Invalid API key`).

## Schema changes

`ChannelConfig::Email` (in `crates/server/src/service/notification.rs`):

```rust
// Before
Email {
    smtp_host: String,
    smtp_port: u16,
    username: String,
    password: String,
    from: String,
    to: String,
}

// After
Email {
    from: String,
    to: Vec<String>,  // must contain at least one address
}
```

Validation: parse-time check that `to` is non-empty; reject with `AppError::Validation` otherwise.

### Update-path validation (closes a current gap)

`NotificationService::update()` (in `crates/server/src/service/notification.rs`) currently writes `config_json` back to the DB without re-validating it — only `create()` calls `parse_config`. This design requires that `update()` re-parse the *effective* `(notify_type, config_json)` pair after merging the partial input, using the same `parse_config` path as `create()`. Without this, the `PUT /api/notifications/{id}` endpoint (wired in `crates/server/src/router/api/notification.rs`) would remain a hole that lets malformed or cross-type configs bypass validation and corrupt stored state.

Implementation sketch:

1. Load the existing row.
2. Compute the candidate `notify_type` (updated value if provided, otherwise existing).
3. Compute the candidate `config_json` (updated value if provided, otherwise existing).
4. Call `parse_config(&candidate_type, &candidate_json)` and return `AppError::Validation` on failure.
5. Only then mutate the `ActiveModel` and persist.

Add unit tests covering:
- Update of `config_json` alone with an invalid Email payload (empty `to`) → rejected.
- Update of `notify_type` alone to a value that no longer matches the existing stored `config_json` → rejected.

## Data migration

New sea-orm migration `m20260416_000001_migrate_email_to_resend.rs` (exact date/counter chosen at implementation time to follow the sequence already in `crates/server/src/migration/`):

1. `SELECT * FROM notification WHERE notify_type='email'`.
2. For each row, parse the old JSON:
   - Extract `from` (string) and `to` (string). Rewrite `config_json` to `{ "from": <from>, "to": [<to>] }`.
   - If either field is missing or unparseable: **do not delete the row**. Set `enabled = false` and append ` (needs reconfiguration)` to `name`. Log a `tracing::warn!` with the row id.
3. Short-circuit: if the SELECT returns zero rows (fresh install or the email-noti branch is its first introduction of email), do nothing and log nothing.

`down()` is a no-op (`Ok(())`) per project convention — migrations are not reversible.

Rationale for "disable + rename" over delete: notification configuration is stateful user data; losing it silently is worse than the user seeing a clearly-marked row they must fix.

## HTML template

New module `email_template` inside `crates/server/src/service/notification.rs` (or a sibling file if size warrants) exporting:

- `EMAIL_TEXT_TEMPLATE` — a new English-only constant dedicated to the Email channel. Example: `"[ServerBee] {{server_name}} {{event}}\n{{message}}\nTime: {{time}}"`.
- `render_html(ctx: &NotifyContext) -> String`.

**Why a new constant instead of reusing `DEFAULT_TEMPLATE`**: the existing `DEFAULT_TEMPLATE` (`notification.rs:89`) contains the Chinese literal `时间:` and is already used by the Webhook and Telegram branches, which are out of scope for this change. Keeping those unchanged and introducing a separate English template for Email is the least-invasive way to satisfy the "single English email template" requirement.

### Style

- Pure inline CSS (mail clients ignore `<style>` blocks in `<head>`).
- Single-column, 600 px max width.
- Coloured header bar based on `ctx.event`:
  - `triggered` → red/orange.
  - `resolved` → green.
  - Other events → neutral grey.
- Body: a simple key/value `<table>` rendering `server_name`, `rule_name`, `event`, `time`, `cpu`, `memory`, `message`. Rows whose value is empty are skipped entirely.
- Footer: `Sent by ServerBee` in muted small text.

### Safety

All user-controlled fields (`server_name`, `rule_name`, `message`, etc.) must be passed through `html_escape::encode_text` before insertion into the template. The template itself contains no user-controlled HTML.

### Localisation

Single English template. Recipients may not be ServerBee users, so we cannot pull a locale preference reliably. Localising doubles template maintenance for limited benefit. Revisit if there is explicit demand later.

## Error handling and UX

### Backend

Inside `dispatch()`'s `Email` branch:

1. Read `resend.api_key` from `AppState.config`. If missing / empty, return `AppError::Validation("Resend API key not configured (set SERVERBEE_RESEND__API_KEY)")`.
2. Build the JSON body, POST to `https://api.resend.com/emails` with a 10 s timeout (reuse the client built at the top of `dispatch`).
3. On non-2xx, surface the Resend `message` (or raw body) as `AppError::Internal`.

### Frontend

`apps/web/src/routes/_authed/settings/notifications.tsx`:

- Email type form collapses from 6 fields to 2:
  - `from` — single-line input, placeholder `alerts@yourdomain.com`.
  - `to` — multi-value tag input (at least one required).
- Remove `smtp_host`, `smtp_port`, `username`, `password` rendering branches and i18n keys.
- Update `zh/settings.json` and `en/settings.json` in lockstep.

#### State-model change

The current form uses `configFields: Record<string, string>` and sends it verbatim as `config_json`. `to: string[]` does not fit that shape. Change required:

- Widen the state to `Record<string, string | string[]>` (or introduce a sibling `toAddresses: string[]` slot specifically for the Email path and merge it at submit time). The latter is preferred because the rest of the form already benefits from the string-only shape; narrowing email is less invasive than widening the whole form.
- In the Email branch of `handleTypeChange`, seed `{ from: '' }` in `configFields` and `[]` in `toAddresses`.
- In the Email branch of the render block, replace the generic `Object.entries(configFieldLabels[notifyType])` fallthrough with an explicit two-input block: one `Input` for `from`, one tag input for `to`.
- In `handleCreate`, when `notifyType === 'email'`, build the payload as `{ ...configFields, to: toAddresses }` so the wire shape becomes `{from, to: string[]}`.
- Reset `toAddresses` in `resetForm`.

Tag input: use a simple controlled component (array + add/remove buttons + Enter key). No new dependency — the project already has the primitives (`Input` + shadcn badges can represent chips).

#### UX for missing API key

Since the "Test notification" action lives on **list rows** (not the form), it cannot scope a hint inside a form:

- **Create form (Email branch)**: show a static help text directly above the `from` input explaining that delivery requires `SERVERBEE_RESEND__API_KEY` on the server and that the sender domain must be verified in Resend. Link to the relevant `alerts.mdx` section. This surfaces the requirement *before* submission.
- **Test button**: when the test API returns an error, the existing error toast already renders `err.message`. The backend returns the Resend `message` field verbatim (see Error handling section), so no frontend parsing is needed. Ensure the toast variant has enough width to show the full Resend string, and use a longer duration (e.g. `duration: 8000`).

- Rows disabled by the migration (` (needs reconfiguration)` suffix) use the existing "disabled" visual — no special-casing.

### Documentation

Per CLAUDE.md convention, update simultaneously:

- `ENV.md` — add `SERVERBEE_RESEND__API_KEY` entry.
- `apps/docs/content/docs/en/configuration.mdx` and `apps/docs/content/docs/cn/configuration.mdx` — document the new env var.
- `apps/docs/content/docs/en/alerts.mdx` and `apps/docs/content/docs/cn/alerts.mdx` — rewrite the Email channel section to describe Resend, including the requirement to verify the sender domain in Resend before use.

## Testing

### Rust unit tests (existing `mod tests` in `notification.rs`)

Add:

- `test_parse_config_email_new_schema` — `{from, to: ["a","b"]}` parses to `ChannelConfig::Email { from, to: Vec<String> }`.
- `test_parse_config_email_empty_to_rejected` — `to: []` returns `AppError::Validation`.
- `test_render_html_triggered_color` — header block uses red/orange tone.
- `test_render_html_resolved_color` — header block uses green tone.
- `test_render_html_escapes_user_input` — `server_name = "<script>alert(1)</script>"` does not appear raw in the output.
- `test_render_html_skips_empty_fields` — `cpu = ""` does not render a CPU row.

Remove (SMTP is gone):

- `test_parse_config_email`
- `test_parse_config_email_default_port`

### Migration tests

In `crates/server/tests/` (integration) or beside the migration module:

- Insert a row with the old SMTP `config_json`. Run the migration. Assert `config_json` is rewritten to the new schema and `enabled` is unchanged.
- Insert a malformed row (missing `from`). Run the migration. Assert `enabled = false` and `name` ends with ` (needs reconfiguration)`.
- Empty table: migration runs without error and without log noise.

### Frontend tests

`apps/web/src/routes/_authed/settings/notifications.tsx` vitest coverage:

- Email form renders only `from` and `to` inputs.
- Multi-address `to` input produces a `string[]` payload.

### Manual E2E checklist

Add a new checklist (`tests/notifications/email-resend.md` or matching location):

1. With env var set and a verified Resend domain, click "Test notification" on a saved Email channel → receive a colour-coded HTML email (and a plain-text fallback visible in "View raw").
2. Env var unset → clicking "Test notification" produces an error toast containing the "Resend API key not configured (set SERVERBEE_RESEND__API_KEY)" message. The create-form help text is visible regardless of env var state.
3. `from` uses an unverified domain → clicking "Test notification" produces an error toast containing Resend's `Domain not verified` message verbatim.
4. `to` contains two addresses → both recipients receive the same email (confirm via a single Resend API call in the dashboard's Log view).
5. Update path: PUT an existing Email channel with `config_json` that has `to: []` → `422` with validation error. Change `notify_type` from `email` to `telegram` without also updating `config_json` → `422`.
6. Upgrade from a DB that has the old SMTP email row → on server restart the row is disabled and renamed; new UI shows it as such.

### Build / lint gates

- `cargo build --workspace` succeeds, no warnings.
- `cargo clippy --workspace -- -D warnings` passes (CI requirement).
- `bun x ultracite check` passes.
- `cargo test --workspace` and `bun run test` both green.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Users on self-hosted SMTP (no Resend account) lose email alerts after upgrade | The `email-noti` branch is pre-release; acceptable. Changelog must flag the breaking change; docs point to Resend free tier (3 000/month) as sufficient for the target "lightweight VPS" use case. |
| Domain verification friction stops users getting any alerts | Surface Resend's `Domain not verified` error verbatim via the test button; document the verification step in `alerts.mdx`. |
| Resend outage or account suspension blocks all email alerts | Existing notification system already logs per-channel failures; alert groups dispatch best-effort, so other channels (Telegram, Bark, APNS) still fire. No extra work needed. |
| HTML template breaks in an unusual client (Outlook / dark mode) | Single-column, inline-CSS, table layout is the safest pattern. Manual checklist covers two clients; additional reports can be addressed incrementally. |
| Migration disables a row the user did not want disabled | Renaming to ` (needs reconfiguration)` makes the state visible; user can re-enable after filling in the Resend fields. Row is never deleted. |

## Out of scope (tracked separately if needed)

- Resend webhook ingress for delivery status.
- Template editor for HTML customisation.
- Per-locale email bodies.
- In-email action buttons linking back to ServerBee.
