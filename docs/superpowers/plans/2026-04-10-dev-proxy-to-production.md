# Dev Proxy to Production Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `bun run dev:prod` / `make web-dev-prod` mode that makes the local Vite dev server transparently forward all `/api/*` traffic (HTTP + WebSocket) to the production Railway backend, with read-only enforcement, auth-header isolation, and a persistent UI banner, so the developer can iterate on frontend styles against live production data.

**Architecture:** Vite `--mode prod-proxy` triggers a function-form `vite.config.ts` that delegates `/api` proxy logic to a pure factory in `apps/web/vite/dev-proxy.ts`. The factory blocks non-read HTTP methods, blocks `/api/auth/*` (except exactly `GET /api/auth/me`), strips `Cookie` / `Authorization` from requests and `Set-Cookie` from responses, and injects a member-role `X-API-Key`. The factory takes already-validated inputs, so unit tests drive it via mock proxy events without spinning up Vite. Zero Rust / DB changes.

**Tech Stack:** TypeScript, Vite 7.2, Vitest 4.1, TanStack Router (`createRootRoute`), Node `http-proxy` (bundled with Vite).

**Spec:** [`docs/superpowers/specs/2026-04-10-dev-proxy-to-production-design.md`](../specs/2026-04-10-dev-proxy-to-production-design.md)

---

## File Map

| Path | Role |
|---|---|
| `apps/web/vite/dev-proxy.ts` (new) | Pure factory `createDevProxy(opts)` returning a Vite `ProxyOptions`. All safety logic (write block, auth path block, header strip, API-key inject, Set-Cookie strip) lives here so it can be unit tested in isolation. |
| `apps/web/vite/dev-proxy.test.ts` (new) | Vitest suite with 11 cases covering every branch of the factory. Uses a minimal fake proxy emitter — no real HTTP, no real Vite. |
| `apps/web/vite.config.ts` (modify) | Convert to function form (`defineConfig(({ mode }) => …)`). Load `.env` from project root via `loadEnv`, validate required env vars, delegate `/api` proxy to `createDevProxy`. Inject `import.meta.env.VITE_DEV_PROXY_TARGET` via `define` for the banner. |
| `apps/web/tsconfig.node.json` (modify) | Extend `include` from `["vite.config.ts"]` to `["vite.config.ts", "vite/**/*.ts"]` so the new proxy module is part of the Node-side TypeScript project. |
| `apps/web/package.json` (modify) | Add `"dev:prod": "vite --mode prod-proxy"` to `scripts`. |
| `apps/web/src/components/dev-proxy-banner.tsx` (new) | Small React component that renders a fixed-position orange warning banner when `import.meta.env.MODE === 'prod-proxy'`. Reads target URL from `import.meta.env.VITE_DEV_PROXY_TARGET`. |
| `apps/web/src/routes/__root.tsx` (modify) | Mount `<DevProxyBanner />` inside the top-level fragment, before `<ThemeProvider>`, so it overlays every route including unauthenticated ones. |
| `scripts/make-menu.ts` (modify) | Append a `web-dev-prod` entry under the `Web` category so `bun scripts/make-menu.ts run web-dev-prod` resolves to `bun --filter @serverbee/web dev:prod`. |
| `Makefile` (modify) | Add `web-dev-prod` to the `COMMAND_TARGETS := \` list so `make web-dev-prod` isn't rejected by the whitelist. |
| `.env.example` (modify) | Add new required var `SERVERBEE_PROD_READONLY_API_KEY` with a comment block that points to `mobile.rs` for audit scope and explicitly warns against reusing the db-pull admin key. |
| `AGENTS.md` (modify) | Add "Debugging the frontend with production data" section documenting the two workflows (`make db-pull` vs `make web-dev-prod`) and their credential requirements. |
| `tests/web/dev-proxy.md` (new) | Manual E2E verification checklist matching the existing `tests/` convention. |

## Key Reference Points (verified during planning)

- Current `apps/web/vite.config.ts`: object form, proxies `/api` to `http://localhost:9527` with `ws: true`. 65 lines.
- Current `apps/web/src/routes/__root.tsx`: uses a fragment wrapper with `{import.meta.env.DEV && <Agentation />}` then `<ThemeProvider>`. Banner goes in the same fragment.
- `apps/web/tsconfig.node.json` include is currently `["vite.config.ts"]` only — **this is why we need a tsconfig edit**.
- `apps/web/vitest.config.ts`: `environment: 'jsdom'`, `globals: true`, `setupFiles: ['./src/test/setup.ts']`.
- Existing test convention (`apps/web/src/lib/ws-client.test.ts:1`): tests **still** explicitly `import { describe, it, expect, vi } from 'vitest'` despite `globals: true`.
- `scripts/make-menu.ts`: Web-category commands use `bun --filter @serverbee/web <script>`, not `cd apps/web && bun run <script>`.
- `Makefile`: targets are controlled by `COMMAND_TARGETS := \` list near line 5; a pattern rule at line 73 delegates to `$(MENU_RUNNER) run $@`.
- `.env.example` currently has 6 lines declaring only `SERVERBEE_PROD_URL` and `SERVERBEE_PROD_API_KEY` for db-pull.
- `crates/server/src/router/ws/browser.rs:54-59` accepts `X-API-Key` header for WebSocket auth (verified in spec review round 2).
- `crates/server/src/router/api/auth.rs:92` confirms `/api/auth/me` endpoint exists (verified in spec review round 2).
- `crates/server/src/router/api/mod.rs:47-48` shows `auth::protected_router` and `mobile::protected_router` are OUTSIDE `require_admin`, meaning member-role keys can write to mobile endpoints — the `.env.example` comment must acknowledge this (verified in spec review round 3).

---

## Task 1: Scaffold dev-proxy module + test harness

**Files:**
- Create: `apps/web/vite/dev-proxy.ts`
- Create: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/tsconfig.node.json`

**Context:** Stand up the file skeleton with correct types, get a smoke test running, and update tsconfig so TypeScript recognizes the new files. All subsequent TDD tasks mutate these files.

- [ ] **Step 1: Create the module file with types and a placeholder factory**

