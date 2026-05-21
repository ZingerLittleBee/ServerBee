# Firewall Blocklist Design

**Status:** Draft
**Date:** 2026-05-21
**Branch:** `abuja`
**Depends on:** [Security Events](2026-05-21-security-events-design.md)

## Goal

Let administrators block inbound traffic from individual IPs or CIDRs across one or more agents, either manually (UI / API) or automatically as an action on `ssh_brute_force_detected` / `port_scan_detected` alert rules. Server is the single source of truth; each agent applies the list via `nftables`.

## Non-goals (v1)

- Domain-name blocking (DNS resolution + re-resolution loop)
- Time-based expiration or auto-unblock
- Outbound / forward chain filtering
- iptables / ipset / fail2ban / cloud-SG backends
- Dry-run / shadow mode
- Per-tenant or per-team isolation

These constraints are deliberate to keep v1 small and auditable. The data model leaves room for `expires_at` / `direction` extensions, but no UI or executor hook is added.

---

## 1. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                         SERVER (source of truth)              │
│                                                               │
│  block_list table   ◄──┐                                       │
│       ▲               │  auto-block path                       │
│       │ REST           └── alert_rule.actions_json             │
│       │                   triggered by SecurityService          │
│  ┌────┴────────┐                                                │
│  │ FirewallSvc │ ── BlocklistSync / Add / Remove (WS) ────►    │
│  └─────────────┘                                                │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│                          AGENT (executor)                      │
│                                                               │
│   ws::agent handler ──► FirewallManager ──► `nft` CLI         │
│                              │ guardrail re-check               │
│                              │ in-memory desired set            │
│                              ▼                                  │
│              inet table `serverbee`                             │
│              ├ set  block_v4 (ipv4_addr; interval)              │
│              ├ set  block_v6 (ipv6_addr; interval)              │
│              └ chain input (hook input, priority -10)           │
│                    ip  saddr @block_v4 drop                     │
│                    ip6 saddr @block_v6 drop                     │
└──────────────────────────────────────────────────────────────┘
```

- New capability `CAP_FIREWALL_BLOCK = 1 << 9 = 512`. **High risk, disabled by default** (requires root + `nft` CLI on the host).
- Linux-only. Non-Linux agents ignore the capability bit.
- Agent does **not** persist the blocklist locally. On every reconnect the server pushes a full `BlocklistSync` and the agent reconciles its nft set against it.

---

## 2. Data Model

### 2.1 New table `block_list`

| Column | Type | Notes |
|---|---|---|
| `id` | TEXT PRIMARY KEY | UUID v4 |
| `target` | TEXT NOT NULL | `1.2.3.4` / `10.0.0.0/24` / `2001:db8::/32` |
| `family` | INTEGER NOT NULL | `4` or `6` |
| `cover_type` | TEXT NOT NULL | `all` / `include` / `exclude` (reuses `alert_rule` semantics) |
| `server_ids_json` | TEXT NULL | JSON array, used by `include` / `exclude` |
| `comment` | TEXT NULL | free-text or rendered template |
| `origin` | TEXT NOT NULL | `manual` or `auto` |
| `origin_event_id` | TEXT NULL | FK-like to `security_event.id` (auto only) |
| `origin_rule_id` | TEXT NULL | FK-like to `alert_rule.id` (auto only) |
| `created_by` | TEXT NULL | `user.id`; NULL when `origin = auto` |
| `created_at` | TIMESTAMP NOT NULL | UTC |

Indexes:

- `UNIQUE(target)` — same target must not appear twice. Prevents conflicting cover_type rows.
- `INDEX(created_at DESC)` — list pagination.
- `INDEX(origin)` — UI filter.

### 2.2 Extend `alert_rule`

New nullable column `actions_json` (TEXT). Stored format:

```json
[
  {
    "type": "block_source_ip",
    "cover_type": "all",
    "server_ids_json": null,
    "comment": "Auto-block from {rule_name}"
  }
]
```

Only one action allowed per rule. Only `block_source_ip` defined in v1.

### 2.3 New config `[firewall]` (server.toml)

| Key | Type | Default | Description |
|---|---|---|---|
| `firewall.allow_list` | string[] | `[]` | CIDRs the server refuses to enqueue into any block_list (third guardrail tier) |

Env var: `SERVERBEE_FIREWALL__ALLOW_LIST` (comma-separated).

### 2.4 Migrations

- `m20260521_000027_create_block_list` — create table + indexes.
- `m20260521_000028_extend_alert_rule_actions` — add nullable `actions_json` column.
- `m20260521_000029_extend_capability_mask` — bump `CAP_VALID_MASK` reference (data unchanged; this is mostly a comment/marker migration since the column is `INTEGER NOT NULL DEFAULT`).

Only `up()` is implemented; `down()` returns `Ok(())` (matches existing convention).

---

## 3. Protocol

`crates/common/src/protocol.rs`:

```rust
pub enum ServerMessage {
    // existing variants...
    BlocklistSync { entries: Vec<BlockEntry> },
    BlocklistAdd { entry: BlockEntry },
    BlocklistRemove { id: String },
}

