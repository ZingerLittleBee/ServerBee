# Agent Registration Hardening Design

**Date:** 2026-03-29
**Status:** Draft
**Scope:** Agent registration protocol, server-side defense, dashboard management

## Problem

The current agent registration endpoint (`POST /api/agent/register`) creates a new server record on every call. When an agent repeatedly fails to connect via WebSocket (e.g., reverse proxy stripping auth headers), it re-registers in a tight loop, flooding the servers table with orphaned offline entries. Additionally, a leaked discovery key allows unlimited registrations from any source.

### Existing Protections

| Layer | Protection |
|-------|-----------|
| Registration | Bearer auth via `auto_discovery_key` |
| Registration | Rate limit: 3 per IP per 15min |
| WS auth | Token via query param (survives reverse proxies) |
| Re-registration | Capped at 3 consecutive attempts with exponential backoff |
| Token storage | Argon2 hashing |

### Gaps

1. No deduplication — same agent re-registering always creates a new server entry
2. No global server count limit
3. Discovery key rotation API exists (`PUT /api/settings/auto-discovery-key`) but the frontend has no "Regenerate" button — users must know the API exists
4. No bulk cleanup for orphaned server entries

## Design

### 1. Agent Fingerprint

**Goal:** Uniquely identify a machine so repeated registrations from the same agent reuse the existing server record.

**Fingerprint composition:**
- `hostname` — from `gethostname()`
- `machine_id` — platform-specific:
  - Linux: read `/etc/machine-id`
  - macOS: read `IOPlatformUUID` via `ioreg -rd1 -c IOPlatformExpertDevice`
  - Windows: read `MachineGuid` from registry `HKLM\SOFTWARE\Microsoft\Cryptography`

**Fingerprint generation:**
```
fingerprint = SHA-256("{hostname}:{machine_id}")  →  64-char hex string
```

SHA-256 ensures a fixed-length, non-reversible identifier regardless of input length.

**Tradeoff: hostname in fingerprint.** If a machine is renamed, the fingerprint changes and a new server entry will be created. This is an intentional tradeoff — hostname distinguishes machines behind the same NAT that might share a machine_id (e.g., cloned VMs). The old server entry can be cleaned up manually via the bulk cleanup feature.

**Agent-side implementation:**
- New module `crates/agent/src/fingerprint.rs`
- Platform-specific machine_id reading via `#[cfg(target_os = "...")]`
- Reads hostname + machine_id at startup
- Returns hex-encoded SHA-256 hash
- Graceful fallback: if machine_id is unreadable, return empty string (skip fingerprint, fall through to old path)

**Docker consideration:**
- Docker containers get a new machine-id on each rebuild
- Solution: mount host's machine-id read-only: `-v /etc/machine-id:/etc/machine-id:ro`
- Documented in deployment docs

### 2. Registration Protocol Change

**Current:** `POST /api/agent/register` with empty body `{}`

**New:** `POST /api/agent/register` with body `{ "fingerprint": "<sha256_hex>" }`

`fingerprint` is optional for backward compatibility with old agents.

**Server-side flow:**

```
1. Rate limit check (existing)
2. Discovery key validation (existing)
3. If fingerprint is provided and non-empty:
   a. SELECT * FROM servers WHERE fingerprint = <hash>
   b. If found → REUSE PATH:
      - Generate new token, hash with argon2
      - UPDATE token_hash, token_prefix, updated_at
      - Return existing server_id + new plaintext token
      - Log: "Reusing server {id} for fingerprint {hash}"
      - (skip global limit check — not creating a new server)
   c. If not found → NEW SERVER PATH (continue to step 4)
4. Global server limit check (new — see section 3)
5. Create new server:
   - If fingerprint provided: set fingerprint field
   - If no fingerprint (old agent): fingerprint = NULL
6. Return { server_id, token }
```