Create `apps/web/vite/dev-proxy.ts`:

```ts
import type { ProxyOptions } from 'vite'

export interface DevProxyOptions {
  /** Fully-qualified production URL, e.g. https://xxx.up.railway.app */
  target: string
  /** Member-role API key (validated by caller) */
  readonlyApiKey: string
  /** True only when ALLOW_WRITES=1 is set; unlocks non-read HTTP methods */
  allowWrites: boolean
}

/**
 * Returns the `/api` ProxyOptions entry for Vite when running in
 * dev-prod mode. Pure factory: no env reads, no side effects, no I/O.
 * All env validation belongs to the caller (vite.config.ts).
 */
export function createDevProxy(opts: DevProxyOptions): ProxyOptions {
  return {
    target: opts.target,
    changeOrigin: true,
    ws: true,
    configure: (_proxy) => {
      // Implementations added in Tasks 2–7.
    },
  }
}
```

- [ ] **Step 2: Extend `apps/web/tsconfig.node.json` include**

Modify `apps/web/tsconfig.node.json` line 25 from:

```json
"include": ["vite.config.ts"]
```

to:

```json
"include": ["vite.config.ts", "vite/**/*.ts"]
```

This brings `apps/web/vite/dev-proxy.ts` and `apps/web/vite/dev-proxy.test.ts` into the Node-side TypeScript project so `tsc --noEmit` can check them. They are intentionally separated from `tsconfig.app.json` because they do not use DOM / React types.

- [ ] **Step 3: Create the test file with a mock proxy helper and a smoke test**

Create `apps/web/vite/dev-proxy.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest'
import { createDevProxy, type DevProxyOptions } from './dev-proxy'

/**
 * Minimal fake of http-proxy's event emitter that captures registered
 * handlers and lets us invoke them with mock req/res/proxyReq objects.
 */
function makeMockProxy() {
  const handlers: Record<string, Array<(...args: unknown[]) => void>> = {}
  return {
    on(event: string, handler: (...args: unknown[]) => void) {
      ;(handlers[event] ??= []).push(handler)
    },
    emit(event: string, ...args: unknown[]) {
      for (const h of handlers[event] ?? []) h(...args)
    },
  }
}

function makeMockProxyReq() {
  return {
    setHeader: vi.fn(),
    removeHeader: vi.fn(),
    destroy: vi.fn(),
  }
}

function makeMockReq(method: string, url: string, headers: Record<string, string> = {}) {
  return { method, url, headers }
}

function makeMockRes() {
  return {
    writeHead: vi.fn(),
    end: vi.fn(),
  }
}

const baseOpts: DevProxyOptions = {
  target: 'https://prod.example.com',
  readonlyApiKey: 'serverbee_test_member',
  allowWrites: false,
}

describe('createDevProxy', () => {
  it('returns a ProxyOptions object with target, ws, and configure', () => {
    const result = createDevProxy(baseOpts)
    expect(result.target).toBe('https://prod.example.com')
    expect(result.ws).toBe(true)
    expect(result.changeOrigin).toBe(true)
    expect(typeof result.configure).toBe('function')
  })
})
```

- [ ] **Step 4: Run the smoke test and confirm it passes**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 1 test passes.

- [ ] **Step 5: Run typecheck to confirm tsconfig change works**

Run: `bun --filter @serverbee/web typecheck`

Expected: passes (0 errors). If it reports "file is not listed within the file list of project", re-check the tsconfig.node.json `include` edit.

- [ ] **Step 6: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts apps/web/tsconfig.node.json
git commit -m "feat(web): scaffold dev-proxy factory module and test harness"
```

---

## Task 2: TDD write-method block

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** The first real behavior — non-read HTTP methods (POST/PUT/PATCH/DELETE) must be blocked with 403 unless `allowWrites` is true. This is spec test cases 1 and 2.

- [ ] **Step 1: Write failing tests**

Append to `apps/web/vite/dev-proxy.test.ts` inside the existing `describe('createDevProxy', …)` block:

```ts
describe('write-method block', () => {
  it('blocks POST with 403 when allowWrites is false (default)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('POST', '/api/servers')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).toHaveBeenCalledWith(
      403,
      expect.objectContaining({ 'content-type': 'application/json' })
    )
    expect(res.end).toHaveBeenCalledWith(
      expect.stringContaining('read-only')
    )
    expect(proxyReq.destroy).toHaveBeenCalled()
    expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
  })

  it('allows POST when allowWrites is true (escape hatch)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy({ ...baseOpts, allowWrites: true })
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('POST', '/api/servers')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).not.toHaveBeenCalled()
    expect(proxyReq.destroy).not.toHaveBeenCalled()
    expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
  })
})
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 2 failures in "write-method block". Smoke test still passes.

- [ ] **Step 3: Implement the write-method block**

Replace the body of `configure` in `apps/web/vite/dev-proxy.ts` so it reads:

```ts
    configure: (proxy) => {
      proxy.on('proxyReq', (proxyReq, req, res) => {
        const method = (req.method ?? 'GET').toUpperCase()

        // Block writes by default
        const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
        if (!isReadOnly && !opts.allowWrites) {
          respond403(res, proxyReq,
            'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.')
          return
        }

        // Inject the ONLY credential the backend will see
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })
    },
```

Also add the helper at the bottom of the file (outside `createDevProxy`):

```ts
function respond403(res: any, proxyReq: any, message: string) {
  res.writeHead(403, { 'content-type': 'application/json' })
  res.end(JSON.stringify({ error: message }))
  if (typeof proxyReq.destroy === 'function') proxyReq.destroy()
  else if (typeof proxyReq.abort === 'function') proxyReq.abort()
}
```

Note: `proxyReq.destroy()` is the http-proxy-3 API (bundled with Vite 7). The `abort()` fallback covers older versions. When running tests in Task 2 Step 4, if assertion "expect(proxyReq.destroy).toHaveBeenCalled()" fails, verify http-proxy version used by installed Vite via `bun pm ls vite` and simplify accordingly.

- [ ] **Step 4: Run tests and verify they pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 3 tests pass (smoke + 2 new).

