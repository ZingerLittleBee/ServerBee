import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { WsClient } from '@/lib/ws-client'

interface ServerMetrics {
  cpu_name: string
  cpu_usage: number
  disk_total: number
  disk_used: number
  id: string
  ip: string
  load_avg: [number, number, number]
  memory_total: number
  memory_used: number
  name: string
  network_in_speed: number
  network_out_speed: number
  online: boolean
  os: string
  uptime: number
}

type WsMessage =
  | { type: 'full_sync'; servers: ServerMetrics[] }
  | { type: 'update'; server: ServerMetrics }
  | { type: 'server_online'; server: ServerMetrics }
  | { type: 'server_offline'; id: string }

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
              return [msg.server]
            }
            return prev.map((s) => (s.id === msg.server.id ? msg.server : s))
          })
          break
        }
        case 'server_online': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return [msg.server]
            }
            const exists = prev.some((s) => s.id === msg.server.id)
            if (exists) {
              return prev.map((s) => (s.id === msg.server.id ? msg.server : s))
            }
            return [...prev, msg.server]
          })
          break
        }
        case 'server_offline': {
          queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) => {
            if (!prev) {
              return prev
            }
            return prev.map((s) => (s.id === msg.id ? { ...s, online: false } : s))
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
