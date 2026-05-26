import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { PublicStatusConfig } from '@/lib/api-schema'

export function usePublicStatusConfig() {
  return useQuery({
    queryKey: ['public-status', 'config'],
    queryFn: () => api.get<PublicStatusConfig>('/api/status/config'),
    refetchInterval: 5 * 60_000,
    staleTime: 5 * 60_000
  })
}