- [ ] **Step 5: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): block non-read HTTP methods in dev proxy"
```

---

## Task 3: TDD HTTP header strip + X-API-Key injection

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** For allowed requests, strip `Cookie` and `Authorization` headers so the backend's session and Bearer auth paths are unreachable, then inject `X-API-Key`. This is spec test case 3 — and closes the highest-severity finding from the spec review (session cookie bypass).

- [ ] **Step 1: Write failing test**

Append inside the `describe('createDevProxy', …)` block:

```ts
describe('header stripping and X-API-Key injection', () => {
  it('strips Cookie and Authorization, then injects X-API-Key on GET', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/servers', {
      cookie: 'session_token=leaked',
      authorization: 'Bearer leaked',
    })
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
    expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
    expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
    expect(res.writeHead).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run tests and verify the new test fails**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 1 failure — `removeHeader` not called. Existing tests still pass.

- [ ] **Step 3: Implement header stripping**

In `apps/web/vite/dev-proxy.ts`, update the `proxy.on('proxyReq', …)` handler so that the post-block branch strips headers before injecting the key:

```ts
      proxy.on('proxyReq', (proxyReq, req, res) => {
        const method = (req.method ?? 'GET').toUpperCase()

        const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
        if (!isReadOnly && !opts.allowWrites) {
          respond403(res, proxyReq,
            'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.')
          return
        }

        // Strip auth request headers (defeats session/Bearer auth on the backend)
        proxyReq.removeHeader('cookie')
        proxyReq.removeHeader('authorization')

        // Inject the ONLY credential the backend will see
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })
```

- [ ] **Step 4: Run tests and verify all pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 4 tests pass.

- [ ] **Step 5: Verify the escape-hatch test in Task 2 also now strips headers**

Rationale: the escape-hatch test uses a bare req without cookie/authorization, so it passes vacuously. Add an assertion to that test so a future refactor that moves the strip inside the block branch is caught. Modify the second test in the "write-method block" describe to also assert:

```ts
    expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
    expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
```

Re-run tests:

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: still 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): strip Cookie/Authorization and inject X-API-Key in dev proxy"
```

---

## Task 4: TDD auth-path block (login + OAuth callback)

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** Block every `/api/auth/*` request. This defeats OAuth callbacks (which are GET so the write-method block does not catch them) and gives login POSTs a more informative error message. This is spec test cases 4 and 5.

- [ ] **Step 1: Write failing tests**

Append inside the `describe('createDevProxy', …)` block:

```ts
describe('auth path block', () => {
  it('blocks POST /api/auth/login with auth-specific error message', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('POST', '/api/auth/login')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
    const body = (res.end as ReturnType<typeof vi.fn>).mock.calls[0][0] as string
    expect(body).toContain('Auth paths')
    expect(body).not.toContain('read-only')  // Auth-path check MUST fire before write-method check
    expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
  })

  it('blocks GET /api/auth/oauth/github/callback (read method, still blocked)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/auth/oauth/github/callback')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
    expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
  })
})
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 2 failures in "auth path block" (today's proxy either lets them pass through or catches the POST with the read-only message, not the auth-specific one).

- [ ] **Step 3: Implement the auth-path block**

Update the `proxy.on('proxyReq', …)` handler so auth-path is the **first** check:

```ts
      proxy.on('proxyReq', (proxyReq, req, res) => {
        const url = req.url ?? ''
        const pathname = url.split('?')[0]   // strip query string before matching
        const method = (req.method ?? 'GET').toUpperCase()

        // 1. Block auth paths. Allow-list is EXACTLY `GET /api/auth/me`
        //    (wired up in Task 5). Both path and method must match.
        if (pathname.startsWith('/api/auth/')) {
          respond403(res, proxyReq,
            'Auth paths are blocked in dev proxy to prevent production session leakage')
          return
        }

        // 2. Block writes by default
        const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
        if (!isReadOnly && !opts.allowWrites) {
          respond403(res, proxyReq,
            'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.')
          return
        }

        // 3. Strip auth request headers
        proxyReq.removeHeader('cookie')
        proxyReq.removeHeader('authorization')

        // 4. Inject X-API-Key
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })
```

Note: the allow-list for `GET /api/auth/me` is added in Task 5, so for now ALL auth paths are blocked, including `/api/auth/me`. Tests 6–9 in Task 5 will punch a hole in this block.

- [ ] **Step 4: Run tests and verify they pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): block /api/auth/* paths in dev proxy"
```

---

## Task 5: TDD auth-path allow-list (`GET /api/auth/me`) with query/method/prefix safety

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** The UI needs `GET /api/auth/me` to render the current user. Allow it through, but tightly — must be GET, must be an exact pathname match, and must tolerate query strings. This is spec test cases 6, 7, 8, 9. The combination of "exact path + method" + "query-string tolerant" is what prevents future drift (a hypothetical `POST /api/auth/me` would be blocked) and prevents prefix attacks (`/api/auth/me/evil` is blocked).

- [ ] **Step 1: Write failing tests**

Append inside the `describe('createDevProxy', …)` block:

```ts
describe('auth path allow-list for GET /api/auth/me', () => {
  it('allows GET /api/auth/me through with headers stripped and key injected', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/auth/me', {
      cookie: 'session_token=leaked',
    })
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).not.toHaveBeenCalled()
    expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
    expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
  })

  it('allows GET /api/auth/me?_t=123 (query string ignored in matching)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/auth/me?_t=123')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).not.toHaveBeenCalled()
    expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
  })

  it('blocks POST /api/auth/me (method-scoped allow-list)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('POST', '/api/auth/me')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
    const body = (res.end as ReturnType<typeof vi.fn>).mock.calls[0][0] as string
    expect(body).toContain('Auth paths')
    expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
  })

  it('blocks GET /api/auth/me/evil (exact-match, not prefix)', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/auth/me/evil')
    const res = makeMockRes()
    proxy.emit('proxyReq', proxyReq, req, res)

    expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
    expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
  })
})
```

- [ ] **Step 2: Run tests and verify the first two fail, last two pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 2 failures — the two "allows" tests, because Task 4 currently blocks ALL `/api/auth/*`. The two "blocks" tests already pass because the prefix block is in place.

- [ ] **Step 3: Implement the allow-list**

Update the auth-path branch in `proxy.on('proxyReq', …)`:

```ts
        if (pathname.startsWith('/api/auth/')) {
          const isAllowedAuthRead =
            method === 'GET' && pathname === '/api/auth/me'
          if (!isAllowedAuthRead) {
            respond403(res, proxyReq,
              'Auth paths are blocked in dev proxy to prevent production session leakage')
            return
          }
          // Falls through to the strip/inject block below.
        }
```

- [ ] **Step 4: Run tests and verify all pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 10 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): allow GET /api/auth/me through dev proxy allow-list"
```

---

## Task 6: TDD WebSocket upgrade header strip + injection

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** WebSocket upgrade requests must also have `Cookie`/`Authorization` stripped and `X-API-Key` injected. The browser's `WebSocket` constructor automatically attaches cookies from the same origin, and the Vite dev server is on `localhost:5173`, so even stale localhost cookies must be stripped before the upgrade hits production. This is spec test case 10. The event is `proxyReqWs`, not `proxyReq`.

- [ ] **Step 1: Write failing test**

Append inside the `describe('createDevProxy', …)` block:

```ts
describe('WebSocket upgrade', () => {
  it('strips Cookie/Authorization and injects X-API-Key on proxyReqWs', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyReq = makeMockProxyReq()
    const req = makeMockReq('GET', '/api/ws/servers', {
      cookie: 'session_token=leaked',
      authorization: 'Bearer leaked',
    })
    // proxyReqWs has a different signature: (proxyReq, req, socket, options, head)
    proxy.emit('proxyReqWs', proxyReq, req, {}, {}, Buffer.alloc(0))

    expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
    expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
    expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
  })
})
```

- [ ] **Step 2: Run tests and verify this one fails**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 1 failure — no handler registered for `proxyReqWs` yet.

- [ ] **Step 3: Implement the WebSocket hook**

In `apps/web/vite/dev-proxy.ts`, add a second `proxy.on` inside `configure`, after the existing `proxy.on('proxyReq', …)`:

```ts
      // WebSocket upgrade: same stripping + injection.
      // Browser WebSocket API cannot set custom headers, so the proxy
      // is the only layer that can enforce this on the upgrade request.
      proxy.on('proxyReqWs', (proxyReq) => {
        proxyReq.removeHeader('cookie')
        proxyReq.removeHeader('authorization')
        proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
      })
```

Note: We intentionally do NOT apply the auth-path block or write-method block to `proxyReqWs`. WebSocket upgrades are always `GET`, so the write-method block would be a no-op. The auth-path block could theoretically apply to `GET /api/auth/*` WebSocket paths, but the backend has no such endpoints today and adding speculative blocks muddies the logic. The frontend's only WS path is `/api/ws/servers`, which is unambiguous.

- [ ] **Step 4: Run tests and verify all pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 11 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): strip headers and inject X-API-Key on WebSocket upgrade"
```

---

## Task 7: TDD Set-Cookie response stripping

**Files:**
- Modify: `apps/web/vite/dev-proxy.test.ts`
- Modify: `apps/web/vite/dev-proxy.ts`

**Context:** Strip `Set-Cookie` from all responses before they reach the browser. This closes the final leak — even if some response somehow includes a session cookie, the browser's `localhost:5173` origin never saves it. Belt-and-braces behind the request-side cookie strip. This is spec test case 11.

- [ ] **Step 1: Write failing test**

Append inside the `describe('createDevProxy', …)` block:

```ts
describe('Set-Cookie response stripping', () => {
  it('removes Set-Cookie from proxyRes headers', () => {
    const proxy = makeMockProxy()
    const options = createDevProxy(baseOpts)
    options.configure?.(proxy as never, {} as never)

    const proxyRes = {
      headers: {
        'content-type': 'application/json',
        'set-cookie': ['session_token=abc; Secure; HttpOnly'],
      } as Record<string, unknown>,
    }
    proxy.emit('proxyRes', proxyRes, {}, {})

    expect(proxyRes.headers['set-cookie']).toBeUndefined()
    expect(proxyRes.headers['content-type']).toBe('application/json')  // other headers untouched
  })
})
```

- [ ] **Step 2: Run tests and verify this one fails**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 1 failure — no handler registered for `proxyRes`.

- [ ] **Step 3: Implement the response hook**

In `apps/web/vite/dev-proxy.ts`, add a third `proxy.on` inside `configure`, after the WebSocket hook:

```ts
      // Strip Set-Cookie from all responses so the browser never
      // accumulates a production session cookie on its localhost origin.
      proxy.on('proxyRes', (proxyRes) => {
        delete proxyRes.headers['set-cookie']
      })
```

- [ ] **Step 4: Run tests and verify all pass**

Run: `bun --filter @serverbee/web test vite/dev-proxy`

Expected: 12 tests pass (smoke + 11 behavior cases).

- [ ] **Step 5: Run the full web test suite to confirm no existing test regresses**

Run: `bun --filter @serverbee/web test`

Expected: all tests pass (existing count + 12 new).

- [ ] **Step 6: Commit**

```bash
git add apps/web/vite/dev-proxy.ts apps/web/vite/dev-proxy.test.ts
git commit -m "feat(web): strip Set-Cookie from dev proxy responses"
```

---

## Task 8: Wire dev-proxy into `vite.config.ts`

**Files:**
- Modify: `apps/web/vite.config.ts`

**Context:** Convert the config from object form to function form. Load `.env` from the project root (two levels up from `apps/web`). In `prod-proxy` mode, validate required env vars, delegate `/api` proxy to `createDevProxy`, and inject `VITE_DEV_PROXY_TARGET` via `define`. In default mode, preserve exactly the current behavior.

- [ ] **Step 1: Replace `vite.config.ts`**

Rewrite `apps/web/vite.config.ts` as follows (preserving the existing plugins, resolve, and build sections verbatim):

```ts
import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, loadEnv, type ProxyOptions } from 'vite'
import { VitePWA } from 'vite-plugin-pwa'
import { createDevProxy } from './vite/dev-proxy'

export default defineConfig(({ mode }) => {
  const isProdProxy = mode === 'prod-proxy'

  // Load .env from project root (two levels up from apps/web).
  // Empty prefix = read every variable; we only consume the three we need.
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
    plugins: [
      TanStackRouterVite({
        routeFileIgnorePattern: 'components|hooks|types\\.ts'
      }),
      react(),
      tailwindcss(),
      VitePWA({
        registerType: 'autoUpdate',
        manifest: {
          name: 'ServerBee',
          short_name: 'ServerBee',
          description: 'Server Monitoring Dashboard',
          start_url: '/',
          display: 'standalone',
          background_color: '#0a0a0a',
          theme_color: '#f59e0b',
          icons: [
            { src: '/pwa-192.png', sizes: '192x192', type: 'image/png' },
            { src: '/pwa-512.png', sizes: '512x512', type: 'image/png' },
            { src: '/pwa-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' }
          ]
        },
        workbox: {
          globPatterns: ['**/*.{js,css,html,woff2,png,svg}'],
          navigateFallback: '/index.html',
          runtimeCaching: [
            { urlPattern: /^\/api\//, handler: 'NetworkOnly' },
            { urlPattern: /^\/pwa-/, handler: 'CacheFirst', options: { cacheName: 'pwa-icons' } }
          ]
        }
      })
    ],
    resolve: {
      alias: {
        '@': path.resolve(import.meta.dirname, './src')
      }
    },
    build: {
      rollupOptions: {
        output: {
          manualChunks: {
            xterm: ['@xterm/xterm', '@xterm/addon-fit', '@xterm/addon-web-links'],
            recharts: ['recharts']
          }
        }
      }
    },
    define: devProxyTarget
      ? { 'import.meta.env.VITE_DEV_PROXY_TARGET': JSON.stringify(devProxyTarget) }
      : {},
    server: {
      proxy: {
        '/api': apiProxy
      }
    }
  }
})
```

- [ ] **Step 2: Run typecheck**

Run: `bun --filter @serverbee/web typecheck`

Expected: passes. If you see `Cannot find module './vite/dev-proxy'`, verify Task 1 Step 2 (tsconfig.node.json include) was applied and saved.

- [ ] **Step 3: Smoke-test default mode**

Run: `bun --filter @serverbee/web dev` (in another terminal if you have a Rust server on :9527, otherwise skip the browser check).

Kill the dev server after it prints the "ready" line. The goal here is just to confirm the default mode still parses the config and boots. No UI check needed — banner work is in Task 10+.

- [ ] **Step 4: Commit**

```bash
git add apps/web/vite.config.ts
git commit -m "feat(web): wire dev-proxy factory into vite.config.ts"
```

---

## Task 9: Add `dev:prod` script to `apps/web/package.json`

**Files:**
- Modify: `apps/web/package.json`

**Context:** Expose the `prod-proxy` mode through `bun run dev:prod`. Keep the existing `dev` script untouched.

- [ ] **Step 1: Add the script**

Modify `apps/web/package.json`, adding `"dev:prod": "vite --mode prod-proxy"` to `scripts` immediately after `"dev": "vite"`:

```json
  "scripts": {
    "dev": "vite",
    "dev:prod": "vite --mode prod-proxy",
    "build": "tsc -b && vite build",
    ...
  }
