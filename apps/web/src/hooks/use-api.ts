import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { useServerDetail } from '@/lib/server-catalog'
import { isoWindow } from '@/lib/utils'

type ServerRecord = import('@/lib/api-schema').ServerMetricRecord

export function useServer(id: string) {
  return useServerDetail(id)
}

export function useServerRecords(id: string, hours: number, interval: string, options?: { enabled?: boolean }) {
  return useQuery<ServerRecord[]>({
    queryKey: ['servers', id, 'records', hours, interval],
    queryFn: () => {
      const { from, to } = isoWindow(hours)
      return api.get<ServerRecord[]>(
        `/api/servers/${id}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
      )
    },
    enabled: !!id && id.length > 0 && (options?.enabled ?? true),
    // Keep the prior window visible while the next range loads so charts can
    // morph (Bklit yDomainTween / path transition) instead of blanking out.
    placeholderData: keepPreviousData,
    refetchInterval: 60_000
  })
}

export function useUptimeDaily(serverId: string, days = 90) {
  return useQuery<UptimeDailyEntry[]>({
    queryKey: ['servers', serverId, 'uptime-daily', days],
    queryFn: () => api.get<UptimeDailyEntry[]>(`/api/servers/${serverId}/uptime-daily?days=${days}`),
    enabled: !!serverId && serverId.length > 0,
    staleTime: 300_000
  })
}

export type { ServerMetricRecord as ServerRecord } from '@/lib/api-schema'
