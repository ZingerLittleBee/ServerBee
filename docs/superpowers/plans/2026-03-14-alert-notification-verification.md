# Alert & Notification E2E Verification Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix alert form fields, add alert state API/UI, mask notification secrets, then E2E verify the complete alert→notification pipeline with all 4 channels.

**Architecture:** 3 feature fixes (frontend alert form, backend alert state endpoint, notification password masking) followed by browser-based E2E verification of the full pipeline. Backend adds one new endpoint (`GET /api/alert-rules/:id/states`) with OpenAPI annotation. Frontend adds conditional form fields and an expandable alert state section.

**Tech Stack:** Rust (Axum, sea-orm, utoipa), TypeScript (React, TanStack Query, vitest), agent-browser (E2E)

**Spec:** `docs/superpowers/specs/2026-03-14-alert-notification-e2e-verification.md`

---

## Task 1: Notification form — mask sensitive fields

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/notifications.tsx`

- [ ] **Step 1: Add SENSITIVE_FIELDS set and update input type**

In `apps/web/src/routes/_authed/settings/notifications.tsx`, add before the component function:

```typescript
const SENSITIVE_FIELDS = new Set(['password', 'bot_token', 'device_key'])
```

Then change the form field rendering (around line 192) from:

```typescript
type="text"
```

to:

```typescript
type={SENSITIVE_FIELDS.has(key) ? 'password' : 'text'}
```

- [ ] **Step 2: Verify build**

Run: `cd apps/web && bun run build 2>&1 | tail -3`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/notifications.tsx
git commit -m "fix(web): mask sensitive fields in notification form (password, bot_token, device_key)"
```

---

## Task 2: Alert form — expand rule types + conditional fields

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`

- [ ] **Step 1: Expand ruleTypes array**

Replace the existing `ruleTypes` array (lines 18-31) with:

```typescript
const THRESHOLD_TYPES = new Set([
  'cpu', 'memory', 'swap', 'disk', 'load1', 'load5', 'load15',
  'tcp_conn', 'udp_conn', 'process', 'net_in_speed', 'net_out_speed',
  'temperature', 'gpu'
])

const CYCLE_TYPES = new Set(['transfer_in_cycle', 'transfer_out_cycle', 'transfer_all_cycle'])

