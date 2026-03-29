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
3. No way to rotate discovery key without server restart
4. No bulk cleanup for orphaned server entries

## Design

### 1. Agent Fingerprint

**Goal:** Uniquely identify a machine so repeated registrations from the same agent reuse the existing server record.

**Fingerprint composition:**
- `hostname` — from `gethostname()`
- `machine_id` — Linux: read `/etc/machine-id`; macOS: read `IOPlatformUUID` via `ioreg`

**Fingerprint generation:**
```
fingerprint = SHA-256("{hostname}:{machine_id}")  →  64-char hex string
```

SHA-256 ensures a fixed-length, non-reversible identifier regardless of input length.

**Agent-side implementation:**
- New module `crates/agent/src/fingerprint.rs`
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
3. Global server limit check (new — see section 3)
4. If fingerprint is provided and non-empty:
   a. SELECT * FROM servers WHERE fingerprint = <hash>
   b. If found:
      - Generate new token, hash with argon2
      - UPDATE token_hash, token_prefix, updated_at
      - Return existing server_id + new plaintext token
      - Log: "Reusing server {id} for fingerprint {hash}"
   c. If not found:
      - Create new server with fingerprint field set
5. If no fingerprint (old agent):
   - Create new server (existing logic, fingerprint = NULL)
6. Return { server_id, token }
```

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

### 4. Discovery Key Rotation API

**Endpoint:** `POST /api/settings/rotate-discovery-key`

**Auth:** Requires Admin role (existing `require_admin` middleware)

**Flow:**
1. Generate new random key (32 bytes, base64url-encoded)
2. Write to ConfigService (`auto_discovery_key`)
3. Return `{ data: { key: "<new_key>" } }`

**Behavior:**
- Old key is immediately invalidated
- Already-registered agents are unaffected (they authenticate with their per-server token, not the discovery key)
- New agents must use the new key to register

**Frontend:**
- On the existing discovery key settings page, add a "Regenerate" button next to the key input
- On click: call API → display new key → prompt user to copy it
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
2. For each matched server, delete related data by `server_id` from all dependent tables:
   - `alert_rules`, `alert_states`
   - `records`, `record_hourlys`, `gpu_records`
   - `ping_tasks`, `ping_records`
   - `network_probe_configs`, `network_probe_records`, `network_probe_record_hourlys`
   - `traffic_states`, `traffic_hourlys`, `traffic_dailys`
   - `uptime_dailys`
   - `tasks`, `task_results`
   - `server_tags`
   - `docker_events`
   - `incidents`, `maintenances`, `service_monitors`
3. Delete the matched server records themselves
4. Return `{ data: { deleted_count: N } }`

Note: Since these are "never connected" servers, most related tables will have no data. The cascade is for correctness.

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
| `src/router/api/agent.rs` | Fingerprint dedup logic, global limit check, store fingerprint + IP on new server |
| `src/router/api/settings.rs` | New endpoint: `POST rotate-discovery-key` |
| `src/router/api/server.rs` | New endpoint: `DELETE /servers/cleanup` |
| `src/config.rs` | Add `max_servers: u32` to AuthConfig |

### Frontend (`apps/web/`)
| File | Change |
|------|--------|
| Settings page (discovery key) | Add "Regenerate" button |
| Server list page | Add "Clean up unconnected servers" button + confirmation dialog |
| API client | Add `rotateDiscoveryKey()` and `cleanupServers()` calls |

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
