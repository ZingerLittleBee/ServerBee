import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type {
  NetworkProbeAnomaly,
  NetworkProbeRecord,
  NetworkProbeSetting,
  NetworkProbeTarget,
  NetworkServerSummary
} from '@/lib/network-types'

export function useNetworkTargets() {
  return useQuery<NetworkProbeTarget[]>({
    queryKey: ['network-probes', 'targets'],
    queryFn: () => api.get('/api/network-probes/targets')
  })
}

export function useNetworkSetting() {
  return useQuery<NetworkProbeSetting>({
    queryKey: ['network-probes', 'setting'],
    queryFn: () => api.get('/api/network-probes/setting')
  })
}

export function useNetworkOverview() {
  return useQuery<NetworkServerSummary[]>({
    queryKey: ['network-probes', 'overview'],
    queryFn: () => api.get('/api/network-probes/overview'),
    refetchInterval: 60_000
  })
}

export function useNetworkServerSummary(serverId: string) {
  return useQuery<NetworkServerSummary>({
    queryKey: ['servers', serverId, 'network-probes', 'summary'],
    queryFn: () => api.get(`/api/servers/${serverId}/network-probes/summary`),
    enabled: serverId.length > 0
  })
}

export function useNetworkRecords(serverId: string, hours: number, options?: { targetId?: string; enabled?: boolean }) {
  return useQuery<NetworkProbeRecord[]>({
    queryKey: ['servers', serverId, 'network-probes', 'records', hours, options?.targetId],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      let url = `/api/servers/${serverId}/network-probes/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      if (options?.targetId) {
        url += `&target_id=${encodeURIComponent(options.targetId)}`
      }
      return api.get(url)
    },
    enabled: serverId.length > 0 && (options?.enabled ?? true),
    refetchInterval: 60_000
  })
}

export function useNetworkAnomalies(serverId: string, hours: number) {
  return useQuery<NetworkProbeAnomaly[]>({
    queryKey: ['servers', serverId, 'network-probes', 'anomalies', hours],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      return api.get(
        `/api/servers/${serverId}/network-probes/anomalies?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`
      )
    },
    enabled: serverId.length > 0
  })
}

export function useCreateTarget() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { name: string; provider: string; location: string; target: string; probe_type: string }) =>
      api.post('/api/network-probes/targets', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['network-probes', 'targets'] })
    }
  })
}

export function useUpdateTarget() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({
      id,
      ...input
    }: {
      id: string
      name: string
      provider: string
      location: string
      target: string
      probe_type: string
    }) => api.put(`/api/network-probes/targets/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['network-probes', 'targets'] })
    }
  })
}

export function useDeleteTarget() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/network-probes/targets/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['network-probes', 'targets'] })
    }
  })
}

export function useUpdateNetworkSetting() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: NetworkProbeSetting) => api.put('/api/network-probes/setting', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['network-probes', 'setting'] })
    }
  })
}

export function useSetServerTargets(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (targetIds: string[]) =>
      api.put(`/api/servers/${serverId}/network-probes/targets`, { target_ids: targetIds }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['servers', serverId, 'network-probes'] })
    }
  })
}
