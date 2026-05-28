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
  return getRuntime().serversStore()
}

export function useServer(id: string | null): unknown {
  if (id === null) {
    return undefined
  }
  return getRuntime().serverByIdStore(id)
}

const PATH_TOKEN_RE = /[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\]/g

export function useMetric(id: string | null, path: string): number | string | undefined {
  if (id === null) {
    return undefined
  }
  const server = getRuntime().serverByIdStore(id) as Record<string, any> | undefined
  if (!server) {
    return undefined
  }
  const tokens = path.match(PATH_TOKEN_RE) ?? []
  let cur: any = server
  for (const tok of tokens) {
    if (cur == null) {
      return undefined
    }
    cur = tok.startsWith('[') ? cur[Number(tok.slice(1, -1))] : cur[tok]
  }
  return cur
}

export function useCapability(id: string | null, cap: string): boolean {
  if (id === null) {
    return false
  }
  const bit = CAPS[cap]
  if (!bit) {
    return false
  }
  const server = getRuntime()
    .serversStore()
    .find((s) => s.id === id)
  return server ? (server.capabilities & bit) !== 0 : false
}