pub enum AgentMessage {
    // existing variants...
    BlocklistAck {
        id: String,
        applied: bool,
        reason: Option<String>, // populated only when applied = false
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockEntry {
    pub id: String,
    pub target: String,
    pub family: u8, // 4 or 6
}
```

`BlockEntry` does **not** carry `cover_type` / `server_ids_json` — the server already filters before sending, so the agent applies whatever it receives.

`BlocklistAck` is sent **only when `applied = false`**. Successful application is silent to keep WS traffic minimal under steady state.

---

## 4. Guardrails (3 tiers)

Misconfigured rules can lock the operator out. Three independent checks reject the same set; any one tripping aborts the insert.

### 4.1 Tier 1 — server-side hard-coded

In `service::firewall::is_protected(target)`. Rejects any target that **overlaps** any of:

```
IPv4: 127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16,
      169.254.0.0/16, 0.0.0.0/8, 224.0.0.0/4
IPv6: ::1/128, fc00::/7, fe80::/10, ff00::/8, ::/128
```

"Overlap" means either direction: `target ⊆ protected` or `protected ⊆ target`. Implemented with `ipnet::IpNet::contains`.

### 4.2 Tier 2 — server-side dynamic

Built at startup from:

| Source | Contents |
|---|---|
| `firewall.allow_list` config | User-configured trusted CIDRs |
| `server.trusted_proxies` config | Existing reverse-proxy whitelist |
| Each agent's reported external IP | Maintained in `Arc<RwLock<HashSet<IpAddr>>>`, updated from `SystemInfo` / `IpChanged` |

Same overlap test as tier 1.

### 4.3 Tier 3 — agent-side

Before issuing `nft add element` the agent re-runs **tier 1 only + its own external IP** (the agent does not know the server's full allow_list). On hit, the agent sends `BlocklistAck { applied: false, reason: "guardrail: …" }`.

### 4.4 Audit on rejection

| Action | When |
|---|---|
| `firewall_block_rejected_server` | Tier 1/2 reject |
| `firewall_block_rejected_agent` | Tier 3 reject |

Detail JSON includes `target`, `reason`, and (for auto-block) `origin_rule_id`.

### 4.5 UI feedback

`POST /api/firewall/blocks` returns `409 Conflict` with the reason; form surfaces a localized error.

---

## 5. Auto-block path

### 5.1 Hook into SecurityService

Inside the existing `SecurityService::evaluate_rules` loop (in `crates/server/src/service/security.rs`), after `send_group` and the dedupe `mark_triggered` call, iterate the rule's `actions_json`:

```rust
for action in actions {
    match action {
        BlockSourceIp { cover_type, server_ids_json, comment } => {
            FirewallService::auto_block(
                &self.db,
                rule, payload, action,
                &state.agent_manager,
            ).await?;
        }
    }
}
```

### 5.2 `FirewallService::auto_block` steps

1. **Dedup check**: `SELECT id FROM block_list WHERE target = $1`. Existing row → skip silently (no second audit entry, no re-broadcast). Rationale: repeated brute-force events from the same IP must not generate row churn.
2. **Guardrail** (tier 1 + 2). Reject → audit `firewall_block_rejected_server`, return without error so notification still went out normally.
3. **Insert** `block_list` row with `origin = "auto"`, `origin_event_id`, `origin_rule_id`, rendered `comment` (placeholders `{rule_name}`, `{event_type}`, `{severity}`).
4. **Audit** `firewall_block_created` with detail `{ id, target, origin, rule_id }`.
5. **Push to covered online agents**: iterate `agent_manager.online_agents()`, filter by `rule_covers_server(row.cover_type, row.server_ids_json, &agent_id)` AND by `has_capability(caps, CAP_FIREWALL_BLOCK)`, send `BlocklistAdd`. Offline agents will pick it up on next `BlocklistSync`.

### 5.3 Validator

In `service::alert::validate_alert_rule`:

```rust
if !actions.is_empty() {
    if actions.len() > 1 {
        return Err("at most one action per alert_rule");
    }
    for a in actions {
        if let AlertRuleAction::BlockSourceIp { .. } = a {
            let allowed = ["ssh_brute_force_detected", "port_scan_detected"];
            if !rules.iter().all(|r| allowed.contains(&r.rule_type.as_str())) {
                return Err("block_source_ip is only allowed on \
                            ssh_brute_force_detected / port_scan_detected rules");
            }
        }
    }
}
```

**Intentionally forbidden**: `ssh_new_ip_login` + auto-block. First-time legitimate logins would lock real users out.

### 5.4 Cascade on rule delete

`DELETE /api/alert/rules/:id` does **not** cascade into `block_list`. Removing the rule does not mean "I now trust those IPs". `origin_rule_id` becomes a dangling reference and the UI shows "Rule deleted" next to the entry. Audit logs `firewall_auto_rule_removed`.

---

## 6. Reconcile semantics

### 6.1 Server side

**On agent WS connect** (after `Hello` and the existing `CapabilitiesSync`):

```rust
if has_capability(caps, CAP_FIREWALL_BLOCK) {
    let entries = FirewallService::list_for_server(&state.db, server_id).await?;
    let _ = ws_tx.send(ServerMessage::BlocklistSync { entries }).await;
}
```

`list_for_server` filters all `block_list` rows with `rule_covers_server`.

**On CRUD**: send `BlocklistAdd` / `BlocklistRemove` to every online agent currently covered. Offline agents are not chased — they reconcile on next Sync.

**On `BlocklistAck { applied: false, .. }`**: write `firewall_block_rejected_agent` audit, log warn. Do **not** delete the row — another agent may apply it fine (different external IP).

### 6.2 Agent side `FirewallManager`

```rust
pub struct FirewallManager {
    /// Desired state per server: id → target
    desired: HashMap<String, BlockEntry>,
    /// nft resources are initialized once per process
    nft_ready: bool,
    external_ip: OnceLock<IpAddr>,
}
```

Message handling:

```rust
match msg {
    BlocklistSync { entries } => {
        self.ensure_nft_resources()?;
        let desired_now: HashMap<_, _> =
            entries.iter().map(|e| (e.id.clone(), e.clone())).collect();
        let to_add:    Vec<&BlockEntry> = desired_now.values()
            .filter(|e| !self.desired.contains_key(&e.id))
            .collect();
        let to_remove: Vec<BlockEntry>  = self.desired.values()
            .filter(|e| !desired_now.contains_key(&e.id))
            .cloned().collect();
        for e in to_add    { self.apply_add(e).await; }
        for e in to_remove { self.apply_remove(&e).await; }
        self.desired = desired_now;
    }
    BlocklistAdd { entry } => {
        self.ensure_nft_resources()?;
        if let Err(r) = self.tier3_guardrail(&entry.target) {
            ack(entry.id, false, Some(r));
            return;
        }
        self.nft_add_element(&entry).await?;
        self.desired.insert(entry.id.clone(), entry);
    }
    BlocklistRemove { id } => {
        if let Some(entry) = self.desired.remove(&id) {
            let _ = self.nft_del_element(&entry).await;
        }
    }
}
```

### 6.3 `nft` invocation

Idempotent init (run once per process, after first `BlocklistSync`):

```bash
nft add table inet serverbee
nft add set inet serverbee block_v4 '{ type ipv4_addr; flags interval; }'
nft add set inet serverbee block_v6 '{ type ipv6_addr; flags interval; }'
nft add chain inet serverbee input '{ type filter hook input priority -10; }'
nft add rule  inet serverbee input ip  saddr @block_v4 drop
nft add rule  inet serverbee input ip6 saddr @block_v6 drop
```

The two `add rule` invocations are guarded with `nft -j list chain inet serverbee input` to detect existing identical rules.

Per-entry add/remove:

```bash
nft add element    inet serverbee block_v4 '{ 1.2.3.4 }'
nft delete element inet serverbee block_v4 '{ 1.2.3.4 }'
```

All commands run via `tokio::process::Command`. Failures bubble up as `BlocklistAck { applied: false, reason }`.

### 6.4 Failure matrix

| Scenario | Behavior |
|---|---|
| Agent offline at CRUD time | No push; next `BlocklistSync` carries the delta |
| Agent ack `applied = false` | Server audits, UI marks entry as "not applied on srv-X" (derived from audit) |
| `nft` missing or unprivileged | Agent emits `CapabilityDenied`; server UI shows "firewall capability unavailable" |
| Server restart | Agent reconnects → full Sync → state is restored |
| Agent restart | Same — agent does not persist blocklist |
| Duplicate target | `UNIQUE(target)` rejects at insert; auto-block dedup skips |
| Mid-Sync WS drop | Sync abandoned; next reconnect retries from scratch |

---

## 7. REST API

| Method | Path | Auth | Notes |
|---|---|---|---|
| `GET` | `/api/firewall/blocks` | session / api_key | Cursor pagination. Query: `cursor`, `origin`, `server_id`, `target_q` |
| `GET` | `/api/firewall/blocks/:id` | session / api_key | Full row + audit trail |
| `POST` | `/api/firewall/blocks` | **admin only** | Body: `{ target, cover_type, server_ids?, comment? }` |
| `DELETE` | `/api/firewall/blocks/:id` | **admin only** | |
| `GET` | `/api/firewall/stats` | session / api_key | KPIs: total, by origin, by family, 7-day new-entry timeline |

No `PATCH`: rows are immutable. To change a target, delete and recreate. This avoids ambiguity about whether the change should refresh agents.

All endpoints annotated `#[utoipa::path]` with `ToSchema` DTOs; appear in Swagger UI at `/swagger-ui/`.

---

## 8. Browser real-time

`crates/common/src/protocol.rs::BrowserMessage`:

```rust
BlocklistChanged {
    kind: String, // "created" | "deleted"
    entry: BlockListItem,
}
```

Frontend `apps/web/src/hooks/use-servers-ws.ts` reacts:

- Invalidate `['firewall', 'blocks']` (debounced 1s)
- Invalidate `['firewall', 'stats']`

---

## 9. Frontend

### 9.1 Routes

- `/settings/firewall` — new page, lives under Settings (alongside Alerts / API Keys).
  - Tab **Blocklist** (default): KPI cards (total / auto / manual / IPv6), filter bar, table, row "Delete" action, "Add" drawer.
  - Tab **Activity**: stream of `firewall_*` audit log entries (reuses existing audit log component).

### 9.2 Cross-page integrations

- `/security` event table row → menu item **"Block source IP"** (admin only) → opens the Add drawer pre-filled with `target = event.source_ip`, `cover_type = "all"`.
- Server detail → **Security** tab — same menu item, but `cover_type` pre-filled `include`, `server_ids = [current]`.
- `/settings/alerts` rule editor — when `rules` are all `ssh_brute_force_detected` / `port_scan_detected`, show "Auto-block source IP" collapsible card containing `cover_type` + `server_ids` + comment template.
- Alert presets (Brute Force, Port Scan) — add checkbox **"Also auto-block source IP"** (default checked). Generates `actions_json` automatically.

### 9.3 Components

```
apps/web/src/components/firewall/
├── kpi-cards.tsx
├── block-table.tsx
├── add-block-drawer.tsx
├── delete-block-dialog.tsx
└── activity-log.tsx          // thin wrapper over existing audit-log table
```

### 9.4 i18n

New file `apps/web/src/locales/{en,zh}/firewall.json`, approx. 30 keys (`blocklist.title`, `target`, `family`, `origin_*`, `cover_*`, `add`, `delete`, `guardrail_rejected`, `auto_block_card_title`, …).

---

## 10. Documentation

| File | Change |
|---|---|
| `ENV.md` | New `[firewall]` subsection: `SERVERBEE_FIREWALL__ALLOW_LIST` |
| `apps/docs/content/docs/{en,cn}/configuration.mdx` | Mirror ENV.md change |
| `apps/docs/content/docs/{en,cn}/capabilities.mdx` | Add `CAP_FIREWALL_BLOCK (512)`, High-risk tier, examples updated |
| `apps/docs/content/docs/{en,cn}/security-events.mdx` | New section "Auto-block source IP" — only on brute-force / port-scan rules |
| **New** `apps/docs/content/docs/{en,cn}/firewall.mdx` | Full feature guide: nftables dependency, guardrails, manual CRUD, auto-block, troubleshooting (`nft -L`, ack failures) |
| `apps/docs/content/docs/{en,cn}/meta.json` | Add `firewall` to the Features section |
| **New** `tests/firewall-block.md` | E2E manual checklist |
| `tests/README.md` | Index `firewall-block.md` |

---

## 11. Test plan

### 11.1 Rust unit

- `service::firewall::is_protected` — IPv4 / IPv6 overlap edge cases (target ⊃ protected, target ⊂ protected, disjoint)
- `service::firewall::FirewallService::auto_block` — dedup, guardrail rejection, normal path (with in-memory DB + mock agent manager)
- `agent::firewall::nft` — mock `tokio::process::Command` (or extract a `NftExecutor` trait): verify generated command strings, error surfacing
- `agent::firewall::reconcile` — diff algorithm: empty → full, full → empty, partial overlap, idempotent re-sync
- `service::alert::validate_alert_rule` — action + rule_type compatibility, max 1 action, forbidden combinations

### 11.2 Integration (`crates/server/tests/integration.rs`)

- `POST /api/firewall/blocks` → row inserted, mock agent receives `BlocklistAdd`, ack written
- Guardrail rejection paths return 409 with localized reason key
- Auto-block end-to-end: insert security_event → matching rule with action → block_list row appears
- Agent reconnect → `BlocklistSync` carries expected entries
- DELETE → row gone, mock agent receives `BlocklistRemove`, ack
- Member (non-admin) on `POST` / `DELETE` → 403

### 11.3 E2E manual (`tests/firewall-block.md`)

- Manual create from UI → `nft list ruleset` shows the element
- Trigger brute-force from external host → entry appears auto, third-party ping/curl to target VPS port 22 drops
- Lock-out drill: attempt to block `127.0.0.1` → expect 409 server-side
- Lock-out drill: attempt to block VPS's own external IP → expect 409 (tier 2)
- Agent restart → `nft list table inet serverbee` re-created with same elements
- VPS without `nft` installed → agent logs `firewall capability unavailable`, server UI shows capability disabled

---

## 12. Rollout

- No legacy data to migrate. New columns/tables introduced with non-breaking defaults.
- `CAP_FIREWALL_BLOCK` defaults to **off** for existing and new agents. Admins must explicitly opt in per agent (Settings → Capabilities).
- Agent without the capability bit silently ignores `BlocklistSync` / `Add` / `Remove`.
- Versioning: agents that pre-date this branch will not have the `BlocklistSync` variant — protocol uses `#[serde(other)]` fallthrough so old agents ignore unknown messages. Server side checks the agent's protocol version stored at `Hello`.

---

## 13. Open questions deferred to plan stage

- Audit log retention: should `firewall_*` events follow `retention.audit_logs_days` (180d) or get their own knob? **Tentative: reuse audit_logs retention.**
- Should `BlocklistAck { applied: true }` be sent (currently silent)? Trade-off: confirmation latency vs WS noise. **Tentative: keep silent; rely on UI inference from audit + capability state.**
- Should we ship a "panic unblock" emergency endpoint that clears the entire set without confirmation? **Tentative: no — `DELETE` per entry is sufficient and `nft flush set inet serverbee block_v4` is a one-liner if an operator needs it from shell.**

These three points are noted but do not block plan writing.
