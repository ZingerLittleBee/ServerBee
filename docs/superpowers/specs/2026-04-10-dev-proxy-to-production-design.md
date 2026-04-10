# Dev Proxy to Production Design

**Date:** 2026-04-10
**Status:** Draft
**Scope:** `apps/web` frontend development workflow

## Problem

When iterating on frontend UI (styles, layouts, live charts, realtime
indicators), the developer needs to see **real production data in real time**
— not a snapshot. The existing workflows fall short:

- `make dev` runs a local Rust server with an empty SQLite DB. No data at
  all.
- `make db-pull` downloads a VACUUM'd SQLite snapshot from production via the
  backup API and runs the local server against it. This shows real data, but
  it is a **frozen snapshot**. Live charts, WebSocket pushes
  (`ServerUpdate`, `ServerOnline`, `CapabilitiesChanged`), and alert flickers
  are all dead because no agents are connected to the local server.

The stated workaround "add PostgreSQL support so the local server can
connect to a production DB" does not actually solve this: even if the local
server queried the production DB, its in-memory `AgentManager` and
`browser_tx: broadcast::Sender<BrowserMessage>` would still be empty because
no agents are connected to the local process. UI realtime behavior would
remain dead.

## Goal

Enable a development mode where:

- The local Vite dev server serves the frontend with HMR as usual.
- **All HTTP API calls and WebSocket connections from the browser are
  transparently forwarded to the production Railway backend.**
- The browser sees live production data, live WebSocket pushes, and live
  agent state — because it is talking to the production server that actually
  has agents connected.
- Write operations are blocked by default so style/layout iteration cannot
  accidentally mutate production state.
- Authentication state is isolated: the proxy strips `Cookie`,
  `Authorization`, and `Set-Cookie` headers so the backend's
  session/Bearer auth paths can never be exercised, forcing every request
  through the injected read-only `X-API-Key`. The UI cannot accidentally
  "log into production".
- A persistent visual banner in the UI makes it impossible to forget that
  the current session is wired to production.

## Non-Goals

- Do **not** add PostgreSQL, MySQL, or any other database backend to the
  Rust server.
- Do **not** modify any Rust code (`crates/*`), any DB schema, or any
  migration.
- Do **not** run the local Rust server against a remote database.
- Do **not** support multiple concurrent production targets. One URL at a
  time is enough.
- Do **not** auto-switch HTTPS on the Vite dev server.
- Do **not** change the default `make dev` / `bun run dev` behavior.

## Design Overview

Introduce a new Vite mode, `prod-proxy`, launched via a new script
`bun run dev:prod` (and a matching `make web-dev-prod` menu entry). In
this mode, the proxy logic lives in an **extracted module**
`apps/web/vite/dev-proxy.ts` (so it can be unit-tested in isolation from
the full Vite config), and `apps/web/vite.config.ts` wires it into the
`server.proxy['/api']` entry when `mode === 'prod-proxy'`.

The extracted module's responsibilities:

1. Validate required env vars at config-build time. Both
   `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_READONLY_API_KEY` (new
   variable — see Env Vars section below) must be present; missing either
   causes Vite to fail fast with a clear error.
2. Short-circuit writes. On `proxyReq`, if the incoming request's method
   is not in `{GET, HEAD, OPTIONS}` and `ALLOW_WRITES` is not `1`,
   respond with HTTP `403` + JSON body and abort the upstream request so
   it never leaves the dev machine.
3. Block auth mutation paths. Requests whose path starts with
   `/api/auth/` are rejected with `403`. The allow-list is **exactly**
   `GET /api/auth/me` — the UI needs this endpoint to render "current
   user", and tightening both path and method prevents drift if the
   backend ever adds `POST /api/auth/me` or similar. Every other
   `/api/auth/*` request is blocked, which prevents OAuth callbacks,
   login, logout, and 2FA flows from ever touching production.
4. Strip auth headers from the request before forwarding. `Cookie` and
   `Authorization` are removed so the backend's `validate_browser_auth`
   can never take the session or Bearer path; the only remaining
   credential is the injected `X-API-Key`. Applied to both `proxyReq`
   (HTTP) and `proxyReqWs` (WebSocket upgrade).
