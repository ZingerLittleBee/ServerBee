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
`bun run dev:prod` (and a matching `make web-dev-prod` menu entry). In this
mode, `apps/web/vite.config.ts` rewrites its `server.proxy['/api']` entry so
that:

1. `target` becomes the value of `SERVERBEE_PROD_URL` loaded from the
   project root `.env` file.
2. The proxy's `configure` hook attaches listeners to the underlying
   `http-proxy` instance:
   - On `proxyReq` and `proxyReqWs`, inject the header
     `X-API-Key: <SERVERBEE_PROD_API_KEY>`.
   - On incoming request, if the HTTP method is not one of
     `GET`/`HEAD`/`OPTIONS` and the `ALLOW_WRITES` env var is not set,
     short-circuit with HTTP `403` and a JSON body; the request is never
     forwarded to production.
3. A compile-time constant `__DEV_PROXY_TARGET__` (or equivalent
   `import.meta.env.VITE_DEV_PROXY_TARGET`) is injected via Vite `define`
   so the frontend can render a banner showing which production URL the
   session is pinned to.

The browser-side WebSocket client (`apps/web/src/lib/ws-client.ts`)
constructs its URL from `window.location.host`, so all WebSocket traffic
naturally flows through the Vite dev server and is handed to `http-proxy`
via the existing `ws: true` setting. The `/api/ws/servers` endpoint on the
Rust server already supports `X-API-Key` authentication
(`crates/server/src/router/ws/browser.rs:54-59`), so header injection on the
upgrade request is sufficient.

Reuse of existing environment variables is deliberate:

- `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_API_KEY` are already declared in
  `.env.example` for the `db-pull` script. No new variables are introduced.
- `.env` lives in the project root; the existing `scripts/db-pull.sh` loads
  it from there. `vite.config.ts` will do the same using Vite's
  `loadEnv(mode, rootDir, '')`.

## Data Flow

```
Browser (http://localhost:5173)
   │
   │ fetch('/api/servers')         window.fetch / apiClient
   ▼
Vite Dev Server (middlewares)
   │
   │ http-proxy configure hook
   │   ├─ if method ∉ {GET, HEAD, OPTIONS} and !ALLOW_WRITES
   │   │     → respond 403 { error: "Dev proxy is read-only" }, STOP
   │   └─ else inject X-API-Key header
   ▼
Railway Production (https://xxx.up.railway.app)
   │
   │ middleware/auth.rs → validate_api_key
   │ sea-orm → SQLite
   │
   ▼
Response → Vite → Browser render

-------- WebSocket path --------

Browser new WebSocket('ws://localhost:5173/api/ws/servers')
   │
   ▼
Vite Dev Server (http-proxy WebSocket upgrade)
   │
   │ proxyReqWs hook → inject X-API-Key
   ▼
Railway WebSocket Upgrade → router/ws/browser.rs
   │
   │ validate_browser_auth() — X-API-Key path
   │ subscribe to broadcast::Sender<BrowserMessage>
   ▼
Live ServerUpdate / ServerOnline / ServerOffline / CapabilitiesChanged
messages stream back through the proxy to the browser in real time.
```

## Components

Scaled to the small size of this change:

### 1. `apps/web/vite.config.ts` (modified)

Convert from object form to function form so the config can react to
`mode`:

```ts
export default defineConfig(({ mode }) => {
  const isProdProxy = mode === 'prod-proxy'
  const rootEnv = loadEnv(mode, path.resolve(import.meta.dirname, '../..'), '')
  const prodUrl = rootEnv.SERVERBEE_PROD_URL
  const prodApiKey = rootEnv.SERVERBEE_PROD_API_KEY
  const allowWrites = rootEnv.ALLOW_WRITES === '1'

  if (isProdProxy) {
    if (!prodUrl) throw new Error('SERVERBEE_PROD_URL is required for dev:prod; set it in the project root .env')
    if (!prodApiKey) throw new Error('SERVERBEE_PROD_API_KEY is required for dev:prod; set it in the project root .env')
  }

  const target = isProdProxy ? prodUrl : 'http://localhost:9527'

  return {
    plugins: [ /* unchanged */ ],
    resolve:  { /* unchanged */ },
    build:    { /* unchanged */ },
    define: isProdProxy
      ? { 'import.meta.env.VITE_DEV_PROXY_TARGET': JSON.stringify(prodUrl) }
      : {},
    server: {
      proxy: {
        '/api': {
          target,
          changeOrigin: true,
          ws: true,
          configure: isProdProxy
            ? (proxy) => {
                // Block writes by default
                proxy.on('proxyReq', (proxyReq, req, res) => {
                  const method = (req.method || 'GET').toUpperCase()
                  const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
                  if (!isReadOnly && !allowWrites) {
                    res.writeHead(403, { 'content-type': 'application/json' })
                    res.end(JSON.stringify({
                      error: 'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.'
                    }))
                    // Abort the upstream request so it never hits production
                    proxyReq.destroy()
                    return
                  }
                  proxyReq.setHeader('X-API-Key', prodApiKey)
                })
                // WebSocket upgrade header injection
                proxy.on('proxyReqWs', (proxyReq) => {
                  proxyReq.setHeader('X-API-Key', prodApiKey)
                })
              }
            : undefined,
        },
      },
    },
  }
})
```

Notes:
- The non-prod-proxy branch is equivalent to the current config. Default
  `bun run dev` behavior is unchanged.
- `loadEnv` with an empty prefix reads every variable; we consume only the
  three we need.
