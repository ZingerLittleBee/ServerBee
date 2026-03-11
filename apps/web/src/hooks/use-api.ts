import { useQuery } from '@tanstack/react-query'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'

type ServerDetail = ServerMetrics & {
  cpu_cores: number
  total_memory_gb: number
}

interface ServerRecord {
  cpu_usage: number
  disk_total: number
  disk_used: number
  load_avg: [number, number, number]
  memory_total: number
  memory_used: number
  network_in: number
  network_out: number
  timestamp: string
}

export function useServer(id: string) {
  return useQuery<ServerDetail>({
    queryKey: ['servers', id],
    queryFn: () => api.get<ServerDetail>(`/api/servers/${id}`),
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

export type { ServerDetail, ServerRecord }
