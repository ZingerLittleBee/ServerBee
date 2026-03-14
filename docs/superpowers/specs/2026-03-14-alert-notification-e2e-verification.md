# Alert & Notification E2E Verification Design

## Goal

End-to-end verification of the complete alert→notification pipeline using all 4 notification channels (Webhook, Telegram, Bark, Email) with real credentials, via browser UI operations. Includes prerequisite UI fixes to make the full flow operable.

## Scope

### Feature Fixes (Prerequisites)

1. **Alert form: add missing fields** — `min`, `duration`, `cycle_interval`, `cycle_limit` fields. Currently only `rule_type` + `max` are exposed.
2. **Alert state API + UI** — New `GET /api/alert-rules/:id/states` endpoint + frontend display of triggered/resolved per-server status. Backend `alert_state` table already has the data.
3. **Notification form: sensitive fields** — Change password/token/key fields to `type="password"` instead of `type="text"`.

### E2E Verification

4. Create all 4 notification channels via UI
5. Verify test notification sends to all channels
6. Verify threshold-based alert triggers notification (CPU max: 1%)
7. Verify offline-based alert triggers notification (Agent stopped, duration: 60s)
8. Verify alert state visible in UI (triggered → resolved)

### Out of Scope

- API Keys Create button issue (unrelated to alert/notification, already verified via API)
- Notifications Add button (code is correct, prior E2E was timing issue — re-verify during testing)

## Credential Configuration

File: `.env.test` (project root, gitignored). Used only by the operator to copy values into the browser UI forms.

```bash
# Webhook (use a request inspection service like webhook.site for observable payloads)
TEST_WEBHOOK_URL=https://webhook.site/your-unique-id

# Telegram
TEST_TELEGRAM_BOT_TOKEN=123456:ABC-DEF...
TEST_TELEGRAM_CHAT_ID=-1001234567890

# Bark
TEST_BARK_SERVER_URL=https://api.day.app
TEST_BARK_DEVICE_KEY=your_device_key

# Email (SMTP)
TEST_EMAIL_SMTP_HOST=smtp.gmail.com
TEST_EMAIL_SMTP_PORT=587
TEST_EMAIL_USERNAME=your@gmail.com
TEST_EMAIL_PASSWORD=app_password
TEST_EMAIL_FROM=your@gmail.com
TEST_EMAIL_TO=target@example.com
```

**Security**: `.env.test` is gitignored. Screenshots are taken only on list views (which do NOT display `config_json`), never on form views with credentials filled in.

## Feature Fix 1: Alert Form — Complete Rule Types + Missing Fields

### Current State

`alerts.tsx` has two problems:
1. The `ruleTypes` dropdown only lists 12 of the 19 supported rule types — missing `memory`, `swap`, `disk`, `transfer_in_cycle`, `transfer_out_cycle`, `transfer_all_cycle`, `expiration`
2. Only `rule_type` + `max` fields are rendered per rule item. The backend also supports `min`, `duration`, `cycle_interval`, `cycle_limit`.

### Backend Threshold Semantics (Important)

The backend `evaluate_threshold` logic works as follows:
- `(Some(min), None)` → triggers when `value >= min` (i.e., "alert when above min")
- `(None, Some(max))` → triggers when `value >= max` (NOTE: same direction, not "below max")
- `(Some(min), Some(max))` → triggers when `min <= value <= max` (range)
- `(None, None)` → never triggers

For a "CPU above 80%" alert, the correct field is `min: 80` (not `max: 80`).

The current UI only exposes `max` and labels it "Threshold", which is confusing. The fix should rename fields:
- `min` → label "Alert when ≥" (trigger floor)
- `max` → label "Alert when ≤" (trigger ceiling, only shown when range alerting is needed)

For simplicity and to match the most common use case (alert when a value exceeds a threshold), the primary field shown should be `min` with label "Threshold ≥".

### Changes

Modify `apps/web/src/routes/_authed/settings/alerts.tsx`:

1. **Expand `ruleTypes` array** to include all 19 backend-supported types:
   - Add: `{ label: 'Memory (bytes)', value: 'memory' }`
   - Add: `{ label: 'Swap (bytes)', value: 'swap' }`
   - Add: `{ label: 'Disk (bytes)', value: 'disk' }`
   - Add: `{ label: 'Transfer In (cycle)', value: 'transfer_in_cycle' }`
   - Add: `{ label: 'Transfer Out (cycle)', value: 'transfer_out_cycle' }`
   - Add: `{ label: 'Transfer Total (cycle)', value: 'transfer_all_cycle' }`
   - Add: `{ label: 'Expiration', value: 'expiration' }`

2. **Replace single `max` input** with conditional fields based on `rule_type`:
   - Threshold types: show `min` (label "Threshold ≥", primary) + optional `max` (label "and ≤", for range)
   - `offline`: show `duration` (label "Duration (seconds)", default 60)
   - `transfer_*_cycle`: show `cycle_interval` (dropdown: hour/day/week/month/year) + `cycle_limit` (label "Limit (bytes)")
   - `expiration`: show `duration` (label "Days before expiry", default 7)

3. **Update default `ruleItems`** to use `min` instead of `max`: `{ rule_type: 'cpu', min: 90 }`

### Rule Type → Field Mapping

| rule_type | Fields shown |
|-----------|-------------|
| cpu, memory, swap, disk, load*, *_conn, process, net_*_speed, temperature, gpu | min ("Threshold ≥", primary), max ("and ≤", optional toggle for range) |
| offline | duration (seconds, default 60) |
| transfer_in_cycle, transfer_out_cycle, transfer_all_cycle | cycle_interval (dropdown), cycle_limit (bytes) |
| expiration | duration (days, default 7) |