```

- [ ] **Step 2: Smoke-test by running without env vars**

Run: `bun --filter @serverbee/web dev:prod`

Expected: Vite throws an error naming `SERVERBEE_PROD_URL` (or `SERVERBEE_PROD_READONLY_API_KEY` if URL is already set in the project root `.env`). The error should NOT crash silently. This is positive evidence that the validation branch in `vite.config.ts` is reachable.

- [ ] **Step 3: Commit**

```bash
git add apps/web/package.json
git commit -m "feat(web): add dev:prod script for production-proxy mode"
```

---

## Task 10: Create `DevProxyBanner` component

**Files:**
- Create: `apps/web/src/components/dev-proxy-banner.tsx`

**Context:** A small fixed-position warning banner that renders only when `import.meta.env.MODE === 'prod-proxy'`. It reads the target URL from `import.meta.env.VITE_DEV_PROXY_TARGET` (injected by `define` in Task 8). No third-party deps, no theme dependency — it must be legible on both light and dark themes on its own.

- [ ] **Step 1: Create the component**

Create `apps/web/src/components/dev-proxy-banner.tsx`:

```tsx
/**
 * Renders a fixed-position warning banner at the top of the viewport
 * whenever the app is running in Vite's prod-proxy mode. Intended to
 * make it impossible to forget that /api/* requests are hitting
 * production data.
 */
