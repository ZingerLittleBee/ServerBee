import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { WsClient } from '@/lib/ws-client'

interface ServerMetrics {
  capabilities?: number
  country_code: string | null
  cpu: number
  cpu_name: string | null
  disk_total: number
  disk_used: number
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

export function useServersWs(): void {
  const queryClient = useQueryClient()
  const wsRef = useRef<WsClient | null>(null)

  useEffect(() => {
    const ws = new WsClient('/api/ws/servers')
    wsRef.current = ws

    ws.onMessage((raw) => {
      const msg = raw as WsMessage

      switch (msg.type) {
        case 'full_sync': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], msg.servers)
          break
        }
        case 'update': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return msg.servers
            }
            return mergeServerUpdate(prev, msg.servers)
          })
          break
        }
        case 'server_online': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return prev
            }
            return setServerOnlineStatus(prev, msg.server_id, true)
          })
          break
        }
        case 'server_offline': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return prev
            }
            return setServerOnlineStatus(prev, msg.server_id, false)
          })
          break
        }
        case 'capabilities_changed': {
          const { server_id, capabilities } = msg
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
            prev?.map((s) => (s.id === server_id ? { ...s, capabilities } : s))
          )
          queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
            prev ? { ...prev, capabilities } : prev
          )
          queryClient.invalidateQueries({ queryKey: ['servers-list'] })
          break
        }
        case 'agent_info_updated': {
          const { server_id, protocol_version } = msg
          queryClient.setQueryData(['servers', server_id], (prev: Record<string, unknown> | undefined) =>
            prev ? { ...prev, protocol_version } : prev
          )
          queryClient.invalidateQueries({ queryKey: ['servers-list'] })
          break
        }
        default:
          break
      }
    })

    return () => {
      ws.close()
      wsRef.current = null
    }
  }, [queryClient])
}
