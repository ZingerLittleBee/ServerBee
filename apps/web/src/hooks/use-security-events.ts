import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { SecurityEventList, StatsBucket } from '@/lib/api-schema'

export interface SecurityEventFilters {
  event_type?: string | null
  limit?: number | null
  server_id?: string | null
  severity?: string | null
  since?: string | null
  source_ip?: string | null
  until?: string | null
}

function buildQuery(params: Record<string, unknown>): string {
  const out = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === null || v === undefined || v === '') {
      continue
    }
    out.set(k, String(v))
  }
  const s = out.toString()
  return s ? `?${s}` : ''
}

export function useSecurityEvents(filters: SecurityEventFilters) {
  return useInfiniteQuery({
    queryKey: ['security', 'events', filters],
    initialPageParam: '' as string,
    queryFn: ({ pageParam }) => {
      const qs = buildQuery({ ...filters, cursor: pageParam || null })
      return api.get<SecurityEventList>(`/api/security/events${qs}`)
    },
    getNextPageParam: (last) => last.next_cursor ?? undefined
  })
}

export interface SecurityStatsFilters {
  group_by?: 'event_type' | 'source_ip' | 'day' | null
  limit?: number | null
  server_id?: string | null
  since?: string | null
  until?: string | null
}

export function useSecurityStats(filters: SecurityStatsFilters) {
  return useQuery({
    queryKey: ['security', 'stats', filters],
    queryFn: () => {
      const qs = buildQuery(filters)
      return api.get<StatsBucket[]>(`/api/security/stats${qs}`)
    }
  })
}
