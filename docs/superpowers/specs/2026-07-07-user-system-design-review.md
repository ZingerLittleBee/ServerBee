# User System Design Review

**Date:** 2026-07-07
**Status:** Concluded — 6 decisions settled; follow-ups implemented alongside this document
**Scope:** The two-tier RBAC user system: `users` table, `admin`/`member` roles, `/settings/users` management UI, `require_admin` route gating, API keys, sessions.

## Context

ServerBee ships a deliberately simple user model:

- `users.role` is a plain string, validated to `"admin" | "member"` (`crates/server/src/service/user.rs`).
- A default `admin` user is bootstrapped only when the users table is empty, with a random password and `must_change_password = true` (`AuthService::init_admin`).
- Authorization is enforced at the router layer: public routes → authenticated read routers → a write-router block wrapped in `require_admin` (`crates/server/src/router/api/mod.rs`). Terminal and Docker-log WebSockets check `role == "admin"` at upgrade time.
- The web UI fail-closes: every `/settings` route is admin-only except an explicit member allowlist (`MEMBER_SETTINGS_ROUTES` in `apps/web/src/routes/_authed.tsx`).

This document records the design decisions confirmed during the review, one per section.

## Decision 1 — Member is a trusted, fleet-wide, read-only observer

**Question:** Should `member` get per-server / per-group visibility scoping, or stay global read-only?

**Decision:** Keep the global two-tier model. `member` is positioned as a **trusted collaborator with full fleet visibility and zero control**. No per-server ACL will be built.

**Rationale:**

- ServerBee already has a three-tier disclosure spectrum; each audience maps to an existing tool:

  | Audience | Tool | Visibility |
  |---|---|---|
  | Strangers / customers | Status pages | Hand-picked servers, masked/aggregated status |
  | Trusted collaborators | `member` account | All servers, all monitoring detail, no control |
  | Operators | `admin` account | Everything |

- "Show a customer their one server" is served by creating a status page for that server (already supports per-server selection and IP masking) — not by scoping member accounts.
- Per-server ACL would require filtering 30+ read endpoints, the browser WebSocket FullSync/Update fan-out, and every cross-server aggregate (insights, alert event streams). Any missed endpoint is a privilege leak. That complexity tax is not worth paying in a single-tenant, self-hosted product; true multi-tenant isolation should be solved by deploying separate instances.

**Follow-ups (cheap, disclosure-only):**

- [x] Create-user dialog: the `member` role option must state that members can see **all** servers' monitoring data (including security events and public IPs) with no write access.
- [x] User-management docs page: state the same boundary and point to status pages for external/partial exposure.

## Decision 2 — No forced password change for admin-created users

**Question:** `AuthService::create_user` hardcodes `must_change_password = false`, and the admin password-reset path (`PUT /api/users/{id}` with `password`) does not set it either — so the admin permanently knows the initial/reset password. Should both paths set `must_change_password = true` (reusing the existing onboarding machinery built for the bootstrap admin)?

**Decision:** Keep as-is. No forced first-login password change for admin-created or admin-reset accounts.