## Feature Fix 2: Alert State API + UI

### Backend: New API Endpoint

Add to `crates/server/src/router/api/alert.rs`:

```
GET /api/alert-rules/:id/states → Vec<AlertStateResponse>
```

Response:
```json
{
  "data": [
    {
      "server_id": "xxx",
      "server_name": "New Server",
      "first_triggered_at": "2026-03-14T07:00:00Z",
      "last_notified_at": "2026-03-14T07:05:00Z",
      "count": 3,
      "resolved": false
    }
  ]
}
```

Implementation: Query `alert_state` table filtered by `rule_id`, join with `server` table for name, return **all** states (both active and resolved). The `resolved` boolean allows the frontend to display recovery status. Sorted by `updated_at` descending.

### Frontend: Alert State Display

In alerts list (`alerts.tsx`), for each rule show:
- A badge with triggered server count (e.g., "2 triggered") next to the rule name
- Clicking the rule expands to show per-server state: server name, triggered since, notification count, resolved status
- Resolved states shown with green check, active with red indicator

## Feature Fix 3: Notification Form Sensitive Fields

### Changes

Modify `apps/web/src/routes/_authed/settings/notifications.tsx`:

Define a set of sensitive field keys:
```typescript
const SENSITIVE_FIELDS = new Set(['password', 'bot_token', 'device_key'])
```

In the form field rendering loop, use `type="password"` for sensitive fields:
```typescript
type={SENSITIVE_FIELDS.has(key) ? 'password' : 'text'}
```

This ensures SMTP password, Telegram bot token, and Bark device key are masked in the form. Webhook URL and other non-secret fields remain visible.

## E2E Verification Flow

All verification runs against the server-served UI (`localhost:9527`), not the Vite dev server.

### Phase 1: Environment Setup

- Confirm Server + Agent running on `localhost:9527`
- Login via browser as admin
- Note the server_id of the connected Agent (for `cover_type: include`)

### Phase 2: Create Notification Channels (Browser UI)

On `/settings/notifications`, use the Add button to create 4 channels. For each:
1. Click "+ Add" → form appears
2. Select channel type from dropdown
3. Fill fields (copy from `.env.test` — password fields now masked)
4. Click Create
5. Verify channel appears in list (screenshot list view only, no secrets visible)

Then create a Notification Group linking all 4 channels.

### Phase 3: Test Notification

For each channel, click the test send button (paper plane icon):
- Webhook: verify payload arrives at webhook.site (check in separate browser tab)
- Telegram: message appears in chat
- Bark: push notification on device
- Email: message in inbox

### Phase 4: Threshold Alert (CPU ≥ 1%)

1. Navigate to `/settings/alerts`
2. Create rule via UI:
   - Name: "High CPU Test"
   - Add condition: rule_type=cpu, min=1 (triggers when CPU ≥ 1%, i.e., always)
   - trigger_mode: "always"
   - notification_group: select the group from Phase 2
   - cover_type: "include", server_ids: [the connected Agent's server_id]
3. Wait 60-120s for evaluation cycle
4. Verify all 4 channels receive alert notification
5. Expand rule in UI → verify triggered state shows server name + timestamp

### Phase 5: Offline Alert

1. Create rule via UI:
   - Name: "Server Offline Test"
   - Add condition: rule_type=offline, duration=60
   - trigger_mode: "once"
   - notification_group: same group
   - cover_type: "include", server_ids: [same server_id]
2. Stop Agent process
3. Wait ~90s (30s offline detection + 60s evaluation)
4. Verify all 4 channels receive offline alert
5. Verify triggered state in UI
6. Restart Agent
7. Wait ~120s for recovery evaluation
8. Verify alert state shows resolved in UI

### Phase 6: Cleanup

- Delete test alert rules
- Delete notification group
- Delete notification channels
- Update TESTING.md with verification results

## Output

- Screenshots at each verification step (list views only, no credential forms)
- TESTING.md updated with alert/notification verification checklist
- PROGRESS.md updated

## Files Modified

### Frontend
- `apps/web/src/routes/_authed/settings/alerts.tsx` — Expand ruleTypes dropdown (add 7 missing types), replace single `max` field with conditional min/duration/cycle fields, add alert state display section
- `apps/web/src/routes/_authed/settings/notifications.tsx` — Sensitive fields use `type="password"`

### Backend
- `crates/server/src/router/api/alert.rs` — Add `GET /api/alert-rules/:id/states` endpoint with utoipa annotation
- `crates/server/src/service/alert.rs` — Add `list_states(db, rule_id)` service method, add `AlertStateResponse` DTO with `ToSchema` derive
- `crates/server/src/openapi.rs` — Register new path + schema for alert states endpoint

### OpenAPI + Types
- `apps/web/src/lib/api-schema.ts` — Add `AlertStateResponse` type (or regenerate via openapi-typescript)

### Tests
- `crates/server/tests/integration.rs` — Add `test_alert_states_endpoint` (create rule → trigger via report → GET states → verify)
- `apps/web/src/routes/_authed/settings/alerts.test.tsx` — Add vitest for conditional field rendering (threshold → shows min, offline → shows duration, transfer → shows cycle fields)

### Config
- `.env.test` — Credential template (gitignored)
- `.gitignore` — Add `.env.test` entry

## Dependencies

- Server + Agent running on localhost:9527
- Real credentials for Telegram, Bark, Email
- webhook.site account (free) for Webhook payload inspection
- agent-browser installed (via npx)