const ruleTypes = [
  { label: 'CPU %', value: 'cpu' },
  { label: 'Memory (bytes)', value: 'memory' },
  { label: 'Swap (bytes)', value: 'swap' },
  { label: 'Disk (bytes)', value: 'disk' },
  { label: 'Load 1m', value: 'load1' },
  { label: 'Load 5m', value: 'load5' },
  { label: 'Load 15m', value: 'load15' },
  { label: 'TCP Connections', value: 'tcp_conn' },
  { label: 'UDP Connections', value: 'udp_conn' },
  { label: 'Processes', value: 'process' },
  { label: 'Network In (B/s)', value: 'net_in_speed' },
  { label: 'Network Out (B/s)', value: 'net_out_speed' },
  { label: 'Temperature', value: 'temperature' },
  { label: 'GPU %', value: 'gpu' },
  { label: 'Offline', value: 'offline' },
  { label: 'Transfer In (cycle)', value: 'transfer_in_cycle' },
  { label: 'Transfer Out (cycle)', value: 'transfer_out_cycle' },
  { label: 'Transfer Total (cycle)', value: 'transfer_all_cycle' },
  { label: 'Expiration', value: 'expiration' }
]
```

- [ ] **Step 2: Update default ruleItems to use `min`**

Change line 39 from:
```typescript
const [ruleItems, setRuleItems] = useState<AlertRuleItem[]>([{ rule_type: 'cpu', max: 90 }])
```
to:
```typescript
const [ruleItems, setRuleItems] = useState<AlertRuleItem[]>([{ rule_type: 'cpu', min: 90 }])
```

Also update `resetForm` (line 93) and `addRuleItem` (line 115) the same way.

- [ ] **Step 3: Replace the single max input with conditional fields**

Replace the rule item rendering block (the part inside `{ruleItems.map(...)}`after the `<select>` dropdown, lines 239-247) with conditional field rendering:

```tsx
{THRESHOLD_TYPES.has(item.rule_type) && (
  <>
    <input
      className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
      onChange={(e) => updateRuleItem(index, 'min', Number.parseFloat(e.target.value) || 0)}
      placeholder="Threshold ≥"
      type="number"
      value={item.min ?? ''}
    />
    <input
      className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
      onChange={(e) => updateRuleItem(index, 'max', Number.parseFloat(e.target.value) || 0)}
      placeholder="and ≤ (optional)"
      type="number"
      value={item.max ?? ''}
    />
  </>
)}
{item.rule_type === 'offline' && (
  <input
    className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
    onChange={(e) => updateRuleItem(index, 'duration', Number.parseInt(e.target.value) || 60)}
    placeholder="Duration (s)"
    type="number"
    value={item.duration ?? 60}
  />
)}
{item.rule_type === 'expiration' && (
  <input
    className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
    onChange={(e) => updateRuleItem(index, 'duration', Number.parseInt(e.target.value) || 7)}
    placeholder="Days before"
    type="number"
    value={item.duration ?? 7}
  />
)}
{CYCLE_TYPES.has(item.rule_type) && (
  <>
    <select
      className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
      onChange={(e) => updateRuleItem(index, 'cycle_interval', e.target.value)}
      value={item.cycle_interval ?? 'month'}
    >
      <option value="hour">Hour</option>
      <option value="day">Day</option>
      <option value="week">Week</option>
      <option value="month">Month</option>
      <option value="year">Year</option>
    </select>
    <input
      className="flex h-9 w-28 rounded-md border border-input bg-transparent px-3 py-1 text-sm"
      onChange={(e) => updateRuleItem(index, 'cycle_limit', Number.parseInt(e.target.value) || 0)}
      placeholder="Limit (bytes)"
      type="number"
      value={item.cycle_limit ?? ''}
    />
  </>
)}
```

- [ ] **Step 4: Update the rule display in the list**

Update the rule summary text (line 294) from:
```typescript
{items.map((item) => `${item.rule_type}${item.max ? ` >= ${item.max}` : ''}`).join(' AND ')}
```
to:
```typescript
{items.map((item) => {
  if (item.rule_type === 'offline') return `offline ${item.duration ?? 60}s`
  if (item.rule_type === 'expiration') return `expires in ${item.duration ?? 7}d`
  if (item.cycle_limit) return `${item.rule_type} > ${item.cycle_limit}B/${item.cycle_interval ?? 'month'}`
  if (item.min && item.max) return `${item.rule_type} [${item.min}, ${item.max}]`
  if (item.min) return `${item.rule_type} ≥ ${item.min}`
  return item.rule_type
}).join(' AND ')}
```

- [ ] **Step 5: Verify build + lint**

Run: `cd apps/web && bun run build && bun x ultracite check 2>&1 | tail -5`
Expected: Build succeeds, lint clean

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/routes/_authed/settings/alerts.tsx
git commit -m "feat(web): expand alert form with all rule types and conditional fields (min, duration, cycle)"
```

---

## Task 3: Backend — alert states endpoint

