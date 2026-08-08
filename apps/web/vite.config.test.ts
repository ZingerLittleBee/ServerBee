// @vitest-environment node
// Importing the real vite config pulls in esbuild, whose
// `TextEncoder().encode("") instanceof Uint8Array` invariant breaks under
// jsdom's realm-mismatched globals. This test exercises no DOM, so node is
// the correct environment for it.
import type { ProxyOptions, UserConfig } from 'vite'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import config from './vite.config'
import type { DevProxyHookEventMap } from './vite/dev-proxy'

const callConfig = config as unknown as (env: { command: 'serve'; mode: string }) => Promise<UserConfig> | UserConfig

function pluginNames(resolved: UserConfig): string[] {
  const flatten = (items: unknown[]): unknown[] =>
    items.flatMap((item) => (Array.isArray(item) ? flatten(item) : [item]))
  return flatten([...(resolved.plugins ?? [])])
    .map((plugin) => (plugin as { name?: string } | null)?.name)
    .filter((name): name is string => typeof name === 'string')
}

type ProxyHandlers = { [K in keyof DevProxyHookEventMap]?: Array<(...args: DevProxyHookEventMap[K]) => void> }

/**
 * Invoke the proxy entry's own `configure` against a capturing registrar so
 * the real createDevProxy guard handlers can be exercised. The exhaustive
 * guard matrix lives in vite/dev-proxy.test.ts; this proves the prod-proxy
 * config output is wired to those guards.
 */
function captureProxyHandlers(proxy: ProxyOptions): ProxyHandlers {
  const handlers: ProxyHandlers = {}
  const registrar = {
    on<K extends keyof DevProxyHookEventMap>(event: K, handler: (...args: DevProxyHookEventMap[K]) => void) {
      ;(handlers[event] ??= []).push(handler)
    }
  }
  proxy.configure?.(registrar as never, proxy)
  return handlers
}

function emit<K extends keyof DevProxyHookEventMap>(
  handlers: ProxyHandlers,
  event: K,
  ...args: DevProxyHookEventMap[K]
) {
  for (const handler of handlers[event] ?? []) {
    handler(...args)
  }
}

const ENV_KEYS = ['ALLOW_WRITES', 'BONEYARD_AUTO_CAPTURE', 'SERVERBEE_PROD_READONLY_API_KEY', 'SERVERBEE_PROD_URL']

describe('vite config boneyard integration', () => {
  const saved: Record<string, string | undefined> = {}

  beforeEach(() => {
    for (const key of ENV_KEYS) {
      saved[key] = process.env[key]
      delete process.env[key]
    }
  })

  afterEach(() => {
    for (const key of ENV_KEYS) {
      if (saved[key] === undefined) {
        delete process.env[key]
      } else {
        process.env[key] = saved[key]
      }
    }
  })

  it('omits the boneyard auto-capture plugin by default in normal dev mode', async () => {
    const resolved = await callConfig({ command: 'serve', mode: 'development' })

    expect(pluginNames(resolved)).not.toContain('boneyard')
    expect(resolved.server?.proxy?.['/api']).toMatchObject({
      changeOrigin: true,
      target: 'http://localhost:9527',
      ws: true
    })
  })

  it('omits the boneyard auto-capture plugin by default in prod-proxy mode', async () => {
    // loadEnv gives existing process.env values priority over any .env file,
    // so these stubs win without touching real credentials.
    process.env.SERVERBEE_PROD_URL = 'https://prod.example.com'
    process.env.SERVERBEE_PROD_READONLY_API_KEY = 'serverbee_test_readonly_key'

    const resolved = await callConfig({ command: 'serve', mode: 'prod-proxy' })

    expect(pluginNames(resolved)).not.toContain('boneyard')
  })

  it('registers the plugin in both modes only when explicitly opted in', async () => {
    process.env.BONEYARD_AUTO_CAPTURE = '1'

    const dev = await callConfig({ command: 'serve', mode: 'development' })
    expect(pluginNames(dev)).toContain('boneyard')

    process.env.SERVERBEE_PROD_URL = 'https://prod.example.com'
    process.env.SERVERBEE_PROD_READONLY_API_KEY = 'serverbee_test_readonly_key'
    const prod = await callConfig({ command: 'serve', mode: 'prod-proxy' })

    expect(pluginNames(prod)).toContain('boneyard')
    expect(prod.define?.['import.meta.env.VITE_DEV_PROXY_TARGET']).toBe('"https://prod.example.com"')
    expect(prod.define?.['import.meta.env.VITE_DEV_PROXY_ALLOW_WRITES']).toBe('"0"')
  })

  it('wires the prod-proxy /api entry to the createDevProxy safety guards', async () => {
    process.env.SERVERBEE_PROD_URL = 'https://prod.example.com'
    process.env.SERVERBEE_PROD_READONLY_API_KEY = 'serverbee_test_readonly_key'

    const resolved = await callConfig({ command: 'serve', mode: 'prod-proxy' })
    const proxy = resolved.server?.proxy?.['/api'] as ProxyOptions
    expect(proxy.target).toBe('https://prod.example.com')
    expect(typeof proxy.configure).toBe('function')

    const handlers = captureProxyHandlers(proxy)

    // Write methods are blocked with 403 by default.
    const writeRes = { writeHead: vi.fn(), end: vi.fn() }
    const writeProxyReq = { removeHeader: vi.fn(), setHeader: vi.fn(), destroy: vi.fn() }
    emit(
      handlers,
      'proxyReq',
      writeProxyReq as never,
      { method: 'POST', url: '/api/servers' } as never,
      writeRes as never
    )
    expect(writeRes.writeHead).toHaveBeenCalledWith(403, expect.objectContaining({ 'content-type': 'application/json' }))
    expect(writeProxyReq.destroy).toHaveBeenCalled()
    expect(writeProxyReq.setHeader).not.toHaveBeenCalled()

    // Allowed reads get Cookie/Authorization stripped and the member key injected.
    const readRes = { writeHead: vi.fn(), end: vi.fn() }
    const readProxyReq = { removeHeader: vi.fn(), setHeader: vi.fn(), destroy: vi.fn() }
    emit(handlers, 'proxyReq', readProxyReq as never, { method: 'GET', url: '/api/servers' } as never, readRes as never)
    expect(readRes.writeHead).not.toHaveBeenCalled()
    expect(readProxyReq.removeHeader).toHaveBeenCalledWith('cookie')
    expect(readProxyReq.removeHeader).toHaveBeenCalledWith('authorization')
    expect(readProxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_readonly_key')

    // Auth paths stay blocked (except GET /api/auth/me, covered in dev-proxy tests).
    const authRes = { writeHead: vi.fn(), end: vi.fn() }
    emit(
      handlers,
      'proxyReq',
      { removeHeader: vi.fn(), setHeader: vi.fn(), destroy: vi.fn() } as never,
      { method: 'POST', url: '/api/auth/login' } as never,
      authRes as never
    )
    expect(authRes.writeHead).toHaveBeenCalledWith(403, expect.any(Object))

    // WebSocket upgrades outside the /api/ws/servers allow-list are refused.
    const socket = { write: vi.fn(), destroy: vi.fn() }
    emit(
      handlers,
      'proxyReqWs',
      { removeHeader: vi.fn(), setHeader: vi.fn(), destroy: vi.fn() } as never,
      { method: 'GET', url: '/api/ws/terminal/server-1' } as never,
      socket as never,
      {} as never,
      {} as never
    )
    expect(socket.write).toHaveBeenCalledWith(expect.stringContaining('403 Forbidden'))
    expect(socket.destroy).toHaveBeenCalled()
  })
})
