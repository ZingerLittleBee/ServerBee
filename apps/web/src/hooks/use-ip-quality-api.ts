import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type {
  CreateCustomServiceInput,
  IpQualitySetting,
  ServerIpQualityData,
  UnlockEventDto,
  UnlockService,
  UpdateServiceInput
} from '@/lib/ip-quality-types'

// ---------------------------------------------------------------------------
// Query hooks
// ---------------------------------------------------------------------------

export function useIpQualityServices() {
  return useQuery<UnlockService[]>({
    queryKey: ['ip-quality', 'services'],
    queryFn: () => api.get('/api/ip-quality/services')
  })
}

export function useIpQualitySetting() {
  return useQuery<IpQualitySetting>({
    queryKey: ['ip-quality', 'setting'],
    queryFn: () => api.get('/api/ip-quality/settings')
  })
}

export function useIpQualityOverview() {
  return useQuery<ServerIpQualityData[]>({
    queryKey: ['ip-quality', 'overview'],
    queryFn: () => api.get('/api/ip-quality/overview'),
    refetchInterval: 60_000
  })
}

export function useIpQualityServer(serverId: string) {
  return useQuery<ServerIpQualityData>({
    queryKey: ['ip-quality', 'servers', serverId],
    queryFn: () => api.get(`/api/ip-quality/servers/${serverId}`),
    enabled: serverId.length > 0,
    refetchInterval: 60_000
  })
}

export function useIpQualityEvents(serverId: string) {
  return useQuery<UnlockEventDto[]>({
    queryKey: ['ip-quality', 'events', serverId],
    queryFn: () => api.get(`/api/ip-quality/events?server_id=${encodeURIComponent(serverId)}`),
    enabled: serverId.length > 0
  })
}

// ---------------------------------------------------------------------------
// Mutation hooks
// ---------------------------------------------------------------------------

export function useCreateService() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateCustomServiceInput) => api.post<UnlockService>('/api/ip-quality/services', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ip-quality', 'services'] })
    }
  })
}

export function useUpdateService() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: { id: string } & UpdateServiceInput) =>
      api.put<UnlockService>(`/api/ip-quality/services/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ip-quality', 'services'] })
    }
  })
}

export function useDeleteService() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/ip-quality/services/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ip-quality', 'services'] })
    }
  })
}

export function useUpdateSetting() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: IpQualitySetting) => api.put<IpQualitySetting>('/api/ip-quality/settings', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['ip-quality', 'setting'] })
    }
  })
}

export function useCheckNow() {
  return useMutation({
    mutationFn: (serverId: string) => api.post(`/api/ip-quality/servers/${serverId}/check`)
  })
}
