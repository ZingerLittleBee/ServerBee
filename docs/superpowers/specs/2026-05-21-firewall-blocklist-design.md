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
| `target` | TEXT NOT NULL | Canonical `IpNet` string. See § 2.5 |
| `family` | INTEGER NOT NULL | `4` or `6`, **derived** from `target` parse, not accepted from clients |
| `cover_type` | TEXT NOT NULL | `all` / `include` / `exclude` (reuses `alert_rule` semantics) |
| `server_ids_json` | TEXT NULL | JSON array, used by `include` / `exclude` |
| `comment` | TEXT NULL | free-text or rendered template |
| `origin` | TEXT NOT NULL | `manual` or `auto` |
| `origin_event_id` | TEXT NULL | FK-like to `security_event.id` (auto only) |
| `origin_rule_id` | TEXT NULL | FK-like to `alert_rule.id` (auto only) |
| `created_by` | TEXT NULL | `user.id`; NULL when `origin = auto` |
| `created_at` | TIMESTAMP NOT NULL | UTC |

Indexes:

- `UNIQUE(target)` — same canonical target must not appear twice. Combined with § 2.5 canonicalization, this prevents `1.2.3.4` and `1.2.3.4/32` from being two rows.
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
| `firewall.allow_list` | string[] | `[]` | CIDRs the server refuses to enqueue into any block_list (server-side guardrail, § 4.2) |

Env var: `SERVERBEE_FIREWALL__ALLOW_LIST` (comma-separated).

### 2.4 Migrations

- `m20260521_000027_create_block_list` — create table + indexes.
- `m20260521_000028_extend_alert_rule_actions` — add nullable `actions_json` column.
- `m20260521_000029_extend_capability_mask` — bump `CAP_VALID_MASK` reference (data unchanged; this is mostly a comment/marker migration since the column is `INTEGER NOT NULL DEFAULT`).

Only `up()` is implemented; `down()` returns `Ok(())` (matches existing convention).

### 2.5 Target canonicalization

Client input goes through `FirewallService::canonicalize_target(input) -> (target, family)` before any insert / dedup / guardrail check:

1. Parse `input` first as `IpAddr`. On success, convert to `IpNet` via the host-bit prefix (`/32` for v4, `/128` for v6).
2. Otherwise parse as `IpNet`. Reject anything else.
3. Re-emit using `IpNet::network()` + prefix length — this collapses `1.2.3.4/24` → `1.2.3.0/24`, `001:0db8::/32` → `1:db8::/32`, and `1.2.3.4` → `1.2.3.4/32`.
4. `family` is derived from the parsed variant; clients **must not** supply it.

Both the REST handler and the auto-block path call this. The `target` column always stores the canonical form, so `UNIQUE(target)` is meaningful. Display in the UI strips the trailing `/32` / `/128` for single hosts.

### 2.6 `RecoveryMergeService` integration