- Using `res.writeHead(403).end(...)` and then `proxyReq.destroy()` is the
  canonical pattern to short-circuit http-proxy before bytes are flushed
  upstream. Verify the exact API shape at implementation time against the
  http-proxy version bundled with the installed Vite.

### 2. `apps/web/package.json` (modified)

Add one script:

```json
"dev:prod": "vite --mode prod-proxy"
```

### 3. `apps/web/src/components/dev-proxy-banner.tsx` (new)

A small, zero-dep React component:

- Renders only when `import.meta.env.MODE === 'prod-proxy'`.
- Shows text like `⚠ Dev proxy → PROD (https://xxx.up.railway.app) · read-only`.
- Persistent at the top of the viewport, high z-index, orange/red
  background, `position: fixed` so it overlays any layout.
- Pulls the URL from `import.meta.env.VITE_DEV_PROXY_TARGET` (injected by
  `define` in vite.config.ts).
- Accessible: contains `role="alert"`.

### 4. Mount point for the banner (modified)

Add `<DevProxyBanner />` to the application's root layout so it is present
on every route, including the login page. The exact mount file
(`apps/web/src/main.tsx`, the TanStack root route, or a layout wrapper)
will be decided at implementation time — requirement is "every page shows
it when in prod-proxy mode, including unauthenticated routes".

### 5. `scripts/make-menu.ts` (modified)

Append a new entry so `make web-dev-prod` becomes available and appears in
`make` menu listings:

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

### 6. `.env.example` (modified)

Add a comment clarifying that `SERVERBEE_PROD_URL` and
`SERVERBEE_PROD_API_KEY` are also consumed by `bun run dev:prod` / `make
web-dev-prod`, and strongly recommend using a **member-role** API key for
only-reading production. Keep the existing variables unchanged.

### 7. `AGENTS.md` (modified)

Add a short section "Debugging the frontend with production data" with two
options documented:

- `make db-pull && make server-dev-prod` — frozen snapshot, full local
  stack, good for backend work.
- `make web-dev-prod` — live production data through a read-only proxy,
  good for UI styling and realtime debugging.

## Security Guardrails (defense in depth)

| Layer           | Mechanism                                             | Intent                                        |
| --------------- | ----------------------------------------------------- | --------------------------------------------- |
| Credential      | Documented recommendation: use member-role API key    | Even if a write somehow slips through, server rejects |
| Proxy (network) | 403 short-circuit on non-read methods                 | Writes never leave the dev machine            |
| Human           | Persistent UI banner on every page                    | No forgetting that the session is wired to prod |

All three layers are independent; no single mistake can defeat all of them.

The proxy layer is the primary guard because it is enforced by the
developer's own machine and cannot be bypassed by an admin-role API key
accidentally reused from the `db-pull` setup. The `ALLOW_WRITES=1` escape
hatch exists for the rare case of deliberately writing to production (e.g.
toggling a server's remark) — it has to be typed on the command line, which
is a conscious act.

## Error Handling

- **Missing `SERVERBEE_PROD_URL`**: Vite fails at startup with a clear
  error pointing to the project root `.env`.
- **Missing `SERVERBEE_PROD_API_KEY`**: same.
- **Production URL unreachable / DNS failure**: `http-proxy` emits an error
  event; the browser receives a 502. Normal proxy behavior, no special
  handling.
- **API key rejected by production (`401`)**: the response passes through;
  the frontend's existing unauth handler redirects to login. The banner
  stays visible and clarifies the situation.
- **Write attempted without `ALLOW_WRITES=1`**: frontend receives a
  synthetic 403 from the dev proxy with the JSON body explaining why. The
  existing `apiClient` error toast will display the message.

## Testing Strategy

This change is small and entirely within the frontend dev tooling layer.
Automated tests are not proportionate.

Manual verification checklist:

1. `bun run dev` (default mode): app boots, proxies to `localhost:9527` as
   before, no banner.
2. `SERVERBEE_PROD_URL=<url> SERVERBEE_PROD_API_KEY=<member_key> bun run
   dev:prod`: app boots, banner visible, server list populates with
   production data, live chart ticks, WebSocket connection indicator shows
   "connected".
3. In prod-proxy mode, attempt to delete a server or save a setting.
   Expected: frontend receives 403, no change on production.
4. In prod-proxy mode, add `ALLOW_WRITES=1` env var and retry a write.
   Expected: request reaches production (use a harmless write to verify,
   e.g. toggle an already-hidden server).
5. `bun run dev:prod` without `.env` present or with missing variables:
   Vite refuses to start, error message names the missing variable and
   points at the project root `.env`.
6. Visual check: banner is legible on both light and dark themes, sticky on
   scroll, does not overlap login form inputs beyond a small acceptable
   margin.

This checklist should be added to `tests/` as a new E2E file
(`tests/web/dev-proxy.md`) to match the existing manual-test convention.

## Decision Record

| Decision                         | Choice                                                |
| -------------------------------- | ----------------------------------------------------- |
| Trigger mechanism                | Vite `--mode prod-proxy` (variant 1 of the three discussed) |
| Script name                      | `dev:prod` (short) + `make web-dev-prod`              |
| Write interception               | Enabled by default; `ALLOW_WRITES=1` to override      |
| UI banner                        | Yes, on every page                                    |
| API key role check               | Documentation only (no runtime check)                 |
| Env var reuse                    | Reuse `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_API_KEY` — no new variables |
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