export function DevProxyBanner() {
  if (import.meta.env.MODE !== 'prod-proxy') {
    return null
  }

  const target =
    (import.meta.env.VITE_DEV_PROXY_TARGET as string | undefined) ?? 'unknown'

  return (
    <div
      role="alert"
      style={{
        position: 'fixed',
        top: 0,
        left: 0,
        right: 0,
        zIndex: 2147483647, // max int, overlays everything
        padding: '6px 12px',
        backgroundColor: '#f97316', // Tailwind orange-500
        color: '#ffffff',
        fontSize: '12px',
        fontFamily:
          'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        fontWeight: 600,
        textAlign: 'center',
        letterSpacing: '0.02em',
        boxShadow: '0 2px 4px rgba(0,0,0,0.2)',
        pointerEvents: 'none', // don't intercept clicks
      }}
    >
      ⚠ Dev proxy → PROD ({target}) · read-only
    </div>
  )
}
```

- [ ] **Step 2: Run typecheck**

Run: `bun --filter @serverbee/web typecheck`

Expected: passes. The `import.meta.env` types come from `vite/client` (already in `tsconfig.app.json`).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dev-proxy-banner.tsx
git commit -m "feat(web): add DevProxyBanner component"
```

---

## Task 11: Mount `DevProxyBanner` in root route

**Files:**
- Modify: `apps/web/src/routes/__root.tsx`