5. Inject `X-API-Key: <SERVERBEE_PROD_READONLY_API_KEY>` on every
   forwarded request (HTTP and WebSocket upgrade). This is the **only**
   credential the backend will see.
6. Strip `Set-Cookie` from responses (on the `proxyRes` event) so the
   browser cannot accumulate any production session cookies even across
   unusual paths.
7. Expose the production URL to the frontend via Vite `define` as
   `import.meta.env.VITE_DEV_PROXY_TARGET`, consumed by the banner
   component.

The browser-side WebSocket client (`apps/web/src/lib/ws-client.ts`)
constructs its URL from `window.location.host`, so all WebSocket traffic
naturally flows through the Vite dev server and is handed to `http-proxy`
via the existing `ws: true` setting. The `/api/ws/servers` endpoint on the
Rust server already supports `X-API-Key` authentication
(`crates/server/src/router/ws/browser.rs:54-59`), so header injection on
the upgrade request is sufficient — combined with cookie stripping, the
backend's session and Bearer code paths are unreachable via the proxy.

Environment variable strategy — **split, not shared**:

- `SERVERBEE_PROD_URL` — shared between `db-pull` and the dev proxy. Same
  production host for both workflows.
- `SERVERBEE_PROD_API_KEY` — **unchanged**, continues to be an admin-role
  key used by `scripts/db-pull.sh` (the backup API requires admin). The
  dev proxy **does not read this variable**.
- `SERVERBEE_PROD_READONLY_API_KEY` — **new variable**, required in
  prod-proxy mode. Must be a member-role (read-only) API key. Vite fails
  to start if missing.

The split is deliberate: the previous iteration of this spec proposed
reusing a single `SERVERBEE_PROD_API_KEY` for both workflows, but that
creates a long-term pressure to store an admin key there (because
`db-pull` strictly requires it), which would silently weaken the dev
proxy's read-only guarantee by making `ALLOW_WRITES=1` + accidental
`POST` effective against production. Two variables with two explicit
permission models cannot drift.

`.env` lives in the project root; the existing `scripts/db-pull.sh` loads
it from there. `vite.config.ts` will do the same using
`loadEnv(mode, path.resolve(import.meta.dirname, '../..'), '')`.

## Data Flow

```
Browser (http://localhost:5173)
   │
   │ fetch('/api/servers')         window.fetch / apiClient
   ▼
Vite Dev Server (middlewares)
   │
   │ http-proxy configure hooks (all applied in order on proxyReq):
   │   1. if path starts with /api/auth/ and NOT (method=GET & path=/api/auth/me)
   │        → respond 403 { error: "Auth paths blocked in dev proxy" }, STOP
   │   2. if method ∉ {GET, HEAD, OPTIONS} and ALLOW_WRITES != "1"
   │        → respond 403 { error: "Dev proxy is read-only" }, STOP
   │   3. strip request headers: Cookie, Authorization
   │   4. inject X-API-Key: <SERVERBEE_PROD_READONLY_API_KEY>
   │
   │ on proxyRes:
   │   5. strip response header: Set-Cookie
   ▼
Railway Production (https://xxx.up.railway.app)
   │
   │ middleware/auth.rs:
   │   - no Cookie → session path skipped
   │   - no Authorization Bearer → bearer path skipped
   │   - X-API-Key present → validate_api_key → member user
   │ sea-orm → SQLite
   │
   ▼
Response → Set-Cookie stripped at Vite → Browser render
(Browser has no production session state, ever.)

-------- WebSocket path --------

Browser new WebSocket('ws://localhost:5173/api/ws/servers')
   │
   │ Browser automatically attaches Cookie from localhost:5173 origin
   │ (usually empty; in any case stripped in next step)
   ▼
Vite Dev Server (http-proxy WebSocket upgrade)
   │
   │ proxyReqWs hook:
   │   - strip Cookie, Authorization headers
   │   - inject X-API-Key
   ▼
Railway WebSocket Upgrade → router/ws/browser.rs
   │
   │ validate_browser_auth() → X-API-Key path wins (only option left)
   │ subscribe to broadcast::Sender<BrowserMessage>
   ▼
Live ServerUpdate / ServerOnline / ServerOffline / CapabilitiesChanged
messages stream back through the proxy to the browser in real time.
```

## Components

Scaled to the small size of this change:

