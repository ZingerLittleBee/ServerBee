import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface TrafficData {
  bytes_in: number
  bytes_out: number
  bytes_total: number
  cycle_end: string
  cycle_start: string
  daily: Array<{ date: string; bytes_in: number; bytes_out: number }>
  hourly: Array<{ hour: string; bytes_in: number; bytes_out: number }>
  prediction: {
    estimated_total: number
    estimated_percent: number
    will_exceed: boolean
  } | null
  traffic_limit: number | null
  traffic_limit_type: string | null
  usage_percent: number | null
}

export function useTraffic(serverId: string) {
  return useQuery<TrafficData>({
    queryKey: ['servers', serverId, 'traffic'],
    queryFn: () => api.get<TrafficData>(`/api/servers/${serverId}/traffic`),
    staleTime: 60_000,
    enabled: !!serverId
  })
}
