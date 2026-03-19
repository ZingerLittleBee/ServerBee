import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { ServerResponse } from '@/lib/api-schema'

type ServerRecord = import('@/lib/api-schema').ServerMetricRecord

export function useServer(id: string) {
  return useQuery<ServerResponse>({
    queryKey: ['servers', id],
    queryFn: () => api.get<ServerResponse>(`/api/servers/${id}`),
    enabled: id.length > 0
  })
}

export function useServerRecords(id: string, hours: number, interval: string, options?: { enabled?: boolean }) {
  return useQuery<ServerRecord[]>({
    queryKey: ['servers', id, 'records', hours, interval],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      return api.get<ServerRecord[]>(
        `/api/servers/${id}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
      )
    },
    enabled: id.length > 0 && (options?.enabled ?? true),
    refetchInterval: 60_000
  })
}

export type { ServerMetricRecord as ServerRecord } from '@/lib/api-schema'