### 1. `apps/web/vite/dev-proxy.ts` (new, extracted module)

All proxy logic lives here so it can be unit-tested without spinning up
Vite. This file exports one factory function:

```ts
// apps/web/vite/dev-proxy.ts
import type { ProxyOptions } from 'vite'

export interface DevProxyOptions {
  target: string                   // validated prod URL
  readonlyApiKey: string           // validated member-role key
  allowWrites: boolean             // true iff ALLOW_WRITES=1
}

// Return the /api ProxyOptions entry for Vite.
// Pure factory: no env reads, no side effects.
export function createDevProxy(opts: DevProxyOptions): ProxyOptions {
  return {
    target: opts.target,
    changeOrigin: true,
    ws: true,
    configure: (proxy) => {
      proxy.on('proxyReq', (proxyReq, req, res) => {
        const url  = req.url ?? ''
        const method = (req.method ?? 'GET').toUpperCase()

        // 1. Block auth paths. Allow-list is EXACTLY `GET /api/auth/me`.
        //    Both path and method must match; a hypothetical future
        //    `POST /api/auth/me` would be blocked.
        if (url.startsWith('/api/auth/')) {
          const isAllowedAuthRead = method === 'GET' && url === '/api/auth/me'
          if (!isAllowedAuthRead) {
            respond403(res, proxyReq,
              'Auth paths are blocked in dev proxy to prevent production session leakage')
            return
          }
        }

        // 2. Block writes by default
        const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
        if (!isReadOnly && !opts.allowWrites) {
          respond403(res, proxyReq,
            'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.')
          return
        }

        // 3. Strip auth request headers (defeats session/Bearer auth)
        proxyReq.removeHeader('cookie')
        proxyReq.removeHeader('authorization')

        // 4. Inject the ONLY credential the backend will see
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })

      // WebSocket upgrade: same stripping + injection
      proxy.on('proxyReqWs', (proxyReq) => {
        proxyReq.removeHeader('cookie')
        proxyReq.removeHeader('authorization')
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })

      // 5. Strip Set-Cookie from all responses so the browser never
      //    accumulates a production session
      proxy.on('proxyRes', (proxyRes) => {
        delete proxyRes.headers['set-cookie']
      })
    },
  }
}

function respond403(res: any, proxyReq: any, message: string) {
  res.writeHead(403, { 'content-type': 'application/json' })
  res.end(JSON.stringify({ error: message }))
  // Abort the upstream request so bytes never reach production
  if (typeof proxyReq.destroy === 'function') proxyReq.destroy()
  else if (typeof proxyReq.abort === 'function') proxyReq.abort()
}
```

Notes:
- The factory takes already-validated inputs. Env validation lives in
  `vite.config.ts` and runs before the factory is called. This separates
  "does the environment look right?" (config concern) from "what does the
  proxy do?" (pure logic), making the unit tests trivial.
- `proxyReq.destroy()` is the http-proxy 1.x / http-proxy-3 API;
  `proxyReq.abort()` is the legacy Node 12- fallback. The helper covers
  both so the code stays version-tolerant. Implementation should verify
  against the installed http-proxy version and simplify if possible.
- This module intentionally imports no React, no node:fs, no env access,
  so tests can `import { createDevProxy }` and drive it with mocks.

### 2. `apps/web/vite.config.ts` (modified)

Convert from object form to function form. All the proxy logic is
delegated to the extracted module; this file only validates env vars and
wires things together:

