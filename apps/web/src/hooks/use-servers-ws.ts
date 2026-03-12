import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { WsClient } from '@/lib/ws-client'

interface ServerMetrics {
  country_code: string | null
  cpu: number
  cpu_name: string | null
  disk_total: number
  disk_used: number
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

export type { ServerMetrics }

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
            const updated = [...prev]
            for (const incoming of msg.servers) {
              const idx = updated.findIndex((s) => s.id === incoming.id)
              if (idx >= 0) {
                updated[idx] = { ...updated[idx], ...incoming }
              }
            }
            return updated
          })
          break
        }
        case 'server_online': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return prev
            }
            return prev.map((s) => (s.id === msg.server_id ? { ...s, online: true } : s))
          })
          break
        }
        case 'server_offline': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return prev
            }
            return prev.map((s) => (s.id === msg.server_id ? { ...s, online: false } : s))
          })
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