**Context:** The banner must appear on every route including unauthenticated ones (login, OAuth landing, 404). The TanStack root route `__root.tsx` renders on all routes unconditionally. Mount the banner inside the top-level fragment, before any theme providers, so it is independent of the theme tree and renders even if a downstream provider crashes.

- [ ] **Step 1: Read current `__root.tsx`**

Run: `cat apps/web/src/routes/__root.tsx` (or use Read tool).

Current structure (verified during planning):

```tsx
import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  return (
    <>
      {import.meta.env.DEV && <Agentation />}
      <ThemeProvider>
        ...
```

- [ ] **Step 2: Add import and mount the banner**

Modify `apps/web/src/routes/__root.tsx` to add the import and place `<DevProxyBanner />` in the fragment, right after the `Agentation` dev-only component:

```tsx
import { createRootRoute, Outlet } from '@tanstack/react-router'
import { Agentation } from 'agentation'
import { DevProxyBanner } from '@/components/dev-proxy-banner'
import { ThemeProvider } from '@/components/theme-provider'
import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'

export const Route = createRootRoute({
  component: RootLayout
})

function RootLayout() {
  return (
    <>
      {import.meta.env.DEV && <Agentation />}
      <DevProxyBanner />
      <ThemeProvider>
        <TooltipProvider>
          <div className="h-screen overflow-hidden bg-background text-foreground">
            <Outlet />
          </div>
          <Toaster />
        </TooltipProvider>
      </ThemeProvider>
    </>
  )
}
```

- [ ] **Step 3: Run lint + typecheck**

Run: `bun --filter @serverbee/web typecheck`

Expected: passes.

Run (from repo root): `bun x ultracite check apps/web/src/routes/__root.tsx apps/web/src/components/dev-proxy-banner.tsx`

Expected: no issues. If Ultracite flags the import ordering, apply `bun x ultracite fix` on the two files and re-verify.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/__root.tsx
git commit -m "feat(web): mount DevProxyBanner in TanStack root route"
```

---

## Task 12: Add `web-dev-prod` menu entry

**Files:**
- Modify: `scripts/make-menu.ts`

**Context:** Register the new workflow with the make-menu runner. Follow the existing `Web` category convention and the `bun --filter @serverbee/web <script>` command shape.

- [ ] **Step 1: Add the entry after `web-dev`**

Modify `scripts/make-menu.ts`. Find the `web-dev` entry near line 108:

```ts
  {
    key: 'web-dev',
    name: 'web:dev',
    category: 'Web',
    description: 'Start the Vite web dev server directly',
    command: 'bun --filter @serverbee/web dev'
  },
```

Insert immediately after it:

```ts
  {
    key: 'web-dev-prod',
    name: 'web:dev:prod',
    category: 'Web',
    description: 'Vite dev server with /api and /api/ws/* proxied to the production Railway backend (read-only; requires SERVERBEE_PROD_URL + SERVERBEE_PROD_READONLY_API_KEY)',
    command: 'bun --filter @serverbee/web dev:prod',
    featured: true
  },
```

- [ ] **Step 2: Verify menu list renders the new entry**

Run: `bun scripts/make-menu.ts menu`

Expected: the output (under the `Web` category) lists `web:dev:prod` with the description above. If you see a TypeScript error, run `bun --filter @serverbee/web typecheck` first — the file is a .ts, not .js.

- [ ] **Step 3: Commit**

```bash
git add scripts/make-menu.ts
git commit -m "feat(scripts): add web-dev-prod menu entry"
```

---

## Task 13: Add `web-dev-prod` to Makefile `COMMAND_TARGETS`

**Files:**
- Modify: `Makefile`

**Context:** The pattern rule at `Makefile:73` delegates every `COMMAND_TARGETS` entry to `$(MENU_RUNNER) run $@`. Without adding `web-dev-prod` to the whitelist, `make web-dev-prod` produces "No rule to make target" even though the menu entry exists. This is issue 2 from the spec review round 2 — do not skip it.

- [ ] **Step 1: Add the target**

Modify `Makefile`. The `COMMAND_TARGETS := \` list near line 5 contains entries like:

```
COMMAND_TARGETS := \
	install \
	dev \
	...
	web-dev \
	web-build \
	...
```

Add `web-dev-prod` to the list. The simplest placement is directly after `web-dev`:

```
	web-dev \
	web-dev-prod \
	web-build \
```

No other Makefile changes are required — the `.PHONY` line at `Makefile:63` already uses `$(COMMAND_TARGETS)` and the pattern rule at `Makefile:73` handles delegation.

- [ ] **Step 2: Verify the target is recognized**

Run: `make -n web-dev-prod` (dry-run, does not execute)

Expected output: something like `bun scripts/make-menu.ts run web-dev-prod`. If you see `make: *** No rule to make target 'web-dev-prod'`, the whitelist edit did not save.

- [ ] **Step 3: Commit**

```bash
git add Makefile
git commit -m "feat(make): register web-dev-prod in COMMAND_TARGETS whitelist"
```

---

## Task 14: Update `.env.example` with split API key

**Files:**
- Modify: `.env.example`

**Context:** Introduce `SERVERBEE_PROD_READONLY_API_KEY` and rework the existing comment so the two keys' purposes are unambiguous. The comment must NOT overstate the safety of the split — `ALLOW_WRITES=1` plus a member-role key still permits whatever writes the backend grants to member role (mobile endpoints exist). This addresses spec review round 3 issue 1.

- [ ] **Step 1: Read the current file**

Current content (verified during planning, 6 lines):

```
# ── Production Database Pull ──────────────────────────────────
# Used by `make db-pull` to download production database locally.
# Create an admin API key in the production web UI first.
SERVERBEE_PROD_URL=https://your-app.up.railway.app
SERVERBEE_PROD_API_KEY=serverbee_xxxxxxxx
```

- [ ] **Step 2: Rewrite the file**

Replace the entire content of `.env.example` with:

```
# ── Production Access ─────────────────────────────────────────
# Shared production URL for both `make db-pull` and
# `make web-dev-prod` workflows.
SERVERBEE_PROD_URL=https://your-app.up.railway.app

