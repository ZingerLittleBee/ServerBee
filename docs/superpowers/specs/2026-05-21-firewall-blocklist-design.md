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
    /// Explicit "wipe everything" — independent of capability mask, agent
    /// always honors it. Used when capability bit transitions off, when an
    /// admin wants to force a clean slate, or on server-side data corruption
    /// recovery. The agent flushes both nft sets, clears `desired`, and acks.
    BlocklistReset,
}

pub enum AgentMessage {
    // existing variants...
    BlocklistAck {
        results: Vec<BlocklistAckItem>,
    },
    /// Sent in response to BlocklistReset. Confirms the wipe happened (or
    /// reports why it could not).
    BlocklistResetAck { ok: bool, reason: Option<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlocklistAckItem {
    pub id: String,
    /// Authoritative agent-observed state for this entry after the op.
    pub state: BlocklistEntryState,
    /// Populated only when state = Failed.
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlocklistEntryState {
    /// The target is currently in the nft set on this agent.
    Present,
    /// The target is absent from the nft set on this agent (either never
    /// applied, or just removed).
    Absent,
    /// The agent tried to act on this entry and failed. `reason` describes
    /// the failure. Server keeps the entry in `block_list` and retries on
    /// the next Sync.
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BlockEntry {
    pub id: String,
    pub target: String, // canonical IpNet string
    pub family: u8,     // 4 or 6
}
```

`BlockEntry` does **not** carry `cover_type` / `server_ids_json` — the server already filters before sending, so the agent applies whatever it receives.

`BlocklistAck` carries **per-entry final state** so the server has authoritative per-agent apply state. The `state` enum distinguishes `present` (added) from `absent` (removed) — both are "successful" but mean different things in the apply-state map and the audit log. `failed` keeps server's view consistent with reality and triggers retry on the next Sync.

For `BlocklistAdd` the ack contains one `Present` (or `Failed`) item. For `BlocklistRemove` the ack contains one `Absent` (or `Failed`) item. For `BlocklistSync` the ack contains one item **per incoming entry** plus one item per just-removed entry — see § 6.2 for why ack-on-unchanged matters for apply-state reconstruction after a server restart.

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

Before issuing `nft add element` the agent re-runs **tier 1 (hard-coded) + its own external IP**. This is intentionally a subset of the server-side check: the agent does not see `firewall.allow_list`, `server.trusted_proxies`, or other agents' external IPs. The server-side tiers run first, so the agent tier is a backstop against bugs / version skew, not the primary gatekeeper. On hit, the agent emits `BlocklistAckItem { state: Failed, reason: "guardrail: …" }` for that entry.

### 4.4 Audit on rejection

| Action | When |
|---|---|
| `firewall_block_created` | Row inserted (manual or auto) |
| `firewall_block_deleted` | Row deleted via REST |
| `firewall_block_applied_agent` | Ack `state=Present` for an entry the agent was asked to add |
| `firewall_block_removed_agent` | Ack `state=Absent` for an entry the agent was asked to remove |
| `firewall_block_rejected_server` | Tier 1 / Tier 2 reject (REST or auto-block) |
| `firewall_block_rejected_agent` | Ack `state=Failed` (agent guardrail or nft error) |
| `firewall_auto_block_skipped_conflict` | Auto-block dedup found a row that does **not** cover the current server (§ 5.2 step 2) |
| `firewall_auto_rule_removed` | Alert rule with action was deleted; existing `block_list` rows are kept |
| `firewall_reset_acked` | `BlocklistResetAck { ok: true }` received |

Detail JSON includes `target`, `reason` (when applicable), `server_id` (for per-agent events), and `origin_rule_id` / `origin_event_id` for auto-block-derived rows.

To disambiguate add vs remove acks the server tracks "what was the last op sent to this agent for this id" in the same in-memory `agent_apply_state` map (§ 6.1) and emits the right audit. An ack for an id the server never sent (e.g. stale agent state during a protocol upgrade) is dropped with a debug log.

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
- bit went **off** → push `ServerMessage::BlocklistReset`. The agent honors `BlocklistReset` regardless of its local capability state (the bit may have flipped off mid-flight, and we still want the existing kernel rules wiped).

**On agent first-ever connect** (no `agent_apply_state` yet) — push `BlocklistReset` followed by `BlocklistSync`. The reset clears any leftover kernel state from a previous agent install / packaging quirk; the Sync re-installs the canonical set. After this, the in-memory apply state reflects ground truth.

**On `BlocklistAck`**: for each item, look up the prior op the server sent for `(id, server_id)`:
- `state=Present` and last op was `Add` (or `Sync` that included it) → audit `firewall_block_applied_agent`, set `agent_apply_state[(id, server_id)] = Present`.
- `state=Absent` and last op was `Remove` (or `Sync` that excluded it) → audit `firewall_block_removed_agent`, set `agent_apply_state[(id, server_id)] = Absent`.
- `state=Failed` → audit `firewall_block_rejected_agent`, set apply-state to `Failed { reason }`. Do **not** delete the `block_list` row; next Sync diff re-attempts (another agent may apply it fine, and the failing agent may succeed after operator action).
- Other combinations (e.g. `Present` for a Remove op) → log warn, drop.

**`agent_apply_state`** is `Arc<RwLock<HashMap<(block_id, server_id), ApplyState>>>` in `AppState`, in-memory only. Populated from acks since boot. The REST `GET /api/firewall/blocks/:id` joins this map for per-agent display. Historical state is reconstructible from the audit log if needed.

**On `BlocklistResetAck { ok: true }`**: audit `firewall_reset_acked`; drop all `agent_apply_state` entries with `server_id == this server`. `ok: false` is logged at warn and the apply state is left untouched (the kernel may still contain entries).

### 6.2 Agent side `FirewallManager`

```rust
pub struct FirewallManager {
    /// Entries the agent has confirmed are in the kernel nft set. Mutated
    /// only after a successful apply_add / apply_remove.
    desired: HashMap<String, BlockEntry>,
    /// Resource-bootstrap state — see § 6.3. Reset to false on Reset, so
    /// the next Sync re-detects kernel resources.
    nft_ready: bool,
    /// Agent's own external IP, used by tier-3 guardrail.
    external_ip: OnceLock<IpAddr>,
}
```

**Cardinal rules**:
1. An entry is in `desired` **only if** the agent has just observed a successful kernel mutation for it. Treat `desired` as a cache of confirmed kernel state, not as authoritative truth about the kernel.
2. `BlocklistReset` and the cleanup paths must **never** be gated by capability bit or `desired` being empty. They are unconditional kernel wipes.
3. Every entry of every incoming `BlocklistSync` produces exactly one ack item. Including unchanged entries — that is how the server rebuilds `agent_apply_state` after a server restart.

Message handling:

```rust
match msg {
    BlocklistReset => {
        // Always honored. Never gated by capability or desired.
        match self.unconditional_wipe().await {
            Ok(()) => {
                self.desired.clear();
                self.nft_ready = false; // force re-detect on next Sync
                send(BlocklistResetAck { ok: true, reason: None });
            }
            Err(reason) => {
                send(BlocklistResetAck { ok: false, reason: Some(reason) });
            }
        }
    }

    BlocklistSync { entries } => {
        self.ensure_nft_resources()?;
        let incoming: HashMap<String, BlockEntry> =
            entries.into_iter().map(|e| (e.id.clone(), e)).collect();

        let to_remove: Vec<BlockEntry> = self.desired.values()
            .filter(|e| !incoming.contains_key(&e.id))
            .cloned().collect();

        let mut results = Vec::with_capacity(incoming.len() + to_remove.len());

        // For every incoming entry: re-affirm via idempotent add. If it was
        // already present (EEXIST) the agent reports Present without an
        // actual kernel mutation. This makes Sync the source of truth
        // re-establishment after server restart even when no diff exists.
        for e in incoming.values() {
            if let Err(r) = self.tier3_guardrail(&e.target) {
                results.push(item(&e.id, Failed, Some(r)));
                self.desired.remove(&e.id); // ensure not claimed as confirmed
                continue;
            }
            match self.apply_add(e).await {  // EEXIST → Ok(())
                Ok(()) => {
                    self.desired.insert(e.id.clone(), e.clone());
                    results.push(item(&e.id, Present, None));
                }
                Err(reason) => {
                    self.desired.remove(&e.id);
                    results.push(item(&e.id, Failed, Some(reason)));
                }
            }
        }

        for e in &to_remove {
            match self.apply_remove(e).await {  // ENOENT → Ok(()); other errors propagate
                Ok(()) => {
                    self.desired.remove(&e.id);
                    results.push(item(&e.id, Absent, None));
                }
                Err(reason) => {
                    // Kernel still has it — do NOT clear desired or claim Absent.
                    results.push(item(&e.id, Failed, Some(reason)));
                }
            }
        }

        send(BlocklistAck { results });
    }

    BlocklistAdd { entry } => {
        self.ensure_nft_resources()?;
        if let Err(r) = self.tier3_guardrail(&entry.target) {
            send(BlocklistAck { results: vec![item(&entry.id, Failed, Some(r))] });
            return;
        }
        match self.apply_add(&entry).await {  // EEXIST → Ok(())
            Ok(()) => {
                self.desired.insert(entry.id.clone(), entry.clone());
                send(BlocklistAck { results: vec![item(&entry.id, Present, None)] });
            }
            Err(reason) => {
                send(BlocklistAck { results: vec![item(&entry.id, Failed, Some(reason))] });
            }
        }
    }

    BlocklistRemove { id } => {
        // Use the entry from `desired` if known (for v4/v6 routing). If
        // unknown, ack Absent — kernel by construction doesn't have it
        // unless server / agent state diverged, in which case the next
        // Sync will reconcile.
        let entry = self.desired.get(&id).cloned();
        let Some(entry) = entry else {
            send(BlocklistAck { results: vec![item(&id, Absent, None)] });
            return;
        };
        match self.apply_remove(&entry).await {  // ENOENT → Ok(())
            Ok(()) => {
                self.desired.remove(&id);
                send(BlocklistAck { results: vec![item(&id, Absent, None)] });
            }
            Err(reason) => {
                // Keep desired — kernel may still contain it.
                send(BlocklistAck { results: vec![item(&id, Failed, Some(reason))] });
            }
        }
    }
}
```

`unconditional_wipe` runs:
```text
nft flush set inet serverbee block_v4   # ENOENT → ok
nft flush set inet serverbee block_v6   # ENOENT → ok
nft delete table inet serverbee         # ENOENT → ok; full teardown
```
Errors that are not ENOENT propagate as the reason in `BlocklistResetAck { ok: false }`.

Key invariant: **`desired` only contains entries the agent has confirmed in the kernel nft set.** Application or removal failure leaves the entry in the appropriate state, so the next `BlocklistSync` diff retries it.

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
| `Error: File exists` on `add element` | success (idempotent re-add) |
| `Error: No such file or directory` on `delete element` / `flush set` / `delete table` | success (already absent) |
| `Error: Could not process rule: Operation not permitted` | failure with `reason: "permission denied — agent needs root or CAP_NET_ADMIN"` |
| `Error: Could not process rule: No such file or directory` on resource ops (`add table` etc.) | failure with `reason: "nft kernel module unavailable"` |
| Any other non-zero exit | failure, `reason` = first stderr line |

All commands run via `tokio::process::Command`. Failures surface as `BlocklistAckItem { state: Failed, reason }` per offending entry. They do **not** trigger `CapabilityDenied`: that message is reserved for "operation not allowed by capability mask", not "kernel says no". Agent advertises the firewall capability only when local probes succeed; runtime kernel errors are reported entry-by-entry. See § 6.5 below for capability advertisement.

### 6.5 Capability advertisement

`CAP_FIREWALL_BLOCK` operates on **two** capability layers, mirroring how `CAP_DOCKER` is currently handled:

1. **Effective capability** (server-controlled) — the existing `capabilities` u32 on the `server` row. Admin sets this in the UI.
2. **Local capability** (agent-controlled) — whether the agent's host can actually execute the feature. Reported in the existing `SystemInfo.capabilities_local` field at connect time. For firewall: requires `nft` binary present AND a self-test write to a throwaway probe set succeeds (proves root/CAP_NET_ADMIN). Probed once at agent startup, cached.

The effective capability used by `has_capability` checks is the **bitwise AND** of the two. Server only sends `Blocklist*` messages when both bits are set. If local cap is missing, the UI shows the firewall toggle as "unavailable on this host" with the probe error.

The agent never tries to "best-effort" firewall ops it knows it cannot perform — it advertises `false` locally and the server omits the messages entirely.

### 6.4 Failure matrix

| Scenario | Behavior |
|---|---|
| Agent offline at CRUD time | No push; next `BlocklistSync` carries the delta and rebuilds full apply state |
| Ack `state=Failed` for an add | Server audits `firewall_block_rejected_agent`, in-memory `agent_apply_state` records Failed; entry stays out of agent `desired` so next Sync diff re-tries it |
| Ack `state=Failed` for a remove | Server audits `firewall_block_rejected_agent`; agent keeps the entry in `desired` so next Sync re-attempts removal. Importantly, server does **not** mark the block as cleared anywhere |
| Ack `state=Present` | Audit `firewall_block_applied_agent`; apply_state updated |
| Ack `state=Absent` after a remove op | Audit `firewall_block_removed_agent`; apply_state updated |
| Local cap probe fails (no nft / no root) | Agent advertises `local capability = false`; server omits firewall messages entirely. UI shows "firewall unavailable on this host" |
| Server restart | Agent reconnects → first contact triggers `Reset + Sync`; ack-on-unchanged re-populates apply_state |
| Agent restart | Same — agent does not persist blocklist; resource bootstrap runs again, idempotent |
| Effective cap toggled off mid-session | Server pushes `BlocklistReset`; agent flushes sets, drops table, clears `desired`. The reset is unconditional (not gated by current local/effective cap state) |
| Effective cap toggled on mid-session | Server pushes `BlocklistSync` with the current set |
| Duplicate target | `UNIQUE(target)` rejects at insert; auto-block dedup checks coverage scope before skipping (§ 5.2 step 2) |
| Mid-Sync WS drop | Sync abandoned; next reconnect retries the full Sync algorithm |
| Old agent (no firewall support) | Server `protocol_version` gate: only agents reporting `protocol_version >= FIREWALL_MIN_PROTOCOL` and local cap = true receive `Blocklist*` (§ 12) |

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
pub enum BrowserMessage {
    // existing variants...
    /// Row created or deleted (REST or auto-block path).
    BlocklistChanged {
        kind: BlocklistChangeKind,   // "created" | "deleted"
        entry: BlockListItem,
    },
    /// Per-agent apply state changed (ack arrived). Driven by acks; the
    /// UI uses this to refresh the per-agent dots in the detail drawer
    /// and the activity log without polling.
    FirewallApplyStateChanged {
        block_id: String,
        server_id: String,
        state: BlocklistEntryState,   // present | absent | failed
        reason: Option<String>,
    },
}
```

Frontend `apps/web/src/hooks/use-servers-ws.ts` reacts:

- `BlocklistChanged` → invalidate `['firewall', 'blocks']` (debounced 1s) + `['firewall', 'stats']`.
- `FirewallApplyStateChanged` → invalidate `['firewall', 'block', block_id]` (the detail query) + `['firewall', 'activity']` (debounced 500ms; bursts are common during Sync).

Bandwidth-wise: each ack item produces one BrowserMessage. A 100-entry Sync from a freshly reconnected agent produces 100 messages — bounded and rare (only at reconnect / capability transition). Steady-state CRUD produces one per op.

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

- `POST /api/firewall/blocks` → row inserted, mock agent receives `BlocklistAdd`, ack `state=Present` written, audit `firewall_block_applied_agent`
- `POST` with non-canonical target (`1.2.3.4/24`) → row stored as `1.2.3.0/24`; second POST with `1.2.3.0/24` → 409 dup
- Guardrail rejection paths return 409 with localized reason key
- Auto-block end-to-end: insert security_event → matching rule with action → block_list row appears, `BlocklistAdd` pushed, apply state recorded from ack
- Auto-block dedup with non-covering existing row → audit `firewall_auto_block_skipped_conflict`, no insert
- Agent reconnect (server has prior in-memory apply_state cleared) → Sync includes every covered entry, agent acks `Present` for unchanged, apply_state fully rebuilt from acks
- Failed-apply ack — agent simulates nft permission error → server records Failed, next Sync includes the entry, ack `Present` second time succeeds
- Effective cap turned off mid-session → `BlocklistReset` pushed → agent acks `ok=true` → server clears apply_state for that server; emits `FirewallApplyStateChanged` per affected block
- Effective cap on while local cap is false → no firewall messages emitted
- DELETE → row gone, mock agent receives `BlocklistRemove`, ack `state=Absent`, audit `firewall_block_removed_agent`
- Failed-remove ack — agent simulates nft permission error on delete → server records Failed, **block_list row stays**, retries on next Sync
- Member (non-admin) on `POST` / `DELETE` → 403
- Recovery merge that rewrites `srv-A → srv-B` updates `block_list.server_ids_json` accordingly
- Old agent (protocol_version < FIREWALL_MIN_PROTOCOL) → no firewall messages sent regardless of effective cap; UI surfaces version mismatch hint

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
- **Protocol version gate**: bump `PROTOCOL_VERSION` (constant in `crates/common/src/protocol.rs`) to a new value `FIREWALL_MIN_PROTOCOL`. The server gates `Blocklist*` and `BlocklistReset` emission on `effective_capability.firewall && agent.protocol_version >= FIREWALL_MIN_PROTOCOL`. This is the only barrier between new servers and old agents; we do **not** rely on `#[serde(other)]` for forward compatibility on the agent side, because today's agent parser errors out on unknown variants rather than silently dropping them. Adding a serde shim now would be a separate change and is not required for v1 if we honor the version gate.
- Agent without local cap (`nft` missing / not root): the agent self-reports `local capability = false` via `SystemInfo.capabilities_local`. Server omits all firewall messages. The agent never receives a message it cannot serve.
- `BlocklistReset` is gated only by **effective capability transition**, not by current local cap. If a host had local cap=true at some prior point and the kernel may contain stale `serverbee` rules, the server still needs a way to clean up. Concretely:
  - When **effective cap** flips off (admin disabled it), server pushes `Reset` once. Agent honors it if local cap is currently true; if local cap is currently false, agent acks `ok=false reason="nft unavailable"` and the operator must clean up manually (or restore local cap).
  - The reset is **not** an emergency back-door that bypasses local capability — if `nft` is missing, the cleanup is moot anyway.

---

## 13. Open questions deferred to plan stage

- Audit log retention: `firewall_*` events follow `retention.audit_logs_days` (180d). Separate knob can be added later if needed.
- Emergency "panic unblock" endpoint: not in v1. `DELETE` per entry is sufficient; `nft flush set inet serverbee block_v4` is a one-liner if an operator needs to bypass the API.