```ts
import path from 'node:path'
import { defineConfig, loadEnv, type ProxyOptions } from 'vite'
// ... existing plugin imports
import { createDevProxy } from './vite/dev-proxy'

export default defineConfig(({ mode }) => {
  const isProdProxy = mode === 'prod-proxy'

  // Load .env from project root (two levels up from apps/web)
  const rootEnv = loadEnv(mode, path.resolve(import.meta.dirname, '../..'), '')

  let apiProxy: ProxyOptions
  let devProxyTarget: string | undefined

  if (isProdProxy) {
    const target = rootEnv.SERVERBEE_PROD_URL
    const readonlyApiKey = rootEnv.SERVERBEE_PROD_READONLY_API_KEY
    const allowWrites = rootEnv.ALLOW_WRITES === '1'

    if (!target) {
      throw new Error(
        'SERVERBEE_PROD_URL is required for dev:prod; set it in the project root .env'
      )
    }
    if (!readonlyApiKey) {
      throw new Error(
        'SERVERBEE_PROD_READONLY_API_KEY is required for dev:prod. ' +
        'Generate a MEMBER-role (read-only) API key in production settings and set it in the project root .env. ' +
        'Do NOT reuse SERVERBEE_PROD_API_KEY (which is admin-scoped for db-pull).'
      )
    }

    apiProxy = createDevProxy({ target, readonlyApiKey, allowWrites })
    devProxyTarget = target
  } else {
    apiProxy = {
      target: 'http://localhost:9527',
      changeOrigin: true,
      ws: true,
    }
  }

  return {
    plugins: [ /* unchanged */ ],
    resolve:  { /* unchanged */ },
    build:    { /* unchanged */ },
    define: devProxyTarget
      ? { 'import.meta.env.VITE_DEV_PROXY_TARGET': JSON.stringify(devProxyTarget) }
      : {},
    server: {
      proxy: {
        '/api': apiProxy,
      },
    },
  }
})
```

Notes:
- The non-prod-proxy branch is equivalent to the current config. Default
  `bun run dev` behavior is unchanged.
- `loadEnv` with an empty prefix reads every variable; we consume only
  the three we need (`SERVERBEE_PROD_URL`,
  `SERVERBEE_PROD_READONLY_API_KEY`, `ALLOW_WRITES`).
- The error messages name the exact variable **and** explicitly warn
  against reusing the db-pull admin key, so a developer who runs into the
  error cannot easily fall into the unsafe pattern.

### 3. `apps/web/vite/dev-proxy.test.ts` (new, vitest)

Because this change carries safety-critical configuration that is easy
to silently regress during refactors, the extracted module ships with
unit tests. They run under the existing `bun run test` harness in
`apps/web`.

Coverage (must all pass):

1. **Read-only enforcement, default**: call `createDevProxy({ allowWrites: false, ... })`, simulate a `POST /api/servers` on the `proxyReq` event via a mock `proxy.on` emitter. Expect `res.writeHead(403, ...)` and **no** `X-API-Key` injection and **no** `cookie` stripping attempt on the forwarded request (because the request was aborted).
2. **Read-only escape hatch**: same as above with `allowWrites: true`. Expect `X-API-Key` to be set, `cookie`/`authorization` removed, no 403.
3. **HTTP header stripping + injection on GET**: simulate a `GET /api/servers` with `Cookie: session_token=abc` and `Authorization: Bearer xyz`. Expect `proxyReq.removeHeader('cookie')`, `proxyReq.removeHeader('authorization')`, and `proxyReq.setHeader('X-API-Key', 'test-key')` in that order.
4. **Auth path block — login**: simulate `POST /api/auth/login`. Expect 403 with the auth-specific error message (verifies the auth-block fires *before* the write-block so the error message is more informative).
5. **Auth path block — OAuth callback**: simulate `GET /api/auth/oauth/github/callback`. Expect 403 even though the method is GET.
6. **Auth path allow-list — `GET /api/auth/me`**: simulate `GET /api/auth/me`. Expect headers stripped, `X-API-Key` injected, no 403.
7. **Auth path allow-list is method-scoped — `POST /api/auth/me`**: simulate `POST /api/auth/me` (a hypothetical future endpoint that does not exist today). Expect 403 with the auth-specific error message, confirming the allow-list checks BOTH path and method and does not drift open if the backend adds a write variant of `/me`.
8. **WebSocket upgrade header stripping + injection**: simulate the `proxyReqWs` event with the same cookie/authorization mock. Expect both removed and `X-API-Key` set.
9. **Set-Cookie stripping**: simulate the `proxyRes` event with `set-cookie: session_token=abc; Secure; HttpOnly`. Expect `proxyRes.headers['set-cookie']` to be absent afterwards.

Each test uses a tiny fake `proxy` object that records handler
registrations, then invokes the registered handlers directly with mock
`proxyReq` / `req` / `res` / `proxyRes` objects. No real HTTP traffic,
no real Vite, no real http-proxy. Test file should be under ~150 lines.

### 4. `apps/web/package.json` (modified)

Add one script:

```json
"dev:prod": "vite --mode prod-proxy"
```

