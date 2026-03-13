import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'

interface ServerRecord {
  cpu: number
  disk_used: number
  gpu_usage: number | null
  id: number
  load1: number
  load5: number
  load15: number
  mem_used: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  process_count: number
  server_id: string
  swap_used: number
  tcp_conn: number
  temperature: number | null
  time: string
  udp_conn: number
}

export function useServer(id: string) {
  return useQuery<ServerResponse>({
    queryKey: ['servers', id],
    queryFn: () => api.get<ServerResponse>(`/api/servers/${id}`),
    enabled: id.length > 0
  })
}

export function useServerRecords(id: string, from: string, to: string, interval: string) {
  return useQuery<ServerRecord[]>({
    queryKey: ['servers', id, 'records', from, to, interval],
    queryFn: () =>
      api.get<ServerRecord[]>(
        `/api/servers/${id}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
      ),
    enabled: id.length > 0
  })
}

export type { ServerRecord }