**Rationale (owner's call):** The admin is fully trusted and already holds maximum privilege over the whole system; knowing a member's password grants nothing the admin cannot already do. Forcing a change would add onboarding friction without a meaningful security gain in this trust model. The bootstrap admin keeps its forced change because its random password is printed to the startup log (a broader exposure channel).

**Clarification recorded during this decision:** A bootstrap-created admin and a UI-created admin are identical after creation. There is no "super admin" concept — `role` is a plain string and every check is `role == "admin"`. Differences exist only at birth: the bootstrap admin has a fixed `admin` username, a random password shown once in a startup banner, `must_change_password = true`, and **no audit entry** (UI creation writes `user.create` to the audit log). The bootstrap admin can be deleted or demoted like any other admin as long as the count-based last-admin guard is satisfied. There is no config/env path that seeds the bootstrap admin password in production; `admin/admin123` exists only in the dev demo seed.

## Decision 3 — Live WebSocket channels are not re-validated after role changes (known boundary)

**Question:** Terminal (`ws/terminal.rs`) and Docker-log (`ws/docker_logs.rs`) WebSockets check `role == "admin"` only at upgrade time. A demoted or deleted admin's already-open PTY/log stream stays alive until the client disconnects, even though REST access is revoked immediately (role is re-read from the DB on every request, and deletion drops sessions). Should the WS loops periodically re-validate the session and role (e.g., every 60s) and close on failure?

**Decision:** No — rejected as overdesign for this product's trust model. Recorded as a known boundary instead.

**Rationale (owner's call):** Admin demotion/removal is a rare, deliberate act performed by another trusted admin in a single-operator/small-team deployment. The residual window (an already-open terminal surviving until disconnect) does not justify adding re-validation machinery to the WS loops. Operators who need immediate revocation can restart the server process, which drops all WS connections.

## Decision 4 — OAuth auto-registration risk is handled by documentation, not an allowlist

**Question:** With `SERVERBEE_OAUTH__ALLOW_REGISTRATION=true` (default `false`), any identity that can authenticate against the configured OAuth provider is auto-provisioned as a `member` on first login (`OAuthService::find_or_create_user`), with no domain/org/username allowlist and no approval step. Combined with Decision 1 (member = fleet-wide read visibility), enabling this against a public provider such as GitHub effectively publishes all read-only monitoring data — public IPs, process lists, security events — to every user of that provider. Should an allowlist be added?

**Decision:** No allowlist. Documentation-only mitigation.

**Rationale:** The flag is fail-closed by default, and the legitimate use case (a private/self-hosted IdP where the provider itself is the allowlist) doesn't need extra config. The danger is purely one of operator expectation, so it is fixed at the disclosure layer.

**Follow-ups:**

- [x] ENV.md `SERVERBEE_OAUTH__ALLOW_REGISTRATION` row and the OAuth docs page must warn: enable only when the OAuth provider itself is access-controlled (self-hosted OIDC / org-internal IdP). Enabling it with a public provider (e.g., GitHub) grants every user of that provider full read access to all monitoring data, including security events and public IPs.

## Verified clean — no credential leakage to members

Checked during the review, no action needed: `ServerResponse` deliberately excludes `token_hash`/`token_prefix` (exposes only a `has_token` bool), and `agent::read_router` exposes only `/agent/latest-version`. Members have no read path to agent tokens or enrollment secrets, so a member account cannot escalate to agent impersonation.

## Decision 5 — Fix the last-admin guard race (wrap check + mutation in a transaction)

**Question:** The last-admin guard in `UserService::update_user` (demote path) and `UserService::delete_user` is check-then-act: the `count(role = 'admin')` query and the subsequent write run outside a shared transaction. Two concurrent requests (e.g., two admins demoting each other) can both observe `admin_count = 2`, both pass, and leave the system with **zero admins**. Because `init_admin` re-bootstraps only when the users table is empty, an all-admins-demoted state is unrecoverable in-product — every `require_admin` route (including user management itself) returns 403 forever; the only way out is manual SQLite surgery.

**Decision:** Fix. Wrap the guard count and the mutation in a single transaction in both paths (`update_user` role-demotion branch, `delete_user`). SQLite serializes write transactions, so the second concurrent request re-reads `admin_count = 1` inside its transaction and is correctly rejected. No new concepts or config; `delete_user`'s multi-table cleanup should have been transactional anyway.

**Status:** Implemented — `update_user` and `delete_user` each run guard + mutation in one transaction.

## Decision 6 — Admin lockout recovery is documented, not built

**Question:** There is no forgot-password flow (reasonable — self-hosted, no mail channel), and passwords are argon2 hashes that cannot be hand-crafted via the sqlite CLI. If the only admin loses their password or 2FA device, the only real recovery is: stop the server → `sqlite3 serverbee.db "DELETE FROM users;"` → restart, which triggers `init_admin` re-bootstrap (new random password in the startup banner). This works but is written down nowhere. Should ServerBee ship a `reset-admin-password` CLI subcommand instead?

**Decision:** No CLI. Document the wipe-and-rebootstrap recovery procedure in the docs site.

**Rationale:** Anyone who can reach the DB file already has host-level access, so a CLI adds no new capability — only a new maintained entry point. The recovery recipe (with its side effects: all user accounts and API-key ownership are reset; monitoring data untouched) belongs in the troubleshooting docs.

**Follow-ups:**

- [x] Add a "Lost admin access" recovery section to the docs (Admin Guide, CN+EN).

## Summary of outcomes

| # | Topic | Outcome |
|---|---|---|
| 1 | Member scoping | Keep global read-only member; external exposure → status pages; add UI/docs disclosure |
| 2 | Forced password change for created users | Keep as-is (admin is fully trusted) |
| 3 | Live WS revalidation after role change | Known boundary, no code change |
| 4 | OAuth auto-registration blast radius | Docs warning only, no allowlist |
| 5 | Last-admin guard race | **Fix**: wrap count+mutation in one transaction (update_user demote path, delete_user) |
| 6 | Admin lockout recovery | Document wipe-and-rebootstrap procedure, no CLI |
