import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import type { NetworkProbeResultData } from '@/lib/network-types'
import { WsClient } from '@/lib/ws-client'
import type {
  DockerContainer,
  DockerContainerStats,
  DockerEventInfo
} from '@/routes/_authed/servers/$serverId/docker/types'

const MAX_DOCKER_EVENTS = 100

interface ServerMetrics {
  capabilities?: number
  country_code: string | null
  cpu: number
  cpu_name: string | null
  disk_total: number
  disk_used: number
  features?: string[]
  group_id: string | null
  id: string
  last_active: number
  load1: number
  load5: number
  load15: number
  mem_total: number
  mem_used: number
  name: string
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  online: boolean
  os: string | null
  process_count: number
  protocol_version?: number
  region: string | null
  swap_total: number
  swap_used: number
  tcp_conn: number
  udp_conn: number
  uptime: number
}

type WsMessage =
  | { type: 'full_sync'; servers: ServerMetrics[] }
  | { type: 'update'; servers: ServerMetrics[] }
  | { type: 'server_online'; server_id: string }
  | { type: 'server_offline'; server_id: string }
  | { type: 'capabilities_changed'; server_id: string; capabilities: number }
  | { type: 'agent_info_updated'; server_id: string; protocol_version: number }
  | { type: 'network_probe_update'; server_id: string; results: NetworkProbeResultData[] }
  | {
      type: 'docker_update'
      server_id: string
      containers: DockerContainer[]
      stats: DockerContainerStats[] | null
    }
  | { type: 'docker_event'; server_id: string; event: DockerEventInfo }
  | { type: 'docker_availability_changed'; server_id: string; available: boolean }

export type { ServerMetrics }

const STATIC_FIELDS = new Set([
  'mem_total',
  'swap_total',
  'disk_total',
  'cpu_name',
  'os',
  'region',
  'country_code',
  'group_id'
])

export function mergeServerUpdate(prev: ServerMetrics[], incoming: ServerMetrics[]): ServerMetrics[] {
  const updated = [...prev]
  for (const server of incoming) {
    const idx = updated.findIndex((s) => s.id === server.id)
    if (idx >= 0) {
      const merged = { ...updated[idx] }
      for (const [key, value] of Object.entries(server)) {
        const isStaticDefault = STATIC_FIELDS.has(key) && (value === null || value === 0)
        if (!isStaticDefault) {
          ;(merged as Record<string, unknown>)[key] = value
        }
      }
      updated[idx] = merged as ServerMetrics
    }
  }
  return updated
}

export function setServerOnlineStatus(prev: ServerMetrics[], serverId: string, online: boolean): ServerMetrics[] {
  return prev.map((s) => (s.id === serverId ? { ...s, online } : s))
}

export function setServerDockerAvailability(
  prev: ServerMetrics[],
  serverId: string,
  available: boolean
): ServerMetrics[] {
  return prev.map((s) => {
    if (s.id !== serverId) {
      return s
    }
    const features = s.features ?? []
    if (available && !features.includes('docker')) {
      return { ...s, features: [...features, 'docker'] }
    }
    if (!available && features.includes('docker')) {
      return { ...s, features: features.filter((f) => f !== 'docker') }
    }
    return s
  })
}

function setServerDetailDockerAvailability(
  prev: Record<string, unknown> | undefined,
  available: boolean
): Record<string, unknown> | undefined {
  if (!prev) {
    return prev
  }

  const features = Array.isArray(prev.features)
    ? prev.features.filter((feature): feature is string => typeof feature === 'string')
    : []

  if (available && !features.includes('docker')) {
    return { ...prev, features: [...features, 'docker'] }
  }

  if (!available && features.includes('docker')) {
    return { ...prev, features: features.filter((feature) => feature !== 'docker') }
  }

  return prev
}

type QueryClient = ReturnType<typeof useQueryClient>

function isWsMessageLike(raw: unknown): raw is { type: string } & Record<string, unknown> {
  return typeof raw === 'object' && raw !== null && 'type' in raw && typeof (raw as { type: unknown }).type === 'string'
}

function handleServerMetricsMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'full_sync' || raw.type === 'update') {
    if (!Array.isArray(raw.servers) || raw.servers.some((s: unknown) => s == null || typeof s !== 'object')) {
      return
    }
    const msg = raw as WsMessage & { type: 'full_sync' | 'update' }
    if (raw.type === 'full_sync') {
      queryClient.setQueryData<ServerMetrics[]>(['servers'], msg.servers)
    } else {
      queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
        prev ? mergeServerUpdate(prev, msg.servers) : msg.servers
      )
    }
    return
  }
  if (raw.type === 'server_online' || raw.type === 'server_offline') {
    if (typeof raw.server_id !== 'string') {
      return
    }
    const online = raw.type === 'server_online'
    const { server_id } = raw as { server_id: string } & Record<string, unknown>
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev ? setServerOnlineStatus(prev, server_id, online) : prev
    )
  }
}

function handleCapabilityMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'capabilities_changed') {
    if (typeof raw.server_id !== 'string' || typeof raw.capabilities !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'capabilities_changed' }
    const { server_id, capabilities } = msg
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev?.map((s) => (s.id === server_id ? { ...s, capabilities } : s))
    )
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      prev ? { ...prev, capabilities } : prev
    )
    queryClient.invalidateQueries({ queryKey: ['servers-list'] })
    return
  }
  if (raw.type === 'agent_info_updated') {
    if (typeof raw.server_id !== 'string' || typeof raw.protocol_version !== 'number') {
      return
    }
    const msg = raw as WsMessage & { type: 'agent_info_updated' }
    const { server_id, protocol_version } = msg
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      prev ? { ...prev, protocol_version } : prev
    )
    queryClient.invalidateQueries({ queryKey: ['servers-list'] })
  }
}

function handleDockerMessage(raw: { type: string } & Record<string, unknown>, queryClient: QueryClient): void {
  if (raw.type === 'docker_update') {
    if (
      typeof raw.server_id !== 'string' ||
      !Array.isArray(raw.containers) ||
      raw.containers.some((c: unknown) => c == null || typeof c !== 'object')
    ) {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_update' }
    const { server_id, containers, stats } = msg
    queryClient.setQueryData<DockerContainer[]>(['docker', 'containers', server_id], containers)
    if (stats) {
      queryClient.setQueryData<DockerContainerStats[]>(['docker', 'stats', server_id], stats)
    }
    return
  }
  if (raw.type === 'docker_event') {
    if (typeof raw.server_id !== 'string' || typeof raw.event !== 'object' || raw.event === null) {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_event' }
    const { server_id, event } = msg
    queryClient.setQueryData<DockerEventInfo[]>(['docker', 'events', server_id], (prev) => {
      const events = prev ?? []
      const updated = [event, ...events]
      return updated.length > MAX_DOCKER_EVENTS ? updated.slice(0, MAX_DOCKER_EVENTS) : updated
    })
    return
  }
  if (raw.type === 'docker_availability_changed') {
    if (typeof raw.server_id !== 'string' || typeof raw.available !== 'boolean') {
      return
    }
    const msg = raw as WsMessage & { type: 'docker_availability_changed' }
    const { server_id, available } = msg
    queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
      prev ? setServerDockerAvailability(prev, server_id, available) : prev
    )
    queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
      setServerDetailDockerAvailability(prev, available)
    )
  }
}

function handleWsMessage(raw: unknown, queryClient: QueryClient): void {
  if (!isWsMessageLike(raw)) {
    console.warn('WS: unexpected message shape', raw)
    return
  }
  switch (raw.type) {
    case 'full_sync':
    case 'update':
    case 'server_online':
    case 'server_offline':
      handleServerMetricsMessage(raw, queryClient)
      break
    case 'capabilities_changed':
    case 'agent_info_updated':
      handleCapabilityMessage(raw, queryClient)
      break
    case 'network_probe_update': {
      if (
        typeof raw.server_id !== 'string' ||
        !Array.isArray(raw.results) ||
        raw.results.some((r: unknown) => r == null || typeof r !== 'object')
      ) {
        break
      }
      const msg = raw as WsMessage & { type: 'network_probe_update' }
      window.dispatchEvent(
        new CustomEvent('network-probe-update', {
          detail: { server_id: msg.server_id, results: msg.results }
        })
      )
      break
    }
    case 'docker_update':
    case 'docker_event':
    case 'docker_availability_changed':
      handleDockerMessage(raw, queryClient)
      break
    default:
      break
  }
}

export function useServersWs(enabled = true): React.RefObject<WsClient | null> {
  const queryClient = useQueryClient()
  const wsRef = useRef<WsClient | null>(null)

  useEffect(() => {
    if (!enabled) {
      wsRef.current = null
      return
    }

    const ws = new WsClient('/api/ws/servers')
    wsRef.current = ws

    ws.onMessage((raw) => handleWsMessage(raw, queryClient))

    return () => {
      ws.close()
      wsRef.current = null
    }
  }, [enabled, queryClient])

  return wsRef
}
