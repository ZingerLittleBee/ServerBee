import { useCallback, useMemo, useSyncExternalStore } from 'react'
import { getRuntime, type ServerSummary } from '../runtime-context'

const CAPS: Record<string, number> = {
  CAP_TERMINAL: 1,
  CAP_EXEC: 2,
  CAP_UPGRADE: 4,
  CAP_PING_ICMP: 8,
  CAP_PING_TCP: 16,
  CAP_PING_HTTP: 32,
  CAP_FILE: 64,
  CAP_DOCKER: 128,
  CAP_SECURITY_EVENTS: 256,
  CAP_FIREWALL_BLOCK: 512,
  CAP_IP_QUALITY: 1024
}

export function useServers(): ServerSummary[] {
  const rt = getRuntime()
  return useSyncExternalStore(rt.subscribeServers, rt.serversStore, rt.serversStore)
}

const PATH_TOKEN_RE = /[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\]/g

export function useServer(id: string | null): unknown {
  const rt = getRuntime()
  const subscribe = rt.subscribeServers
  const getSnapshot = useCallback((): unknown => {
    if (id === null) {
      return undefined
    }
    return rt.serverByIdStore(id)
  }, [rt, id])
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function useMetric(id: string | null, path: string): number | string | undefined {
  const rt = getRuntime()
  const subscribe = rt.subscribeServers
  const tokens = useMemo(() => path.match(PATH_TOKEN_RE) ?? [], [path])
  const getSnapshot = useCallback((): number | string | undefined => {
    if (id === null) {
      return undefined
    }
    const server = rt.serverByIdStore(id) as Record<string, unknown> | undefined
    if (!server) {
      return undefined
    }
    let cur: unknown = server
    for (const tok of tokens) {
      if (cur == null || typeof cur !== 'object') {
        return undefined
      }
      const obj = cur as Record<string, unknown> | unknown[]
      cur = tok.startsWith('[') ? (obj as unknown[])[Number(tok.slice(1, -1))] : (obj as Record<string, unknown>)[tok]
    }
    if (typeof cur === 'number' || typeof cur === 'string') {
      return cur
    }
    return undefined
  }, [rt, id, tokens])
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}

export function useCapability(id: string | null, cap: string): boolean {
  const rt = getRuntime()
  const subscribe = rt.subscribeServers
  const getSnapshot = useCallback((): boolean => {
    if (id === null) {
      return false
    }
    const bit = CAPS[cap]
    if (!bit) {
      return false
    }
    const server = rt.serversStore().find((s) => s.id === id)
    // biome-ignore lint/suspicious/noBitwiseOperators: capabilities is a bitmask, AND is the intended operation
    return server ? (server.capabilities & bit) !== 0 : false
  }, [rt, id, cap])
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot)
}