# Admin-scoped API key for `make db-pull`.
# The backup API requires admin role; this key MUST be admin.
# DO NOT reuse for dev:prod — mixing credentials would defeat the
# read-only guard when ALLOW_WRITES=1 is set.
SERVERBEE_PROD_API_KEY=serverbee_xxxxxxxx

# Member-role API key for `make web-dev-prod` (Vite dev proxy).
# MUST be a member-role (read-only) key. Create one in production at
# Settings → API Keys, selecting role=member. Keeping this separate
# from SERVERBEE_PROD_API_KEY prevents admin-key bleed-through: even
# if ALLOW_WRITES=1 is set to override the proxy's method check, the
# backend still enforces the member role's permission surface.
# Note: this is NOT zero writes — a handful of member-accessible
# write endpoints exist (e.g. mobile pairing, push registration,
# device deletion under crates/server/src/router/api/mobile.rs).
# Audit your key's permissions if you plan to use ALLOW_WRITES=1.
SERVERBEE_PROD_READONLY_API_KEY=serverbee_xxxxxxxx
```

- [ ] **Step 3: Commit**

```bash
git add .env.example
git commit -m "docs(env): split prod API key into admin (db-pull) and member (dev proxy)"
```

---

## Task 15: Update `AGENTS.md` with debugging workflow

**Files:**
- Modify: `AGENTS.md`

**Context:** Document the new `make web-dev-prod` workflow alongside the existing `make db-pull` flow, with accurate descriptions of each's credential requirements and the proxy's safety model.

- [ ] **Step 1: Find the right insertion point**

Run: `grep -n "db-pull\|server-dev-prod" AGENTS.md` to locate any existing mention.

If an existing section discusses db-pull, add the new section immediately after it. If no such section exists, add a new H2 near the end of `AGENTS.md` titled `## Debugging the frontend with production data`.

- [ ] **Step 2: Insert the new section**

Append (or insert at the right location):

```markdown
## Debugging the frontend with production data

Two workflows exist for seeing production data while developing:

- **`make db-pull && make server-dev-prod`** — downloads a VACUUM'd SQLite
  snapshot from production via the backup API and runs the local Rust
  server against it. Best for backend work where you need a full local
  stack (editing Rust handlers, migrations, background tasks). Uses the
  admin-scoped `SERVERBEE_PROD_API_KEY`. Data is a frozen snapshot — no
  live WebSocket pushes, no agent updates.

- **`make web-dev-prod`** — runs the Vite dev server in `prod-proxy`
  mode. All `/api/*` requests (HTTP and WebSocket) are transparently
  forwarded to the production Railway backend, so the browser sees
  **live** production data and realtime WebSocket pushes. Best for
  frontend style/layout work, live-chart debugging, and UI iteration
  against real load. Uses the member-scoped
  `SERVERBEE_PROD_READONLY_API_KEY`.

  Safety model for `web-dev-prod`:
  - Non-read HTTP methods (POST/PUT/PATCH/DELETE) are blocked with 403
    at the proxy layer. Set `ALLOW_WRITES=1 make web-dev-prod` to
    override, but writes are still gated by the member key's backend
    permissions.
  - `Cookie`, `Authorization` request headers and `Set-Cookie` response
    headers are stripped at the proxy, so "logging in" from localhost
    cannot establish a production session.
  - Every `/api/auth/*` path is blocked except exactly `GET /api/auth/me`
    (used by the UI to show the current user).
  - A persistent orange banner renders on every route to prevent
    forgetting the session is wired to production.

  Configuration: set `SERVERBEE_PROD_URL` and
  `SERVERBEE_PROD_READONLY_API_KEY` in the project root `.env`. See
  `.env.example` for the full comment block.
```

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md
git commit -m "docs: add dev-proxy workflow to AGENTS.md"
```

---

## Task 16: Create manual E2E checklist at `tests/web/dev-proxy.md`

**Files:**
- Create: `tests/web/dev-proxy.md`

**Context:** Match the existing `tests/` manual verification convention. The automated vitest suite covers the proxy factory branches; the manual checklist covers end-to-end user-visible behavior that cannot be tested in isolation (banner visibility, real WebSocket pushes, env-var validation messages).

- [ ] **Step 1: Verify the directory exists or create it**

Run: `ls tests/web/ 2>/dev/null || echo "not present"`

If the directory doesn't exist, create it: `mkdir -p tests/web`

Verify the existing test-file convention: `ls tests/*.md tests/*/*.md 2>/dev/null | head -5` so the new file matches the project style.

- [ ] **Step 2: Create the checklist file**

Create `tests/web/dev-proxy.md`:

```markdown
# Dev Proxy to Production — Manual Verification Checklist

Related spec: `docs/superpowers/specs/2026-04-10-dev-proxy-to-production-design.md`
Related plan: `docs/superpowers/plans/2026-04-10-dev-proxy-to-production.md`

Use this checklist when changing any of:
- `apps/web/vite/dev-proxy.ts`
- `apps/web/vite.config.ts`
- `apps/web/src/components/dev-proxy-banner.tsx`
- `apps/web/src/routes/__root.tsx`

Automated tests in `apps/web/vite/dev-proxy.test.ts` cover the factory's
branches; this checklist covers end-to-end behavior that requires a real
browser and real network traffic.

## Setup

1. Ensure the project root `.env` has:
   - `SERVERBEE_PROD_URL=https://<your-prod>.up.railway.app`
   - `SERVERBEE_PROD_READONLY_API_KEY=<member-role key>`
   - (Optional) `SERVERBEE_PROD_API_KEY=<admin key>` for db-pull
2. Member key must be role=member. Create one in the production UI at
   Settings → API Keys → New → Role: Member.

## Tests

### 1. Default mode unaffected

- [ ] `make dev` (or `make web-dev`) still boots the local Rust server
  and Vite as before.
- [ ] No orange banner is visible.
- [ ] Server list populates from local `:9527` as before.

### 2. Happy path — live production data

- [ ] `make web-dev-prod` boots Vite without errors.
- [ ] Browser at `http://localhost:5173/` shows the orange banner:
  `⚠ Dev proxy → PROD (https://...) · read-only`.