**Race condition handling:**
Two simultaneous registrations from the same machine could race between the SELECT (step 3a) and INSERT (step 5). The unique index on `fingerprint` ensures only one INSERT succeeds. The loser gets a unique constraint violation, which the handler catches and retries as a reuse (SELECT + UPDATE). Implementation: wrap the SELECT-then-INSERT in a single function that catches `SqlErr::UniqueConstraintViolation` on the fingerprint column and falls back to the reuse path.

**Key behaviors:**
- Fingerprint match reuses the server even if the agent's IP changed (VPS migration, etc.)
- Token is always regenerated on re-registration (old token becomes invalid)
- A reused server retains its name, group, capabilities, and other user-configured fields

### 3. Global Server Limit

**Configuration:**
- Field: `auth.max_servers` (`u32`)
- Default: `0` (no limit)
- Env var: `SERVERBEE_AUTH__MAX_SERVERS`
- TOML: `[auth] max_servers = 50`

**Check logic:**
- Only checked when creating a **new** server (fingerprint reuse does not count)
- `SELECT COUNT(*) FROM servers`
- If `count >= max_servers && max_servers > 0` → return `400 Bad Request` with message: "Server limit reached ({max_servers}). Delete unused servers or increase max_servers in config."

**This is a best-effort soft cap, not a hard limit.** Concurrent registrations with different fingerprints can race past the COUNT check and slightly exceed the limit. This is acceptable because: (1) the primary defense against abuse is the discovery key + rate limit, not the cap; (2) SQLite serializes writes so the overshoot is at most a few rows; (3) adding transaction-level locking for an exact hard limit adds complexity disproportionate to the threat model. The cap prevents runaway growth, not exact enforcement.

### 4. Discovery Key Rotation (Frontend Only)

The server already has `PUT /api/settings/auto-discovery-key` which generates a new random key and returns it. No new backend endpoint needed.

**Frontend change:**
- On the existing discovery key settings page, add a "Regenerate" button next to the key input
- On click: call `PUT /api/settings/auto-discovery-key` → display new key → prompt user to copy it
- Show warning: "This will invalidate the current key. Already connected agents are not affected."

### 5. Bulk Cleanup of Orphaned Servers

**Endpoint:** `DELETE /api/servers/cleanup`

**Auth:** Requires Admin role

**Cleanup criteria:**
- `name = 'New Server'` (default name, never customized by user)
- `os IS NULL` (never received a system info report from an agent)

These two conditions together identify servers that were registered but never successfully connected.

**Flow:**
1. Count matching servers: `SELECT COUNT(*) FROM servers WHERE name = 'New Server' AND os IS NULL`
2. Collect the IDs of matched servers
3. For each orphan server_id, clean up related data in two categories:

   **Tables with `server_id` FK (delete rows WHERE server_id = ?):**
   - `records`, `record_hourlys`, `gpu_records`
   - `alert_states`
   - `network_probe_configs`, `network_probe_records`, `network_probe_record_hourlys`
   - `traffic_states`, `traffic_hourlys`, `traffic_dailys`
   - `uptime_dailys`
   - `task_results`
   - `server_tags`
   - `docker_events`
   - `ping_records`

   **Tables with `server_ids_json` array — per-table rules (empty array has different semantics per table):**

   For each table, parse the JSON array, remove the orphan ID, then apply the table-specific rule:

   | Table | If array becomes empty after removal |
   |-------|--------------------------------------|
   | `ping_tasks` | **Delete the row.** Empty array means "all agents" in current code — leaving it would unintentionally expand scope. |
   | `tasks` | **Delete the row.** Task creation forbids empty `server_ids`; an empty array is invalid state. |
   | `maintenances` | **Delete the row.** Empty array means "all servers" — leaving it would expand scope. |
   | `alert_rules` | **Delete the row.** With `cover_type = "exclude"`, empty array means "match all servers". Safer to delete. |
   | `service_monitors` | **Set `server_ids_json` to NULL.** This field is optional and only used for maintenance suppression linkage, not dispatch scope. Deleting the row would destroy monitor config and historical records. |
   | `incidents` | **Keep the row.** Incidents are historical records; removing server references is fine, the incident itself should be preserved. |
   | `status_pages` | **Keep the row.** Status pages are user-configured; empty server list just means no servers displayed. |

   If the array still has remaining IDs after removal, always write back the updated array (never delete the row).

