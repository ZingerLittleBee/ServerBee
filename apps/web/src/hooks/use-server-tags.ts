import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'

export function useServerTags(serverId: string, enabled = true) {
  return useQuery<string[]>({
    queryKey: ['server-tags', serverId],
    queryFn: () => api.get<string[]>(`/api/servers/${serverId}/tags`),
    enabled: enabled && !!serverId,
    staleTime: 60_000
  })
}

export function useUpdateServerTags(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation<string[], Error, string[]>({
    mutationFn: (tags) => api.put<string[]>(`/api/servers/${serverId}/tags`, { tags }),
    onSuccess: (data) => {
      queryClient.setQueryData<string[]>(['server-tags', serverId], data)
      queryClient.setQueryData<ServerMetrics[]>(['servers'], (prev) =>
        prev?.map((s) => (s.id === serverId ? { ...s, tags: data } : s))
      )
    }
  })
}