### 5. `apps/web/src/components/dev-proxy-banner.tsx` (new)

A small, zero-dep React component:

- Renders only when `import.meta.env.MODE === 'prod-proxy'`.
- Shows text like `⚠ Dev proxy → PROD (https://xxx.up.railway.app) · read-only`.
- Persistent at the top of the viewport, high z-index, orange/red
  background, `position: fixed` so it overlays any layout.
- Pulls the URL from `import.meta.env.VITE_DEV_PROXY_TARGET` (injected
  by `define` in vite.config.ts).
- Accessible: contains `role="alert"`.

### 6. Mount point for the banner (modified)

Add `<DevProxyBanner />` to the TanStack root route at
`apps/web/src/routes/__root.tsx`. That file renders on every route
including unauthenticated ones (login, OAuth landing, 404), which
satisfies the requirement "every page shows the banner in prod-proxy
mode". Implementation should verify the exact root-route file path at
edit time (project uses TanStack Router file-based routing under
`apps/web/src/routes/`) — if the current tree has a wrapper above
`__root.tsx`, mount at the highest unconditional render level.

### 7. `scripts/make-menu.ts` and `Makefile` (both modified)

**Two files must be edited** — a previous iteration of this spec named
only one, which would have silently broken `make web-dev-prod`:

1. `scripts/make-menu.ts`: append a new menu entry so the menu runner
   knows what command to execute:

   ```ts
   {
     key: 'web-dev-prod',
     name: 'web:dev:prod',
     category: 'Frontend',
     description: 'Vite dev server with all /api requests proxied to production Railway (read-only)',
     command: 'cd apps/web && bun run dev:prod',
     featured: true
   }
   ```

2. `Makefile`: add `web-dev-prod` to the `COMMAND_TARGETS := \` list
   (currently near `Makefile:5`). The existing pattern rule at
   `Makefile:73` delegates to `$(MENU_RUNNER) run $@`, so adding the
   name to `COMMAND_TARGETS` is sufficient — no new rule is needed. The
   `.PHONY` declaration at `Makefile:63` already expands
   `$(COMMAND_TARGETS)`, so it picks up the new name automatically.

Without the Makefile edit, `make web-dev-prod` produces `make: *** No
rule to make target 'web-dev-prod'` even though the menu entry exists.

### 8. `.env.example` (modified)

Introduce the **new** `SERVERBEE_PROD_READONLY_API_KEY` variable and
clarify the existing `SERVERBEE_PROD_API_KEY` comment so the split is
unambiguous:

```bash
# Production URL — shared by db-pull and dev:prod frontend proxy
SERVERBEE_PROD_URL=https://your-app.up.railway.app

# Admin-scoped API key for scripts/db-pull.sh.
# The backup API requires admin role; this key MUST be admin.
# DO NOT reuse for dev:prod — it would defeat the read-only guard
# when ALLOW_WRITES=1 is set.
SERVERBEE_PROD_API_KEY=