- [ ] Server list populates with production servers.
- [ ] CPU / memory / network charts show data moving in real time
  (WebSocket `ServerUpdate` pushes are working).
- [ ] WebSocket connection indicator shows "connected".

### 3. Write block

- [ ] Attempt to delete a server, toggle a server's visibility, or save
  any setting.
- [ ] DevTools Network tab shows the request returning 403.
- [ ] Response body contains the phrase "read-only".
- [ ] Production state is unchanged (refresh and confirm).

### 4. Escape hatch (optional)

- [ ] Stop the dev server. Re-run with `ALLOW_WRITES=1 make web-dev-prod`.
- [ ] Repeat a harmless write (e.g. toggle a hidden server's remark).
- [ ] Request reaches production. It either succeeds (member key has
  permission) or returns a backend 403 (member key lacks permission).
  Either is acceptable — the point is the proxy layer no longer blocks.

### 5. Auth block

- [ ] Navigate to `/login`.
- [ ] Attempt to sign in with any credentials.
- [ ] DevTools Network tab shows `POST /api/auth/login` returning 403
  with the message "Auth paths are blocked in dev proxy".
- [ ] DevTools Network tab shows no `Set-Cookie` response header on any
  proxied response (important — this is the session-isolation check).

### 6. Missing env vars

- [ ] Temporarily rename `.env` to `.env.bak`.
- [ ] Run `make web-dev-prod`.
- [ ] Expected: Vite refuses to start, error names
  `SERVERBEE_PROD_URL` and points at the project root `.env`.
- [ ] Restore `.env` → `mv .env.bak .env`.
- [ ] Temporarily comment out `SERVERBEE_PROD_READONLY_API_KEY` in
  `.env`.
- [ ] Run `make web-dev-prod`.
- [ ] Expected: error names `SERVERBEE_PROD_READONLY_API_KEY` and
  explicitly warns against reusing `SERVERBEE_PROD_API_KEY`.
- [ ] Restore `.env`.

### 7. Separation check (documented failure mode)

This test documents the known unsafe configuration that the split API
key is designed to make deliberate:

- [ ] Set `SERVERBEE_PROD_READONLY_API_KEY` to the value of
  `SERVERBEE_PROD_API_KEY` (admin key in the wrong slot).
- [ ] Run `ALLOW_WRITES=1 make web-dev-prod`.
- [ ] Perform a harmless write.
- [ ] Expected: write reaches production and succeeds (because the
  proxy layer is overridden and the backend trusts the admin key).
- [ ] Restore `.env` to the correct member key immediately after.

The point of this test is not that it should pass — it's that the
failure mode is documented, opt-in, and requires typing two incorrect
configurations on the command line.

### 8. Visual QA

- [ ] Banner is legible on light theme.
- [ ] Banner is legible on dark theme.
- [ ] Banner stays fixed during scroll.
- [ ] Banner does not intercept clicks (pointerEvents: none).
- [ ] Login form inputs are not obscured by the banner (or obscured
  only by the expected 28-30px margin, which is acceptable).

## Pass criteria

Tests 1, 2, 3, 5, 6 must all pass. Test 4 is optional (requires a
harmless writable row). Test 7 documents the intended failure mode and
does not need to "pass" in the traditional sense — confirm only that
the behavior matches the description. Test 8 is visual QA; screenshot
before/after when the banner component changes.
```

- [ ] **Step 3: Commit**

```bash
git add tests/web/dev-proxy.md
git commit -m "docs(tests): add manual verification checklist for dev proxy"
```

---

## Task 17: Final verification

**Files:** (none — this task only runs checks)

**Context:** Run all the project quality gates to confirm nothing regressed.

- [ ] **Step 1: Full web test suite**

Run: `bun --filter @serverbee/web test`

Expected: all existing tests pass + 12 new tests in `vite/dev-proxy.test.ts` pass. 0 failures.

- [ ] **Step 2: Web typecheck**

Run: `bun --filter @serverbee/web typecheck`

Expected: 0 errors.

- [ ] **Step 3: Web lint**

Run: `bun x ultracite check apps/web/vite apps/web/src/components/dev-proxy-banner.tsx apps/web/src/routes/__root.tsx apps/web/vite.config.ts`

Expected: 0 issues. If there are any, run `bun x ultracite fix` on the same paths and verify the diff is cosmetic only.

- [ ] **Step 4: Root lint (broader scope, catches issues in scripts/)**

Run: `bun run lint` (from the repo root)

Expected: 0 issues.

- [ ] **Step 5: Default dev mode still works**

Run: `bun --filter @serverbee/web dev` and wait for the "ready" line.

Expected: no errors on boot. Open `http://localhost:5173/` and verify:
- No orange banner (default mode).
- `import.meta.env.MODE` is `development` (check in browser console).

Stop the dev server.

- [ ] **Step 6: prod-proxy mode fails cleanly without env vars**

Temporarily ensure `.env` does not have the new var set (or rename `.env`).

Run: `bun --filter @serverbee/web dev:prod`

Expected: Vite exits with an error naming `SERVERBEE_PROD_URL` or `SERVERBEE_PROD_READONLY_API_KEY`. The error message must tell the developer to set it in the project root `.env` and warn against reusing `SERVERBEE_PROD_API_KEY`.

Restore `.env` after the check.

- [ ] **Step 7: (Optional) prod-proxy happy path end-to-end**

With the member-role `SERVERBEE_PROD_READONLY_API_KEY` set in `.env`:

Run: `make web-dev-prod`

Open `http://localhost:5173/` and verify tests 2, 3, and 5 from `tests/web/dev-proxy.md` pass. This is a full E2E smoke and is the only step that requires network access to production.

- [ ] **Step 8: No commit**

This task is verification only. If any step failed, return to the relevant earlier task, fix, and re-run this task from Step 1.

---

## Rollback Notes

Every task ends with a commit. If a later task reveals a problem, revert the specific commit with `git revert <hash>` — all edits are small and isolated. The most likely revert target is Task 8 (vite.config.ts), which is the only file whose full rewrite could be disruptive; the rewrite preserves the existing plugins/resolve/build verbatim, so a revert is safe.
