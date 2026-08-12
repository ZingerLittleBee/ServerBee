# testloop — 2026-08-12 — broad VPS regression

**Verdict:** pass · **Rounds:** 3

Fresh install of current main on the dedicated Linux test VPS (Debian 13,
plain HTTP on :9527), one real agent on the same box with terminal / exec /
file / docker capabilities enabled. The deployment itself exercised the real
first-run path: random admin password from the logs → forced onboarding
password change → enrollment code → agent claim.

## Covered

- Auth (login, wrong-password error, session persistence, logout, member
  role isolation, audit log), first-run onboarding, agent enrollment UI.
- Dashboards: create / rename / delete / edit mode, add-widget flow, layout
  persistence; live WebSocket metric updates.
- Server detail: metrics charts, system info, terminal (PTY commands), file
  manager (mkdir, upload, download with byte-for-byte verification, rename,
  delete, root-path escape rejection), docker (list + log streaming,
  read-only).
- Service monitors (HTTP keyword up/down verdicts), ping tasks (ICMP results
  chart), scheduled commands (run-now via exec), alert rules (threshold
  trigger with 5-min debounce, webhook failure handling), security events
  (injected SSH auth failures), public status page.

## Found & fixed

- `crypto.randomUUID` is unavailable on plain-HTTP origins: the /servers page
  crashed with an error boundary and the dashboard Add Widget flow failed
  silently. Fixed with a `getRandomValues`-based fallback in
  `apps/web/src/lib/uuid.ts` (used by `add-server-dialog.tsx` and
  `use-dashboard-editor.ts`).
- Public status page: the admin UI promises "leave empty to include every
  server" but an empty selection resolved to zero servers, so /status showed
  "No servers available". Fixed in
  `crates/server/src/service/public_status.rs` (`resolve_scope`), with an
  integration regression test in `tests/public_status_gating.rs`.

## Still open

- UX observation, not a defect: ping-task latency lives only under
  Settings → Ping Tasks; the server-detail Network tab belongs to the
  separate network-probe feature and shows "No probe targets configured"
  even when ping tasks are collecting. Two independent test rounds went
  looking for ping data there first — worth considering a cross-link or an
  empty-state hint.
- Round-1 terminal first-connect needed one manual Reconnect; not
  reproducible in round 2 (connected first try). Watch for recurrence.
