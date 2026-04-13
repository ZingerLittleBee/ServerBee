import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface TrafficOverviewItem {
  billing_cycle: string | null
  cycle_in: number
  cycle_out: number
  days_remaining: number | null
  name: string
  percent_used: number | null
  server_id: string
  traffic_limit: number | null
}

export function useTrafficOverview() {
  return useQuery<TrafficOverviewItem[]>({
    queryKey: ['traffic', 'overview'],
    queryFn: () => api.get<TrafficOverviewItem[]>('/api/traffic/overview'),
    staleTime: 60_000
  })
}