**Files:**
- Modify: `crates/server/src/service/alert.rs`
- Modify: `crates/server/src/router/api/alert.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Add AlertStateResponse DTO and list_states method**

In `crates/server/src/service/alert.rs`, add after the existing DTOs (after `UpdateAlertRule`):

```rust
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertStateResponse {
    pub server_id: String,
    pub server_name: String,
    pub first_triggered_at: chrono::DateTime<chrono::Utc>,
    pub last_notified_at: chrono::DateTime<chrono::Utc>,
    pub count: i32,
    pub resolved: bool,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

Add service method:

```rust
impl AlertService {
    pub async fn list_states(
        db: &DatabaseConnection,
        rule_id: &str,
    ) -> Result<Vec<AlertStateResponse>, AppError> {
        use crate::entity::{alert_state, server};

        let states = alert_state::Entity::find()
            .filter(alert_state::Column::RuleId.eq(rule_id))
            .order_by_desc(alert_state::Column::UpdatedAt)
            .all(db)
            .await
            .map_err(AppError::from)?;

        let mut result = Vec::new();
        for state in states {
            let server_name = server::Entity::find_by_id(&state.server_id)
                .one(db)
                .await
                .map_err(AppError::from)?
                .map(|s| s.name)
                .unwrap_or_else(|| "Unknown".to_string());

            result.push(AlertStateResponse {
                server_id: state.server_id,
                server_name,
                first_triggered_at: state.first_triggered_at,
                last_notified_at: state.last_notified_at,
                count: state.count,
                resolved: state.resolved,
                resolved_at: state.resolved_at,
            });
        }
        Ok(result)
    }
}
```

- [ ] **Step 2: Add route handler**

In `crates/server/src/router/api/alert.rs`, add the route:

```rust
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alert-rules", get(list_rules))
        .route("/alert-rules", post(create_rule))
        .route("/alert-rules/{id}", get(get_rule))
        .route("/alert-rules/{id}", put(update_rule))
        .route("/alert-rules/{id}", delete(delete_rule))
        .route("/alert-rules/{id}/states", get(list_states))  // NEW
}
```

Add handler function:

```rust
#[utoipa::path(
    get,
    path = "/api/alert-rules/{id}/states",
    tag = "alert-rules",
    params(("id" = String, Path, description = "Alert rule ID")),
    responses(
        (status = 200, description = "Alert states for this rule", body = Vec<AlertStateResponse>),
        (status = 404, description = "Rule not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_states(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<AlertStateResponse>>>, AppError> {
    let states = AlertService::list_states(&state.db, &id).await?;
    ok(states)
}
```

Add `AlertStateResponse` to the import line at the top:
```rust
use crate::service::alert::{AlertService, AlertStateResponse, CreateAlertRule, UpdateAlertRule};
```

- [ ] **Step 3: Register in OpenAPI**

In `crates/server/src/openapi.rs`, add to `paths(...)`:
```rust
crate::router::api::alert::list_states,
```

Add to `schemas(...)`:
```rust
crate::service::alert::AlertStateResponse,
```

- [ ] **Step 4: Verify build**

Run: `cargo build --workspace 2>&1 | tail -3`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/alert.rs crates/server/src/router/api/alert.rs crates/server/src/openapi.rs
git commit -m "feat(server): add GET /api/alert-rules/:id/states endpoint"
```

---

## Task 4: Frontend — alert state display

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`
- Modify: `apps/web/src/lib/api-schema.ts`

- [ ] **Step 1: Add AlertStateResponse type**

In `apps/web/src/lib/api-schema.ts`, add after the AlertRule types:

```typescript
export interface AlertStateResponse {
  server_id: string
  server_name: string
  first_triggered_at: string
  last_notified_at: string
  count: number
  resolved: boolean
  resolved_at: string | null
}
```

- [ ] **Step 2: Add expandable state section to alert list**

In `alerts.tsx`, add state to track expanded rules:

```typescript
const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null)
```

Add a query for states (inside the component, conditionally enabled):

```typescript
const { data: states } = useQuery<AlertStateResponse[]>({
  queryKey: ['alert-rule-states', expandedRuleId],
  queryFn: () => api.get<AlertStateResponse[]>(`/api/alert-rules/${expandedRuleId}/states`),
  enabled: !!expandedRuleId,
  refetchInterval: 10_000
})
```

Import `AlertStateResponse` from api-schema.

In the rule list rendering, add a triggered badge next to the rule name, and an expandable section. Wrap the existing rule `<div>` content:

After the rule name paragraph, add a clickable badge:
```tsx
<button
  className="ml-2 rounded-full bg-destructive/10 px-2 py-0.5 text-destructive text-xs"
  onClick={(e) => {
    e.stopPropagation()
    setExpandedRuleId(expandedRuleId === rule.id ? null : rule.id)
  }}
  type="button"
>
  States
</button>
```

After the rule row `</div>`, add an expanded section:
```tsx
{expandedRuleId === rule.id && (
  <div className="border-t bg-muted/20 px-4 py-2">
    {states && states.length > 0 ? (
      <div className="space-y-1">
        {states.map((s) => (
          <div className="flex items-center justify-between text-xs" key={s.server_id}>
            <span className="flex items-center gap-2">
              <span className={`size-2 rounded-full ${s.resolved ? 'bg-green-500' : 'bg-red-500'}`} />
              {s.server_name}
            </span>
            <span className="text-muted-foreground">
              {s.resolved ? 'Resolved' : `Triggered (${s.count}x)`}
              {' · '}
              {new Date(s.first_triggered_at).toLocaleString()}
            </span>
          </div>
        ))}
      </div>
    ) : (
      <p className="text-muted-foreground text-xs">No triggered states</p>
    )}
  </div>
)}
```

- [ ] **Step 3: Verify build + lint**

Run: `cd apps/web && bun run build && bun x ultracite check 2>&1 | tail -5`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/alerts.tsx apps/web/src/lib/api-schema.ts
git commit -m "feat(web): add alert state display with expandable per-server status"
```

---

## Task 5: Config + gitignore

**Files:**
- Create: `.env.test`
- Modify: `.gitignore`

- [ ] **Step 1: Create .env.test template**

```bash
# Webhook (use webhook.site for observable payloads)
TEST_WEBHOOK_URL=https://webhook.site/your-unique-id

# Telegram
TEST_TELEGRAM_BOT_TOKEN=
TEST_TELEGRAM_CHAT_ID=

# Bark
TEST_BARK_SERVER_URL=https://api.day.app
TEST_BARK_DEVICE_KEY=

# Email (SMTP)
TEST_EMAIL_SMTP_HOST=smtp.gmail.com
TEST_EMAIL_SMTP_PORT=587
TEST_EMAIL_USERNAME=
TEST_EMAIL_PASSWORD=
TEST_EMAIL_FROM=
TEST_EMAIL_TO=
```

- [ ] **Step 2: Add to .gitignore**

Add `.env.test` to `.gitignore`.

- [ ] **Step 3: Commit (only .gitignore, NOT .env.test)**

```bash
git add .gitignore
git commit -m "chore: add .env.test to gitignore"
```

---

## Task 6: Integration test for alert states endpoint

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Add test**

Append to `integration.rs`:

```rust
#[tokio::test]
async fn test_alert_states_endpoint() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create an alert rule
    let resp = client
        .post(format!("{base_url}/api/alert-rules"))
        .json(&serde_json::json!({
            "name": "Test States",
            "rules": [{"rule_type": "cpu", "min": 1.0}],
            "cover_type": "all",
            "trigger_mode": "always"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let rule_id = body["data"]["id"].as_str().unwrap();

    // Query states (should be empty initially)
    let resp = client
        .get(format!("{base_url}/api/alert-rules/{rule_id}/states"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let states = body["data"].as_array().unwrap();
    assert!(states.is_empty());

    // Cleanup
    client
        .delete(format!("{base_url}/api/alert-rules/{rule_id}"))
        .send()
        .await
        .unwrap();
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p serverbee-server --test integration test_alert_states_endpoint -- --nocapture`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test: add integration test for alert states endpoint"
```

---

## Task 7: E2E verification — full pipeline

**Prerequisites:** Server + Agent running on localhost:9527. `.env.test` filled with real credentials.

This task is manual browser-based verification using `npx agent-browser`. Rebuild and restart the server with the new code first:

```bash
cd apps/web && bun run build && cd ../..
# Restart server to pick up new embedded frontend + new endpoint
```

- [ ] **Step 1: Login and verify notifications page Add button works**

Open `http://localhost:9527/settings/notifications`, click "+ Add", verify form appears.

- [ ] **Step 2: Create 4 notification channels via UI**

Create Webhook, Telegram, Bark, Email channels. Screenshot list view after each (NOT form view).

- [ ] **Step 3: Create notification group linking all 4**

- [ ] **Step 4: Test notification for each channel**

Click paper plane icon for each. Verify receipt externally (webhook.site, Telegram chat, Bark push, email inbox).

- [ ] **Step 5: Create threshold alert rule (CPU ≥ 1%)**

Navigate to `/settings/alerts`, create rule: name="High CPU Test", rule_type=cpu, min=1, trigger_mode=always, notification_group=the group, cover_type=include, select the server.

- [ ] **Step 6: Wait 60-120s and verify alert triggered**

Check all 4 channels received notification. Click "States" on the rule → verify server appears as triggered.

- [ ] **Step 7: Create offline alert rule**

Create rule: name="Server Offline Test", rule_type=offline, duration=60, trigger_mode=once, cover_type=include, same server.

- [ ] **Step 8: Stop Agent and verify offline alert**

Kill Agent process. Wait ~90s. Verify all 4 channels received offline notification. Check States shows triggered.

- [ ] **Step 9: Restart Agent and verify recovery**

Restart Agent. Wait ~120s. Check States shows resolved (green indicator).

- [ ] **Step 10: Cleanup and document**

Delete test rules, notification group, notification channels. Update TESTING.md with results.

- [ ] **Step 11: Commit docs**

```bash
git add TESTING.md
git commit -m "docs: update TESTING.md with alert/notification E2E verification results"
```
