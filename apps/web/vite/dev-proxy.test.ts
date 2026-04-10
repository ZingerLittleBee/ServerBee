import { describe, expect, it, vi } from 'vitest'
import {
  createDevProxy,
  type DevProxyHookEventMap,
  type DevProxyHookRegistrar,
  type DevProxyOptions,
  registerDevProxyHandlers
} from './dev-proxy'

type MockReqHeaders = Record<string, string>

interface MockReq {
  headers: MockReqHeaders
  method?: string
  url?: string
}

interface MockRes {
  end: ReturnType<typeof vi.fn<(body: string) => void>>
  writeHead: ReturnType<typeof vi.fn<(statusCode: number, headers: Record<string, string>) => void>>
}

interface MockProxyReq {
  abort?: () => void
  destroy?: () => void
  removeHeader: (name: string) => void
  setHeader: (name: string, value: string) => void
}

interface MockProxyRes {
  headers: Record<string, unknown>
}

type MockProxyHandlers = {
  [EventName in keyof DevProxyHookEventMap]: Array<(...args: DevProxyHookEventMap[EventName]) => void>
}

interface MockProxy extends DevProxyHookRegistrar {
  emit<EventName extends keyof DevProxyHookEventMap>(event: EventName, ...args: DevProxyHookEventMap[EventName]): void
}

function addHandler<EventName extends keyof DevProxyHookEventMap>(
  handlers: MockProxyHandlers,
  event: EventName,
  handler: (...args: DevProxyHookEventMap[EventName]) => void
) {
  handlers[event].push(handler)
}

function emitHandlers<EventName extends keyof DevProxyHookEventMap>(
  handlers: MockProxyHandlers,
  event: EventName,
  ...args: DevProxyHookEventMap[EventName]
) {
  for (const handler of handlers[event]) {
    handler(...args)
  }
}

/**
 * Minimal fake of http-proxy's event emitter that captures registered
 * handlers and lets us invoke them with mock req/res/proxyReq objects.
 */
function makeMockProxy(): MockProxy {
  const handlers: MockProxyHandlers = {
    proxyReq: [],
    proxyReqWs: [],
    proxyRes: []
  }

  return {
    on<EventName extends keyof DevProxyHookEventMap>(
      event: EventName,
      handler: (...args: DevProxyHookEventMap[EventName]) => void
    ) {
      addHandler(handlers, event, handler)
    },
    emit<EventName extends keyof DevProxyHookEventMap>(event: EventName, ...args: DevProxyHookEventMap[EventName]) {
      emitHandlers(handlers, event, ...args)
    }
  }
}

function makeMockProxyReq(): MockProxyReq {
  return {
    setHeader: vi.fn<(name: string, value: string) => void>(),
    removeHeader: vi.fn<(name: string) => void>(),
    destroy: vi.fn<() => void>(),
    abort: vi.fn<() => void>()
  }
}

function makeAbortOnlyProxyReq(): MockProxyReq {
  return {
    setHeader: vi.fn<(name: string, value: string) => void>(),
    removeHeader: vi.fn<(name: string) => void>(),
    abort: vi.fn<() => void>()
  }
}

function makeMockReq(method: string, url: string, headers: MockReqHeaders = {}): MockReq {
  return { method, url, headers }
}

function makeMockRes(): MockRes {
  return {
    writeHead: vi.fn<(statusCode: number, headers: Record<string, string>) => void>(),
    end: vi.fn<(body: string) => void>()
  }
}

const baseOpts: DevProxyOptions = {
  target: 'https://prod.example.com',
  readonlyApiKey: 'serverbee_test_member',
  allowWrites: false
}

function makeRegisteredProxy(opts: DevProxyOptions = baseOpts): MockProxy {
  const proxy = makeMockProxy()
  registerDevProxyHandlers(proxy, opts)
  return proxy
}

