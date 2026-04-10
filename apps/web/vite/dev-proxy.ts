import type { ProxyOptions } from 'vite'

export interface DevProxyOptions {
  /** True only when ALLOW_WRITES=1 is set; unlocks non-read HTTP methods */
  allowWrites: boolean
  /** Member-role API key (validated by caller) */
  readonlyApiKey: string
  /** Fully-qualified production URL, e.g. https://xxx.up.railway.app */
  target: string
}

interface ProxyReqLike {
  abort?(): void
  destroy?(): void
  removeHeader(name: string): void
  setHeader(name: string, value: string): void
}

interface IncomingReqLike {
  method?: string
  url?: string
}

interface ServerResLike {
  end(body: string): void
  writeHead(statusCode: number, headers: Record<string, string>): void
}

interface ProxyResLike {
  headers: Record<string, unknown>
}

export interface DevProxyHookEventMap {
  proxyReq: [proxyReq: ProxyReqLike, req: IncomingReqLike, res: ServerResLike]
  proxyReqWs: [proxyReq: ProxyReqLike, req: IncomingReqLike, socket: unknown, options: unknown, head: unknown]
  proxyRes: [proxyRes: ProxyResLike, req: IncomingReqLike, res: ServerResLike]
}

export interface DevProxyHookRegistrar {
  on(event: 'proxyReq', handler: (...args: DevProxyHookEventMap['proxyReq']) => void): void
  on(event: 'proxyReqWs', handler: (...args: DevProxyHookEventMap['proxyReqWs']) => void): void
  on(event: 'proxyRes', handler: (...args: DevProxyHookEventMap['proxyRes']) => void): void
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
    configure: (proxy) => {
      registerDevProxyHandlers(proxy, opts)
    }
  }
}

export function registerDevProxyHandlers(proxy: DevProxyHookRegistrar, opts: DevProxyOptions) {
  proxy.on('proxyReq', (proxyReq, req, res) => {
    const url = req.url ?? ''
    const pathname = url.split('?')[0]
    const method = (req.method ?? 'GET').toUpperCase()

    if (pathname.startsWith('/api/auth/')) {
      const isAllowedAuthRead = method === 'GET' && pathname === '/api/auth/me'

      if (!isAllowedAuthRead) {
        respond403(res, proxyReq, 'Auth paths are blocked in dev proxy to prevent production session leakage.')
        return
      }
    }

    const isReadOnly = method === 'GET' || method === 'HEAD' || method === 'OPTIONS'

    if (!(isReadOnly || opts.allowWrites)) {
      respond403(res, proxyReq, 'Dev proxy is read-only. Set ALLOW_WRITES=1 to override.')
      return
    }

    proxyReq.removeHeader('cookie')
    proxyReq.removeHeader('authorization')
    proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
  })

  proxy.on('proxyReqWs', (proxyReq) => {
    proxyReq.removeHeader('cookie')
    proxyReq.removeHeader('authorization')
    proxyReq.setHeader('X-API-Key', opts.readonlyApiKey)
  })

  proxy.on('proxyRes', (proxyRes) => {
    proxyRes.headers['set-cookie'] = undefined
  })
}

function respond403(res: ServerResLike, proxyReq: ProxyReqLike, message: string) {
  res.writeHead(403, { 'content-type': 'application/json' })
  res.end(JSON.stringify({ error: message }))

  if (typeof proxyReq.destroy === 'function') {
    proxyReq.destroy()
    return
  }

  if (typeof proxyReq.abort === 'function') {
    proxyReq.abort()
  }
}
