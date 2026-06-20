# Temporary Capability Grants Design

**Status:** Draft
**Date:** 2026-06-20
**Branch:** `feat/temporary-capability-grants`
**Depends on:** Agent-owned capabilities (commits `3c66c16f`, `eadd7df6` on `feat/agent-owned-capabilities`)

## Goal

Let a host operator **temporarily enable** an agent capability that is otherwise off, for a bounded duration, after which it auto-disables. The grant is created on the agent host (the trust boundary is the host's shell/root), survives an agent restart, and expires at its original wall-clock deadline. While a temporary grant is active the server's control-plane gates open for it, the web UI shows it as a distinct "temporary" state with a live countdown, every grant/expiry is audited, and temporarily enabling a high-risk capability can fire an alert.

This extends — and deliberately does not weaken — the agent-owned capability model: the **server can never modify an agent's capabilities**. The permanent capability state lives in `agent.toml` `[capabilities]`; the temporary state lives in `capability_grants.json`. Both files are writable only on the agent host by root. The server remains a read-only mirror of whatever the agent reports.

## Non-goals (v1)

- **Server- or web-initiated grants.** The trigger is host-local only. There is no new `ServerMessage` and no way for the server to request, push, or extend a grant. (Considered and rejected to keep the trust model pure.)
- **A `temporary_grantable` allowlist.** Anyone who can run the grant CLI already has root-equivalent power on the host (they could edit `agent.toml` permanently). An allowlist would be security theater. Any capability that is currently *off* can be temporarily granted.
- **Temporary *removal* of a capability.** Grants can only turn *on* a bit that is off in the base set; they cannot strip a base capability.
- **Remote / IPC control surface.** No unix socket, no local HTTP, no signal protocol. The grant CLI and the running daemon communicate only through the grants file.
- **Sub-second pickup latency.** A few seconds of polling latency is acceptable for "open a window to use a capability".

---

## 1. Architecture

```
┌───────────────────────────────  AGENT HOST (root)  ───────────────────────────────┐
│                                                                                    │
│  $ serverbee-agent grant terminal --for 30m --reason "debug"                       │
│        │ one-shot process: load Figment config, validate, atomic write, exit       │
│        ▼                                                                            │
│  capability_grants.json   ◄── single source of truth (absolute expires_at) ──┐     │
│        ▲                                                                      │     │
│        │ read / prune / self-heal                                            │     │
│  ┌─────┴───────────────┐                                                      │     │
│  │  grant supervisor    │  (long-running daemon, tokio task, ~3s tick)        │     │
│  │  effective =         │                                                      │     │
│  │   base | active_bits │── .store() ──►  Arc<AtomicU32>  (shared by all)      │     │
│  └─────┬───────────────┘                  ▲   terminal / exec / file / docker  │     │
│        │ on change                        └── managers read .load() at gate    │     │
│        ▼                                                                        │     │
│  AgentMessage::CapabilitiesChanged { capabilities, temporary[], changes[] }     │     │
│        │ (WebSocket, agent → server only)                                      │     │
└────────┼───────────────────────────────────────────────────────────────────────────┘
         ▼
┌──────────────────────────────────  SERVER (read-only mirror)  ─────────────────────┐
│   ws::agent handler                                                                 │
│     ├─ AgentManager.update live capabilities  → control-plane gates open/close       │
│     ├─ AgentManager.store temporary[] metadata → FullSync to late-joining browsers    │
│     ├─ ServerService::update_capabilities_mirror → servers.capabilities (display)     │
│     ├─ AuditService.record(granted | expired | revoked)                               │
│     ├─ AlertEvaluator: capability_grant_detected (high-risk caps)                     │
│     └─ broadcast BrowserMessage::CapabilitiesChanged { capabilities, temporary[] }    │
└─────────────────────────────────────────────────────────────────────────────────────┘
         ▼
   React SPA: read-only capability matrix, amber "Temporary" badge + live countdown
```

Key invariant: **the grants file is the truth; the in-memory `AtomicU32` is a derived cache.** Every code path that needs the effective capability set recomputes it from `base | active_grants` — at startup, on every supervisor tick, and after a restart. Nothing relies on in-memory state to decide whether a grant is still valid.

---

## 2. Data Model

### 2.1 Grants file

Path: `<state_dir>/capability_grants.json`, default `/var/lib/serverbee/capability_grants.json`. Mode `0600` (it controls privilege elevation). `state_dir` resolves from config (see § 7); it defaults to the parent of the existing security `data_dir` so a single `/var/lib/serverbee` tree holds all agent state.

```json
{
  "version": 1,
  "grants": [
    {
      "cap": "terminal",
      "granted_at": 1750000000,
      "expires_at": 1750001800,
      "granted_by": "root",
      "reason": "debug deploy"
    }
  ]
}
```

| Field | Type | Notes |
|---|---|---|
| `version` | int | Schema version, currently `1`. Unknown/future version → treat file as empty (fail-safe). |
| `cap` | string | Capability key (`CapabilityKey` `FromStr`: `terminal`, `exec`, `file`, `docker`, …). |
| `granted_at` | int | Unix epoch seconds, for display/audit. |
| `expires_at` | int | **Absolute** Unix epoch seconds. The sole expiry authority. |
| `granted_by` | string | Local OS identity captured by the CLI (`USER` env / uid → name). Audit only. |
| `reason` | string\|null | Optional free text. Audit only. |

Constraints:

- **At most one record per `cap`.** Re-granting an already-granted cap replaces the record (extends/shortens the window). The store is keyed by cap.
- A record is **active** iff `expires_at > now`. Active records for caps already in `base` are ignored when computing effective caps (granting an on-cap is a CLI-level no-op; see § 6).

### 2.2 Persistence idiom

Reuse the `FirstSeenStore` pattern (`crates/agent/src/security/first_seen_store.rs`):

- Serialize with `serde_json`.
- Write to `<path>.tmp` (unique temp name), `fsync`, then atomic `fs::rename` over the target. `sync_all` the parent dir on unix.
- Missing file → empty store. **Corrupt / unparseable / unknown version → log a warning and treat as empty.** The fail-safe direction is "no temporary grants" (revert to `base`), never "stuck on".
- `create_dir_all` the parent on first write; copy `0600` perms.

A new module `crates/agent/src/capability_grants/store.rs` (`CapabilityGrantStore`) owns load/save/prune.

**Single-writer rule:** the **one-shot CLI handlers are the only writer** of the file; the daemon (startup load + supervisor) is **read-only**. This eliminates any write-write race between the CLI process and the daemon. Expired records linger harmlessly until the next CLI write (the record set is bounded by the number of capabilities, ≤ 11) and are ignored by every reader because `expires_at <= now`. The CLI prunes expired records whenever it rewrites the file (on `grant`/`revoke`).

---

## 3. Effective capability computation

```
base      = compute_local_capabilities(config, cli)        // existing, computed once at startup, immutable
active    = OR of cap.to_bit() for grants where expires_at > now AND (base & cap.to_bit()) == 0
effective = (base | active) & CAP_VALID_MASK
```

- `base` never changes at runtime — the agent's permanent, agent-owned set.
- A grant can only **add** an off bit. It can never remove a base bit (the `& cap.to_bit() == 0` guard plus the OR-only merge guarantee this).
- The shared `Arc<AtomicU32>` (created at `reporter.rs:138`, cloned into every manager) holds `effective`. All existing capability gates (`has_capability(caps.load(SeqCst), CAP_*)`) automatically respect grants and expiries with zero changes to the gate sites.

There is **no defensive clamp** in the supervisor. The grants file is trusted: the only entity that can write it is host root, who could equivalently edit `agent.toml`. Duration policy is enforced at write time by the CLI (§ 6), not re-validated on read.

---

## 4. Restart & clock semantics

This is the property the design exists to guarantee.

- **State is only the file + absolute `expires_at`.** Memory (`AtomicU32`, the supervisor's in-memory view) is fully reconstructible and is never the basis for an expiry decision.
- **Startup:** before the first `SystemInfo` is sent, the agent loads the grants file, prunes records with `expires_at <= now`, computes `effective`, and seeds the `AtomicU32`. The first `SystemInfo.agent_local_capabilities` therefore already reflects surviving grants, and `temporary[]` is reported with them.
- **A grant that elapsed while the agent was down** is pruned on the next load and never reapplied.
- **A grant still inside its window** resumes with the correct *remaining* wall-clock time (because expiry is an absolute timestamp, not a monotonic countdown that resets on restart). This is exactly why an in-memory timer is insufficient.
- **Clock:** wall-clock `SystemTime::now()` epoch seconds. Forward jumps → earlier expiry (safe). Backward jumps → slightly longer grant, bounded in practice by the CLI's `temporary_max_duration` at grant time. Documented, accepted.
- **Crash safety:** atomic temp+rename + fsync means a partially written file is never observed; a corrupt file fails safe to "no grants".

---

## 5. Runtime: grant supervisor task

Spawned once at agent startup, modeled on the security/pinger task pattern (`tokio::spawn` + `tokio::time::interval`). It holds:

- `Arc<AtomicU32>` — the shared effective-caps cache (same instance all managers read).
- `base: u32` — immutable.
- `CapabilityGrantStore` (file path).
- A `mpsc::Sender<AgentMessage>` (or the reporter's existing outbound channel) to push `CapabilitiesChanged`.
- `prev_active: u32` — the set of currently-active grant bits seen on the previous tick, used to diff new vs. expired.

Tick loop (default every **3 s**; optionally wake early at the nearest `expires_at`):

1. Load the grants file (re-parse only if mtime changed; the file is tiny so unconditional parse is also fine). Parse failure → treat as empty.
2. Compute the active set, ignoring records with `expires_at <= now`. The supervisor **never writes the file** (single-writer rule, § 2.2) — expired records are simply not applied.
3. Compute `active` and `effective` (§ 3).
4. If `effective != AtomicU32.load(SeqCst)`:
   - `.store(effective, SeqCst)`.
   - Diff against `prev_active`:
     - bits newly present → `CapabilityChangeEvent { action: "granted", expires_at, granted_by, reason }`
     - bits newly absent → `action: "expired"` (or `"revoked"` if the record was removed by the CLI rather than by time — see below).
   - Send one `AgentMessage::CapabilitiesChanged { capabilities: effective, temporary: active_grants, changes }`.
   - Set `prev_active = active`.

**Seeding `prev_active` at startup:** initialize it from the file's active grants *before* the first tick, and include those grants in the initial `SystemInfo` rather than as `CapabilitiesChanged`. This ensures a grant that merely *survived a restart* is **not** re-audited or re-alerted as a new "granted" event — only genuinely new grants emit `granted`.

**`expired` vs `revoked`:** a bit disappearing because `now >= expires_at` is `expired`; a bit disappearing because the record vanished from the file while still in-window (operator ran `revoke`) is `revoked`. The supervisor distinguishes them by checking whether a still-future record existed on the previous tick.

---

## 6. CLI subcommands (one-shot, host-local)

```
serverbee-agent grant  <cap> --for <DURATION> [--reason "..."]
serverbee-agent revoke <cap>
serverbee-agent grants                       # list active grants + remaining time
```

- `DURATION` grammar: `<n>s | <n>m | <n>h | <n>d` (e.g. `90s`, `30m`, `2h`, `1d`). Must parse to a positive duration.
- These are **one-shot invocations**: dispatch on `argv[1]`, load config via the **same Figment layering** the daemon uses (so `state_dir`/`temporary_max_duration` match), perform the action, print a result, and exit. They never start the daemon.

`grant`:
1. Parse `cap` (`CapabilityKey::FromStr`; unknown → error).
2. Reject if `cap` is already in `base` (permanently enabled) → message: *"`<cap>` is already enabled in agent.toml; nothing to grant."*
3. Reject if `--for` exceeds `temporary_max_duration` (default `24h`) → message naming the cap and the configured max. This is a **footgun guard, not a security boundary**.
4. Compute `expires_at = now + duration`, capture `granted_by` from `USER`/uid.
5. Read-modify-write the grants file atomically (replace any existing record for `cap`).
6. Print: granted cap, expiry local time, and a note that the running agent picks it up within a few seconds.

`revoke`: remove the record for `cap` (atomic write). Next supervisor tick drops the bit, re-reports, and the server audits `revoked`. Revoking an absent cap is a no-op with a friendly message.

`grants`: load the file, print each active grant with cap, granted_by, reason, and remaining time. Reads the file directly — works even if the daemon is not running.

**main() dispatch:** the agent currently parses args ad hoc (no clap). Add a lightweight `match argv.get(1)` at the top of `main`: `Some("grant" | "revoke" | "grants")` → run the one-shot handler and `return`; otherwise fall through to the existing daemon path. Stays dependency-light and consistent with the codebase.

**Operational note (documented):** the CLI must run with the same environment/config the daemon uses — normally as root reading `/etc/serverbee/agent.toml` — so both resolve the same `state_dir`. For systemd-managed agents this means `sudo serverbee-agent grant …`.

---

## 7. Configuration

Add an optional `[capabilities]` field. No new env var is *required*; both have defaults.

| TOML key | Env var | Default | Notes |
|---|---|---|---|
| `capabilities.temporary_max_duration` | `SERVERBEE_CAPABILITIES__TEMPORARY_MAX_DURATION` | `24h` | Upper bound the CLI enforces on `grant --for`. Footgun guard only; not set-required. Accepts the same `Ns/Nm/Nh/Nd` grammar. |
| `capabilities.state_dir` | `SERVERBEE_CAPABILITIES__STATE_DIR` | `/var/lib/serverbee` | Directory holding `capability_grants.json`. |

Per project convention, **ENV.md and `apps/docs/content/docs/{en,zh}/configuration.mdx` are updated in the same change** as these keys.

`CapabilitiesConfig` (`crates/agent/src/config.rs`) gains:

```rust
#[serde(default = "default_temporary_max_duration")]
pub temporary_max_duration: String,   // parsed to Duration at use
#[serde(default = "default_capability_state_dir")]
pub state_dir: String,
```

---

## 8. Protocol additions (agent → server only)

New `AgentMessage` variant in `crates/common/src/protocol.rs`:

```rust
CapabilitiesChanged {
    msg_id: String,
    capabilities: u32,                    // new effective bitmask — drives server gates + display mirror
    temporary: Vec<TemporaryGrant>,       // all currently-active temporary grants — drives UI countdown
    changes: Vec<CapabilityChangeEvent>,  // deltas since last report — drives audit + alerts
}

pub struct TemporaryGrant {
    pub cap: String,
    pub granted_at: i64,
    pub expires_at: i64,
}

pub struct CapabilityChangeEvent {
    pub cap: String,
    pub action: CapabilityChangeAction,   // Granted | Expired | Revoked
    pub expires_at: Option<i64>,          // present for Granted
    pub granted_by: Option<String>,
    pub reason: Option<String>,
}
```

`PROTOCOL_VERSION` bumps **5 → 6**.

Server handler (`crates/server/src/router/ws/agent`):

1. **Update live capabilities** in `AgentManager` for the server (so `get_agent_local_capabilities` returns the new value and every control-plane gate — terminal/exec/file/docker — opens or closes immediately).
2. **Store `temporary[]`** alongside the capabilities in `AgentManager` (in-memory, `DashMap<String, Vec<TemporaryGrant>>`). Not persisted to the DB: it is transient and the agent re-reports on reconnect. A browser that connects or reloads mid-grant reads the active grants from the **REST server DTO** (the same DTO that already carries the `capabilities` mirror), since `ServerStatus`/`FullSync` do **not** carry capability data at all — on the web the initial capability values come from REST and live updates come from `BrowserMessage::CapabilitiesChanged`. The REST server list/detail handler reads `AgentManager::get_temporary_grants(server_id)`.
3. **Mirror for display:** `ServerService::update_capabilities_mirror(server_id, capabilities)` (existing path, the only writer of `servers.capabilities`).
4. **Audit:** for each `change`, write an `audit_logs` row (§ 9).
5. **Alerts:** feed `Granted` events for high-risk caps to the alert evaluator (§ 9).
6. **Broadcast** `BrowserMessage::CapabilitiesChanged { server_id, capabilities, temporary }` to browser clients.

No `ServerMessage` is added. The removed `ServerMessage::CapabilitiesSync` is **not** reintroduced. `SystemInfo` continues to carry the initial (possibly grant-augmented) bitmask and now also a `temporary[]` field for grants that survived a restart.

Browser-facing: `BrowserMessage::CapabilitiesChanged` (already exists, with `server_id`/`capabilities`/`agent_local_capabilities`/`effective_capabilities`) gains a `temporary: Vec<TemporaryGrant>` field. `ServerStatus`/`FullSync` are **not** touched (they carry no capability data); initial temporary state is delivered through the REST server DTO instead (§ 8.2 above).

---

## 9. Audit & alert chain

The user opted into the fullest visibility tier: re-report + countdown + audit + alert notification.

### 9.1 Audit

Server writes an `audit_logs` row per `CapabilityChangeEvent`. New actions:

| action | detail |
|---|---|
| `capability_temporarily_granted` | cap, duration/expiry, `granted_by`, `reason`, server name |
| `capability_grant_expired` | cap, server name |
| `capability_grant_revoked` | cap, `granted_by` of the revoked grant, server name |

Visible in **Settings → Audit Logs**. `user_id` is null (host-local origin); `granted_by` is carried in the detail. Survives restarts of audited grants are **not** logged as new grants (see § 5 seeding).

### 9.2 Alerts

Add a new **event-driven alert rule type** `capability_grant_detected`, modeled on the existing **`ip_changed`** precedent (an event-driven rule that is *not* part of the SSH/security-event pipeline and carries no `SecurityRuleParams`/event payload — it is triggered by a bare `AlertService::check_event_rules(db, config, state_manager, server_id, "capability_grant_detected")` call). It fires when a **high-risk** capability (`terminal`, `exec`, `file`, `docker`) is temporarily *granted*. The server calls `check_event_rules` from the `CapabilitiesChanged` handler for each high-risk cap that newly transitioned to *granted*. Reuses the existing alert → notification-group → channel pipeline and `(rule_id, server_id, event_key)` dedup. `capability_grant_detected` is added to `EVENT_DRIVEN_RULE_TYPES` (and the rule-type validation set) but **not** to `SECURITY_RULE_TYPES` or `SOURCE_IP_RULE_TYPES`. The Alerts page gets a preset card for one-click setup, mirroring the existing security preset cards in `apps/web/src/components/security/alert-presets.tsx`.

**Deliberate rejection of the SecurityEvent path:** a privilege-elevation audit must not be gated by `CAP_SECURITY_EVENTS` (it can be denied) and must not be Linux-only. Therefore the change events ride on `CapabilitiesChanged`, not on `AgentMessage::SecurityEvent`, and the server evaluates them through a dedicated alert type rather than the `/security` pipeline.

---

## 10. UI (read-only + countdown)

The capability surfaces are already read-only (agent-owned model). Add a **temporary** state:

- A cap enabled via a temporary grant renders a distinct **amber "Temporary"** badge instead of the plain "Enabled" badge, plus a **live client-side countdown** computed from `expires_at` (e.g. `expires in 28:14`), which auto-collapses to the off state at zero.
- Tooltip: *"Temporarily enabled on the agent host until HH:MM. Manage with `serverbee-agent grant` / `revoke` on the host."*
- Applies to the settings capability matrix (`routes/_authed/settings/capabilities.tsx`) and the per-server capabilities dialog (`components/server/capabilities-dialog.tsx`).
- **No toggles** — the UI stays read-only; granting happens only on the host.

Data sources: `temporary[]` from the **REST server DTO** (initial page load) and `BrowserMessage::CapabilitiesChanged` (live, merged into the `['servers']` query cache via `setServerCapabilities`). `apps/web/src/lib/capabilities.ts` gains a helper to classify a cap as `off | enabled | temporary` and to expose its `expires_at`. The live countdown reuses the `setInterval` + `useState` pattern already used by `mobile-pair-dialog.tsx`, extracted into a small `useCountdown` hook.

---

## 11. Testing

Rust (agent):
- `CapabilityGrantStore`: atomic read-modify-write; prune expired; one-record-per-cap replace; missing → empty; corrupt/unknown-version → empty (fail-safe); `0600` perms.
- Effective computation: `base | active`, grant cannot strip base, on-cap grant ignored, mask applied.
- Restart resume: file with a past `expires_at` → pruned; future `expires_at` → applied with correct remaining time (inject a fixed `now`).
- CLI parse: duration grammar, reject over-max, reject already-on cap, revoke absent cap.

Rust (server, integration):
- Agent sends `CapabilitiesChanged` → `AgentManager` live value updated → a previously-denied `exec`/terminal/file request now passes the gate; after `expired` it is denied again.
- Audit rows written for granted/expired/revoked; restart-surviving grant not re-audited.
- High-risk grant triggers `capability_grant_detected` → mock notification channel dispatched.
- `BrowserMessage::CapabilitiesChanged` carries `temporary[]`; `FullSync` includes active grants.

Frontend (vitest):
- Capability matrix renders the amber Temporary badge + countdown from `temporary[]` metadata; collapses at expiry.

Protocol:
- `PROTOCOL_VERSION == 6` assertion updated.

---

## 12. Documentation & changelog

- `apps/docs/content/docs/{en,zh}/capabilities.mdx`: new "Temporary grants" section — the `grant`/`revoke`/`grants` CLI, duration grammar, restart/expiry semantics, audit + alert behavior, and the host-only trust note.
- `apps/docs/content/docs/{en,zh}/configuration.mdx` + `ENV.md`: `temporary_max_duration`, `state_dir`.
- `apps/docs/content/docs/{en,zh}/admin.mdx`: new audit actions.
- `apps/docs/content/docs/{en,zh}/security.mdx` (and alerts.mdx): `capability_grant_detected` rule.
- `CHANGELOG.md`: Unreleased entry.
- `tests/` manual checklist: a temporary-grant E2E (grant → use via web → restart agent mid-window → still active → expire → denied + audited + alerted).

---

## 13. Trust model, restated

> Permanent capability state lives in `agent.toml` `[capabilities]`. Temporary capability state lives in `capability_grants.json`. Both files are writable only on the agent host by root. The agent computes `effective = base | active_grants` and reports it; the server is a read-only mirror that gates control-plane requests on the reported value, audits changes, and can alert — but can never request, push, or modify either state.