4. Delete the matched server records themselves
5. Return `{ data: { deleted_count: N } }`

Note: Since these are "never connected" servers, most related tables will have no data. The per-table rules above are for correctness to prevent silent semantic changes in shared configuration.

**Frontend:**
- Server list page toolbar: "Clean up unconnected servers" button
- On click: GET count first → show confirmation dialog: "Delete {N} servers that never connected?"
- On confirm: call DELETE API → refresh server list
- Button disabled (greyed out) when count is 0

### 6. Database Migration

**New migration:** `m20260329_add_server_fingerprint`

Add column to `servers` table:
```sql
ALTER TABLE servers ADD COLUMN fingerprint VARCHAR NULL;
CREATE UNIQUE INDEX idx_servers_fingerprint ON servers(fingerprint) WHERE fingerprint IS NOT NULL;
```

- Nullable: existing servers and old-agent registrations have no fingerprint
- Unique partial index: ensures one server per fingerprint, allows multiple NULLs
- Only `up()` implemented (per project convention, `down()` is a no-op)

### 7. Documentation Updates

**Files to update:**

| File | Change |
|------|--------|
| `apps/docs/content/docs/cn/configuration.mdx` | Add `SERVERBEE_AUTH__MAX_SERVERS` env var; add Docker machine-id mount instruction |
| `apps/docs/content/docs/en/configuration.mdx` | Same as CN |
| `ENV.md` | Add `SERVERBEE_AUTH__MAX_SERVERS` |
| `deploy/railway/README.md` | Add `MAX_SERVERS` config |

**Docker agent deployment note (CN + EN):**
> When running the agent in Docker, mount the host's machine-id to ensure correct fingerprint identification:
> ```
> -v /etc/machine-id:/etc/machine-id:ro
> ```

## Files to Modify

### Agent (`crates/agent/`)
| File | Change |
|------|--------|
| `src/fingerprint.rs` | **New.** Read hostname + machine_id, produce SHA-256 hex |
| `src/main.rs` | Call fingerprint at startup, pass to register |
| `src/register.rs` | Add `fingerprint` field to RegisterRequest body |
| `src/lib.rs` or `main.rs` | Add `mod fingerprint` |

### Server (`crates/server/`)
| File | Change |
|------|--------|
| `src/entity/server.rs` | Add `fingerprint: Option<String>` field |
| `src/migration/` | New migration: add fingerprint column + unique partial index |
| `src/router/api/agent.rs` | Fingerprint dedup logic, global limit check, race condition retry, store fingerprint + IP on new server |
| `src/router/api/server.rs` | New endpoint: `DELETE /servers/cleanup` (with JSON array cleanup for shared-config tables) |
| `src/config.rs` | Add `max_servers: u32` to AuthConfig |

### Frontend (`apps/web/`)
| File | Change |
|------|--------|
| Settings page (discovery key) | Add "Regenerate" button (calls existing `PUT /api/settings/auto-discovery-key`) |
| Server list page | Add "Clean up unconnected servers" button + confirmation dialog |
| API client | Add `cleanupServers()` call |

### Documentation
| File | Change |
|------|--------|
| `apps/docs/content/docs/cn/configuration.mdx` | MAX_SERVERS env var + Docker machine-id mount |
| `apps/docs/content/docs/en/configuration.mdx` | Same |
| `ENV.md` | MAX_SERVERS |
| `deploy/railway/README.md` | MAX_SERVERS |

## Backward Compatibility

- Old agents (without fingerprint) continue to work — registration creates new server with `fingerprint = NULL`
- Old agents can still re-register but without dedup (existing rate limit + re-registration cap still protect)
- No breaking API changes — `fingerprint` is an optional field in the request body
- Existing server records are unaffected by the migration (fingerprint defaults to NULL)