# Member-role API key for `bun run dev:prod` / `make web-dev-prod`.
# MUST be a member-role key. Create one in production at
# Settings → API Keys, selecting role=member. Keeping this separate
# from SERVERBEE_PROD_API_KEY prevents admin-key bleed-through: even
# if a developer later sets ALLOW_WRITES=1 to override the proxy's
# method check, the backend still enforces the member role's
# permission surface. Note this is NOT zero writes — a handful of
# routes are accessible to member-role keys (e.g. mobile pairing,
# push registration, device deletion under
# crates/server/src/router/api/mobile.rs). Audit your key's
# permissions if you plan to use ALLOW_WRITES=1.
SERVERBEE_PROD_READONLY_API_KEY=
```

### 9. `AGENTS.md` (modified)

Add a short section "Debugging the frontend with production data" with
two options documented:

- `make db-pull && make server-dev-prod` — frozen snapshot, full local
  stack, good for backend work. Uses the admin-scoped
  `SERVERBEE_PROD_API_KEY`.
- `make web-dev-prod` — live production data through a read-only proxy,
  good for UI styling and realtime debugging. Uses the member-scoped
  `SERVERBEE_PROD_READONLY_API_KEY`. Session cookies and Bearer tokens
  are stripped at the proxy layer, so "logging in" from localhost
  cannot reach production. Default read-only; `ALLOW_WRITES=1 make
  web-dev-prod` enables writes (still gated by the member key's
  backend permissions).

## Security Guardrails (defense in depth)

| # | Layer                  | Mechanism                                                                         | Defeats                                                              |
| - | ---------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| 1 | Credential isolation   | Dedicated env var `SERVERBEE_PROD_READONLY_API_KEY`, required to be member-role   | Admin key bleed-through from `db-pull` setup                         |
| 2 | Auth isolation (net)   | Proxy strips `Cookie`/`Authorization` request headers and `Set-Cookie` response header; blocks every `/api/auth/*` request except exactly `GET /api/auth/me` | Session cookie auth path on the backend; OAuth callbacks; accidentally logging into production from localhost |
| 3 | Write interception (net) | Proxy returns `403` for non-`GET/HEAD/OPTIONS` methods unless `ALLOW_WRITES=1`  | Accidental clicks on delete/save buttons; stale form submits         |
| 4 | Human awareness        | Persistent UI banner on every route, red/orange, shows target URL                 | Forgetting the session is wired to production                        |

These layers are **independent** — no single mistake (even a deliberate
`ALLOW_WRITES=1` plus an admin key in the wrong variable) defeats more
than one layer.

The critical failure mode the previous iteration of this spec missed is:
`crates/server/src/middleware/auth.rs` checks session cookie **first**,
then API key. Without layer 2, a developer who logged into production
once from localhost would have a session cookie stored in the
`localhost:5173` origin, and every subsequent proxied request would
authenticate as an admin (via session) regardless of the injected
read-only `X-API-Key`. Layer 2 closes this by stripping `Cookie` before
the request leaves the dev machine, so the session-cookie path on the
backend is **unreachable**.

The `ALLOW_WRITES=1` escape hatch intentionally bypasses **only** layer
3, and has to be typed on the command line — a conscious act. It does
not touch layers 1, 2, or 4.

## Error Handling

- **Missing `SERVERBEE_PROD_URL`**: Vite fails at startup with a clear
  error pointing to the project root `.env`.
- **Missing `SERVERBEE_PROD_READONLY_API_KEY`**: Vite fails at startup
  with an error that explicitly names the variable AND warns against
  reusing `SERVERBEE_PROD_API_KEY` (preventing the unsafe workaround).
- **Production URL unreachable / DNS failure**: `http-proxy` emits an
  error event; the browser receives a 502. Normal proxy behavior, no
  special handling.
- **API key rejected by production (`401`)**: the response passes
  through; the frontend's existing unauth handler would normally
  redirect to login, but the login POST will itself be blocked at the
  proxy (auth-path block + write-method block), producing another 403
  with a clear error message. The banner stays visible and clarifies
  the situation. The developer should fix the key in `.env` and
  restart Vite.
- **Write attempted without `ALLOW_WRITES=1`**: frontend receives a
  synthetic 403 from the dev proxy with the JSON body explaining why.
  The existing `apiClient` error toast will display the message.
- **Auth path attempted (e.g. clicking "Sign in")**: proxy responds
  403 with the message "Auth paths are blocked in dev proxy to prevent
  production session leakage". This is expected behavior, not a bug.
- **Cookie/Authorization present on incoming request**: silently
  stripped by the proxy. No error. (Defense against stale state in the
  browser's localhost origin.)

## Testing Strategy

This change carries **safety-critical configuration** (read-only
enforcement, credential isolation, auth header stripping). Purely
manual testing is not sufficient because the proxy hooks are easy to
silently regress during future refactors of `vite.config.ts`. The
strategy is:

### Automated — unit tests on the extracted proxy module

`apps/web/vite/dev-proxy.test.ts` covers the nine cases listed in
component 3 above (read-only enforcement default, escape hatch, header
stripping + injection on GET, auth path block for login and OAuth
callback, auth path allow-list for `GET /api/auth/me`, method-scoping
of the allow-list via `POST /api/auth/me`, WebSocket upgrade,
Set-Cookie stripping). Tests run under the existing
`apps/web/package.json` vitest harness — `bun run test` already exists
— so CI automatically executes them with no new wiring.

These tests are **required to pass** before implementation is considered
complete. They exist specifically because the alternative (manual
verification of security guarantees after every config refactor) is
unsustainable.

### Manual — end-to-end verification checklist

Also add `tests/web/dev-proxy.md` (matching the existing
`tests/*/*.md` convention seen under the project's `tests/` directory;
the plan should verify the naming convention against existing files at
implementation time). Checklist:

1. **Default mode unaffected**: `bun run dev` boots, proxies to
   `localhost:9527`, no banner, default local-server flow works as
   before.
2. **Happy path**: `SERVERBEE_PROD_URL=<url>
   SERVERBEE_PROD_READONLY_API_KEY=<member_key> bun run dev:prod` →
   app boots, banner visible, server list populates with production
   data, live chart ticks, WebSocket connection indicator shows
   "connected", realtime `ServerUpdate` pushes are visible as the
   charts animate.
3. **Write block**: in prod-proxy mode, attempt to delete a server or
   save a setting. Expected: browser console shows 403 with the
   "Dev proxy is read-only" message, no change on production.
4. **Escape hatch**: `ALLOW_WRITES=1 bun run dev:prod` + retry a
   harmless write (e.g. toggle a hidden server's remark). Expected:
   request reaches production and succeeds (if the member key has the
   required permission) or returns a backend 403 (if it does not).
5. **Auth block**: in prod-proxy mode, navigate to `/login` and click
   "Sign in". Expected: 403 with "Auth paths are blocked" message,
   no Set-Cookie in response tab.
6. **Missing env vars**: `bun run dev:prod` without `.env` or with
   only `SERVERBEE_PROD_URL` set → Vite refuses to start, error
   message names `SERVERBEE_PROD_READONLY_API_KEY` and warns against
   reusing `SERVERBEE_PROD_API_KEY`.
7. **Separation check**: set `SERVERBEE_PROD_READONLY_API_KEY=$SERVERBEE_PROD_API_KEY`
   (admin key in the wrong slot). Confirm layer 3 still blocks writes
   (layer 1 is advisory; layer 3 is the enforcement). Then set
   `ALLOW_WRITES=1` — write now reaches prod. **Document this as the
   known-unsafe configuration the spec is designed to make deliberate.**
8. **Visual**: banner is legible on both light and dark themes, sticky
   on scroll, does not break login form layout. Screenshot before /
   after.

Tests 1, 2, 3, 5, and 6 must pass. Test 4 is optional (requires a
harmless writable row). Test 7 documents the intended failure mode.
Test 8 is visual QA.

## Decision Record

| Decision                         | Choice                                                |
| -------------------------------- | ----------------------------------------------------- |
| Trigger mechanism                | Vite `--mode prod-proxy` (variant 1 of the three discussed) |
| Script name                      | `bun run dev:prod` + `make web-dev-prod` (requires both `scripts/make-menu.ts` and `Makefile` updates) |
| Write interception               | Enabled by default; `ALLOW_WRITES=1` to override      |
| UI banner                        | Yes, on every page, mounted at `apps/web/src/routes/__root.tsx` |
| API key role enforcement         | Dedicated env var + documentation; backend is the ultimate authority via the member role |
| Env var strategy                 | **Split**: `SERVERBEE_PROD_READONLY_API_KEY` (new, member) for proxy; `SERVERBEE_PROD_API_KEY` (unchanged, admin) for db-pull |
| Auth isolation                   | Strip `Cookie` / `Authorization` from requests; strip `Set-Cookie` from responses; block every `/api/auth/*` except exactly `GET /api/auth/me` (method + path must both match) |
| Proxy logic location             | Extracted to `apps/web/vite/dev-proxy.ts` for unit testability |
| Automated testing                | Required — `apps/web/vite/dev-proxy.test.ts` (vitest) covering all nine listed cases |
| `.env.example` + `AGENTS.md`     | Both updated                                          |

## Out of scope / Future work

- Caching production responses locally for offline dev — not needed for the
  stated goal.
- Multi-target switching (`dev:staging`, `dev:prod-eu`) — trivial extension
  if ever needed; add another mode and another pair of env vars.
- Automated E2E test of the proxy itself — unlikely to pay for itself given
  the size of the change.
- Replacing the existing `db-pull` flow — `db-pull` remains the right tool
  when the developer needs a full local stack (e.g. backend work, offline
  iteration, destructive schema changes). The two workflows are
  complementary.