When `recovery_merge` rewrites server IDs (e.g. after a recovery merge merges agent `srv-A → srv-B`), it must rewrite `block_list.server_ids_json` alongside the existing tables. The plan adds `block_list` to `recovery_merge::rewrite_server_id`'s table list so coverage scopes follow the merge.

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
        results: Vec<BlocklistAckItem>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlocklistAckItem {
    pub id: String,
    pub applied: bool,
    pub reason: Option<String>, // populated when applied = false
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockEntry {
    pub id: String,
    pub target: String, // canonical IpNet string
    pub family: u8,     // 4 or 6
}
```

`BlockEntry` does **not** carry `cover_type` / `server_ids_json` — the server already filters before sending, so the agent applies whatever it receives.

`BlocklistAck` carries **both successes and failures** so the server has authoritative per-agent apply state. For incremental ops (`BlocklistAdd` / `BlocklistRemove`) the ack contains one item. For `BlocklistSync` the ack batches all entries the agent attempted to apply (one item per entry, both old and new), so a single Sync results in exactly one Ack message with N items. This keeps WS volume bounded (1 ack per reconcile) while letting the server stop inferring "not applied" from audit logs.

---

## 4. Guardrails (overlapping)

Misconfigured rules can lock the operator out. Two server-side tiers cover the full protected set; the agent re-checks an overlapping subset as a last line of defense. Any tier tripping aborts the insert/apply.

The agent does **not** see the full server-side allow_list, trusted_proxies, or other agents' external IPs — sending that data over WS is more attack surface than it's worth. Agent tier knowledge is intentionally a subset; the server's authoritative check still runs first.

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

### 4.3 Tier 3 — agent-side (subset)

Before issuing `nft add element` the agent re-runs **tier 1 (hard-coded) + its own external IP**. This is intentionally a subset of the server-side check: the agent does not see `firewall.allow_list`, `server.trusted_proxies`, or other agents' external IPs. The server-side tiers run first, so the agent tier is a backstop against bugs / version skew, not the primary gatekeeper. On hit, the agent emits `BlocklistAck { applied: false, reason: "guardrail: …" }` for that entry.

### 4.4 Audit on rejection

| Action | When |
|---|---|
| `firewall_block_created` | Row inserted (manual or auto) |
| `firewall_block_deleted` | Row deleted via REST |
| `firewall_block_applied_agent` | `BlocklistAckItem { applied: true }` received |
| `firewall_block_rejected_server` | Tier 1 / Tier 2 reject |
| `firewall_block_rejected_agent` | Tier 3 reject (`BlocklistAckItem { applied: false }`) |
| `firewall_auto_block_skipped_conflict` | Auto-block dedup found a row that does **not** cover the current server (§ 5.2 step 2) |
| `firewall_auto_rule_removed` | Alert rule with action was deleted; existing `block_list` rows are kept |

Detail JSON includes `target`, `reason` (when applicable), `server_id` (for per-agent events), and `origin_rule_id` / `origin_event_id` for auto-block-derived rows.

### 4.5 UI feedback

`POST /api/firewall/blocks` returns `409 Conflict` with the reason; form surfaces a localized error.

---

## 5. Auto-block path

### 5.1 Hook into SecurityService

`SecurityService` currently holds `db`, `browser_tx`, `alert_state_manager`, `config`. The plan extends it to also hold `Arc<AgentManager>` and an `Arc<FirewallService>` (or equivalent). These are constructed once in `AppState::new` and injected — no need for `state` to be passed into the service.

Inside `evaluate_rules`, the existing per-rule loop currently does:

```
... match against item, params, evidence ...
mark_triggered(...)
if !should_notify { continue; }                // dedupe window
let Some(group_id) = rule.notification_group_id else { continue; };
send_group(...)
```

Auto-block runs **after `mark_triggered` and before the `should_notify` / `notification_group_id` early-exits** — actions must fire on every rule match, regardless of whether a notification is being suppressed by dedupe or whether a notification group is configured. Concretely:

```rust
mark_triggered(...).await?;

// NEW: actions run on every match, independent of notification state.
for action in deserialize_actions(&rule.actions_json) {
    match action {
        AlertRuleAction::BlockSourceIp { cover_type, server_ids_json, comment } => {
            if let Err(e) = self.firewall.auto_block(
                rule, payload, &cover_type, server_ids_json.as_deref(), comment.as_deref(),
            ).await {
                tracing::error!(rule_id=%rule.id, error=%e, "auto_block failed");
            }
        }
    }
}

if !should_notify { continue; }
let Some(ref group_id) = rule.notification_group_id else { continue; };
NotificationService::send_group(...).await
```

### 5.2 `FirewallService::auto_block` steps

1. **Canonicalize** `payload.source_ip` via § 2.5 → `(target, family)`.
2. **Dedup-with-scope check**: `SELECT id, cover_type, server_ids_json FROM block_list WHERE target = $1`.
   - If no row → continue to step 3.
   - If row exists AND `rule_covers_server(existing.cover_type, existing.server_ids_json, payload.server_id)` is true → genuinely redundant; skip silently.
   - If row exists but does **not** cover the current server, then this attacker is hitting a server not covered by the existing row. We must not pretend the IP is blocked. Write audit `firewall_auto_block_skipped_conflict` with detail `{ target, existing_id, current_server_id }` and skip. The operator can broaden the existing row's coverage manually. (Rationale: silently widening coverage from `include[srv-A]` to `all` would surprise the operator who narrowed it on purpose.)
3. **Guardrail** (tier 1 + 2). Reject → audit `firewall_block_rejected_server`, return `Ok(())`. The caller's notification path continues unaffected.
4. **Insert** `block_list` row with canonical `target`, derived `family`, `origin = "auto"`, `origin_event_id`, `origin_rule_id`, rendered `comment` (placeholders `{rule_name}`, `{event_type}`, `{severity}`).
5. **Audit** `firewall_block_created` with detail `{ id, target, origin, rule_id, event_id }`.
6. **Push to covered online agents**: iterate `agent_manager.online_agents()`, filter by `rule_covers_server(row.cover_type, row.server_ids_json, &agent_id)` AND by `has_capability(caps, CAP_FIREWALL_BLOCK)`, send `BlocklistAdd`. Offline agents pick it up on next `BlocklistSync`.

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

**On capability bit transition** (`CapabilitiesSync` changes the effective capability mask for this connection):
- bit went **on** → push a fresh `BlocklistSync` so the agent installs the current set.
- bit went **off** → push `ServerMessage::BlocklistSync { entries: [] }`. Agent treats an empty Sync as "drop everything", which combined with § 6.3 below means flushing the nft sets and emitting one batched ack. This mirrors the Docker capability-revoke cleanup pattern in `ws::agent` — the connection stays open, but the feature data is wiped.

**On `BlocklistAck`**: for each item, write either `firewall_block_applied_agent` (`applied=true`) or `firewall_block_rejected_agent` (`applied=false`) audit, log accordingly. Do **not** delete the `block_list` row on a single-agent rejection — another agent may apply it fine (different external IP). Server keeps an `agent_apply_state` map (`(block_id, server_id) → {applied, reason, at}`) in memory only, derived from acks since boot, that the REST `GET /api/firewall/blocks/:id` enriches into per-agent status. Historical state is reconstructible from the audit log.

### 6.2 Agent side `FirewallManager`

```rust
pub struct FirewallManager {
    /// Entries successfully applied to nft. Only mutated after a successful apply.
    desired: HashMap<String, BlockEntry>,
    /// One-shot resource bootstrap state — see § 6.3.
    nft_ready: bool,
    external_ip: OnceLock<IpAddr>,
}
```

Message handling:

```rust
match msg {
    BlocklistSync { entries } => {
        if entries.is_empty() && self.desired.is_empty() && !self.nft_ready {
            // Common case before any block ever existed; nothing to do.
            ack_batch(vec![]);
            return;
        }
        self.ensure_nft_resources()?;

        let incoming: HashMap<_, _> =
            entries.into_iter().map(|e| (e.id.clone(), e)).collect();
        let to_add:    Vec<&BlockEntry> = incoming.values()
            .filter(|e| !self.desired.contains_key(&e.id))
            .collect();
        let to_remove: Vec<BlockEntry>  = self.desired.values()
            .filter(|e| !incoming.contains_key(&e.id))
            .cloned().collect();

        let mut results = Vec::with_capacity(to_add.len() + to_remove.len());
        for e in to_add {
            match self.apply_add(e).await {
                Ok(()) => {
                    self.desired.insert(e.id.clone(), e.clone());
                    results.push(BlocklistAckItem { id: e.id.clone(), applied: true, reason: None });
                }
                Err(reason) => {
                    // NOT inserted into desired — next Sync will retry.
                    results.push(BlocklistAckItem { id: e.id.clone(), applied: false, reason: Some(reason) });
                }
            }
        }
        for e in to_remove {
            // Best-effort: a "delete-missing" is treated as success (§ 6.3).
            let _ = self.apply_remove(&e).await;
            self.desired.remove(&e.id);
            results.push(BlocklistAckItem { id: e.id, applied: true, reason: None });
        }

        // Special case: empty incoming on capability-revoke also flushes the
        // sets entirely so leftover rules from the previous capability=on
        // window cannot keep dropping traffic.
        if incoming.is_empty() {
            self.flush_sets().await;     // `nft flush set ...` for both v4 / v6
            self.desired.clear();
        }

        ack_batch(results);
    }

    BlocklistAdd { entry } => {
        self.ensure_nft_resources()?;
        if let Err(r) = self.tier3_guardrail(&entry.target) {
            ack_single(entry.id, false, Some(r));
            return;
        }
        match self.apply_add(&entry).await {
            Ok(()) => {
                self.desired.insert(entry.id.clone(), entry.clone());
                ack_single(entry.id, true, None);
            }
            Err(reason) => {
                // NOT inserted; next Sync re-attempts.
                ack_single(entry.id, false, Some(reason));
            }
        }
    }

    BlocklistRemove { id } => {
        let entry = self.desired.remove(&id);
        match entry {
            Some(e) => {
                let _ = self.apply_remove(&e).await; // delete-missing = success
                ack_single(id, true, None);
            }
            None => ack_single(id, true, None), // unknown id = already absent
        }
    }
}
```

Key invariant: **`desired` only contains entries the agent has confirmed in the kernel nft set.** Application failure leaves the entry out of `desired`, so the next `BlocklistSync` diff retries it.

### 6.3 `nft` invocation

**Resource bootstrap (`ensure_nft_resources`)** — fully idempotent. Run on first need, then short-circuited by `nft_ready = true`. Each resource is detected before mutating:

```text
exists table inet serverbee?      no → nft add table inet serverbee
exists set block_v4?              no → nft add set ... block_v4 ipv4_addr/interval
exists set block_v6?              no → nft add set ... block_v6 ipv6_addr/interval
exists chain input?               no → nft add chain ... input filter hook input priority -10
chain has the v4 drop rule?       no → nft add rule  ... ip  saddr @block_v4 drop
chain has the v6 drop rule?       no → nft add rule  ... ip6 saddr @block_v6 drop
```

Detection uses `nft -j list ruleset` once and inspects the JSON. Any subsequent `nft add` that races with another process and returns `EEXIST` is treated as success (the kernel state is what we wanted).

**Per-entry add / remove** — same EEXIST / ENOENT lenience:

```bash
nft add element    inet serverbee block_v4 '{ 1.2.3.4 }'   # already exists → ok
nft delete element inet serverbee block_v4 '{ 1.2.3.4 }'   # not present   → ok
```

Mapping:

| `nft` stderr signal | Mapped to |
|---|---|
| `Error: File exists` on `add element` | success |
| `Error: No such file or directory` on `delete element` | success |
| `Error: Could not process rule: Operation not permitted` | failure (likely missing root) — `CapabilityDenied` |
| `Error: Could not process rule: No such file or directory` on resource ops | failure (kernel module missing) — `CapabilityDenied` |
| Any other non-zero exit | failure, `reason` = first stderr line |

**Set flush** (used on capability-revoke / empty-Sync case):

```bash
nft flush set inet serverbee block_v4
nft flush set inet serverbee block_v6
```

All commands run via `tokio::process::Command`. Each invocation captures stderr; non-success cases become `BlocklistAck { applied: false, reason }` for the offending entry (or a `CapabilityDenied` ServerMessage when the entire pipeline can't function).

### 6.4 Failure matrix

| Scenario | Behavior |
|---|---|
| Agent offline at CRUD time | No push; next `BlocklistSync` carries the delta |
| Agent ack `applied = false` | Server audits, in-memory `agent_apply_state` records the failure; entry stays out of agent `desired` so next Sync diff re-tries it |
| Agent ack `applied = true` | Server audits `firewall_block_applied_agent`; `agent_apply_state` updated |
| `nft` missing or unprivileged | Agent emits `CapabilityDenied`; server UI shows "firewall capability unavailable" |
| Server restart | Agent reconnects → full Sync → state is restored |
| Agent restart | Same — agent does not persist blocklist; resource bootstrap runs again, idempotent |
| Capability bit toggled off mid-session | Server pushes empty Sync → agent flushes both sets, clears `desired` |
| Duplicate target | `UNIQUE(target)` rejects at insert; auto-block dedup checks coverage scope before skipping (§ 5.2 step 2) |
| Mid-Sync WS drop | Sync abandoned; next reconnect retries from scratch with the same diff algorithm |

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

- `service::firewall::canonicalize_target` — `1.2.3.4` → `1.2.3.4/32`, `1.2.3.4/24` → `1.2.3.0/24`, IPv6 case-fold, garbage rejected
- `service::firewall::is_protected` — IPv4 / IPv6 overlap edge cases (target ⊃ protected, target ⊂ protected, disjoint)
- `service::firewall::FirewallService::auto_block` — coverage-aware dedup (existing row covers / does not cover scope), guardrail rejection, normal path (with in-memory DB + mock agent manager)
- `agent::firewall::nft` — mock `tokio::process::Command` (extract a `NftExecutor` trait): verify generated command strings, EEXIST / ENOENT lenience, error mapping
- `agent::firewall::reconcile` — diff algorithm: empty → full, full → empty, partial overlap, idempotent re-sync, **failed-apply entry stays out of `desired` and retries on next Sync**
- `agent::firewall::FirewallManager` — capability-off Sync (empty entries) triggers `nft flush set` and clears `desired`
- `service::alert::validate_alert_rule` — action + rule_type compatibility, max 1 action, forbidden combinations (`ssh_new_ip_login` + block)
- `service::recovery_merge` — `block_list.server_ids_json` is rewritten alongside other server_id-bearing tables

### 11.2 Integration (`crates/server/tests/integration.rs`)

- `POST /api/firewall/blocks` → row inserted, mock agent receives `BlocklistAdd`, ack (applied=true) written, audit `firewall_block_applied_agent`
- `POST` with non-canonical target (`1.2.3.4/24`) → row stored as `1.2.3.0/24`; second POST with `1.2.3.0/24` → 409 dup
- Guardrail rejection paths return 409 with localized reason key
- Auto-block end-to-end: insert security_event → matching rule with action → block_list row appears
- Auto-block dedup with non-covering existing row → audit `firewall_auto_block_skipped_conflict`, no insert
- Agent reconnect → `BlocklistSync` carries expected entries; failed-apply entries retry on subsequent Sync
- Capability bit turned off mid-session → empty `BlocklistSync` pushed → agent ack stream shows all entries removed; agent `desired` cleared (verified via test hook)
- DELETE → row gone, mock agent receives `BlocklistRemove`, ack
- Member (non-admin) on `POST` / `DELETE` → 403
- Recovery merge that rewrites `srv-A → srv-B` updates `block_list.server_ids_json` accordingly

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

- Audit log retention: `firewall_*` events follow `retention.audit_logs_days` (180d). Separate knob can be added later if needed.
- Emergency "panic unblock" endpoint: not in v1. `DELETE` per entry is sufficient; `nft flush set inet serverbee block_v4` is a one-liner if an operator needs to bypass the API.