describe('createDevProxy', () => {
  it('returns a ProxyOptions object with target, ws, and configure', () => {
    const result = createDevProxy(baseOpts)

    expect(result.target).toBe('https://prod.example.com')
    expect(result.ws).toBe(true)
    expect(result.changeOrigin).toBe(true)
    expect(typeof result.configure).toBe('function')
  })

  describe('write-method block', () => {
    it('blocks POST with 403 when allowWrites is false (default)', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('POST', '/api/servers')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.objectContaining({ 'content-type': 'application/json' }))
      expect(res.end).toHaveBeenCalledWith(expect.stringContaining('read-only'))
      expect(proxyReq.destroy).toHaveBeenCalled()
      expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
    })

    it('allows POST when allowWrites is true (escape hatch)', () => {
      const proxy = makeRegisteredProxy({ ...baseOpts, allowWrites: true })

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('POST', '/api/servers')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).not.toHaveBeenCalled()
      expect(proxyReq.destroy).not.toHaveBeenCalled()
      expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
      expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
      expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
    })

    it('falls back to abort() when destroy() is unavailable', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeAbortOnlyProxyReq()
      const req = makeMockReq('DELETE', '/api/servers')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.objectContaining({ 'content-type': 'application/json' }))
      expect(proxyReq.abort).toHaveBeenCalled()
    })
  })

  describe('header stripping and X-API-Key injection', () => {
    it('strips Cookie and Authorization, then injects X-API-Key on GET', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/servers', {
        cookie: 'session_token=leaked',
        authorization: 'Bearer leaked'
      })
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
      expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
      expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
      expect(res.writeHead).not.toHaveBeenCalled()
    })
  })

  describe('auth path block', () => {
    it('blocks POST /api/auth/login with auth-specific error message', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('POST', '/api/auth/login')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
      const body = res.end.mock.calls[0]?.[0]
      expect(body).toContain('Auth paths')
      expect(body).not.toContain('read-only')
      expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
    })

    it('blocks GET /api/auth/oauth/github/callback (read method, still blocked)', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/auth/oauth/github/callback')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
      expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
    })
  })

  describe('auth path allow-list for GET /api/auth/me', () => {
    it('allows GET /api/auth/me through with headers stripped and key injected', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/auth/me', {
        cookie: 'session_token=leaked'
      })
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).not.toHaveBeenCalled()
      expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
      expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
    })

    it('allows GET /api/auth/me?_t=123 (query string ignored in matching)', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/auth/me?_t=123')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).not.toHaveBeenCalled()
      expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
    })

    it('blocks POST /api/auth/me (method-scoped allow-list)', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('POST', '/api/auth/me')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
      const body = res.end.mock.calls[0]?.[0]
      expect(body).toContain('Auth paths')
      expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
    })

    it('blocks GET /api/auth/me/evil (exact-match, not prefix)', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/auth/me/evil')
      const res = makeMockRes()
      proxy.emit('proxyReq', proxyReq, req, res)

      expect(res.writeHead).toHaveBeenCalledWith(403, expect.any(Object))
      expect(proxyReq.setHeader).not.toHaveBeenCalledWith('X-API-Key', expect.anything())
    })
  })

  describe('WebSocket upgrade', () => {
    it('strips Cookie/Authorization and injects X-API-Key on proxyReqWs', () => {
      const proxy = makeRegisteredProxy()

      const proxyReq = makeMockProxyReq()
      const req = makeMockReq('GET', '/api/ws/servers', {
        cookie: 'session_token=leaked',
        authorization: 'Bearer leaked'
      })

      proxy.emit('proxyReqWs', proxyReq, req, {}, {}, Buffer.alloc(0))

      expect(proxyReq.removeHeader).toHaveBeenCalledWith('cookie')
      expect(proxyReq.removeHeader).toHaveBeenCalledWith('authorization')
      expect(proxyReq.setHeader).toHaveBeenCalledWith('X-API-Key', 'serverbee_test_member')
    })
  })

  describe('Set-Cookie response stripping', () => {
    it('removes Set-Cookie from proxyRes headers', () => {
      const proxy = makeRegisteredProxy()

      const proxyRes: MockProxyRes = {
        headers: {
          'content-type': 'application/json',
          'set-cookie': ['session_token=abc; Secure; HttpOnly']
        }
      }

      proxy.emit('proxyRes', proxyRes, makeMockReq('GET', '/api/servers'), makeMockRes())

      expect(proxyRes.headers['set-cookie']).toBeUndefined()
      expect(proxyRes.headers['content-type']).toBe('application/json')
    })
  })
})
