import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { CostOverviewResponse, ServerCostInsights } from '@/lib/api-schema'

export function useCostOverview() {
  return useQuery<CostOverviewResponse>({
    queryKey: ['cost', 'overview'],
    queryFn: () => api.get<CostOverviewResponse>('/api/cost/overview'),
    staleTime: 60_000
  })
}

export function useCostInsights(serverId: string) {
  return useQuery<ServerCostInsights>({
    queryKey: ['servers', serverId, 'cost-insights'],
    queryFn: () => api.get<ServerCostInsights>(`/api/servers/${serverId}/cost-insights`),
    enabled: serverId.length > 0,
    staleTime: 60_000
  })
}
