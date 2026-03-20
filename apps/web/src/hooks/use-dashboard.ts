import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'

export interface CreateDashboardInput {
  is_default?: boolean
  name: string
}

export interface UpdateDashboardInput {
  is_default?: boolean
  name?: string
  sort_order?: number
}

export function useDashboards() {
  return useQuery<Dashboard[]>({
    queryKey: ['dashboards'],
    queryFn: () => api.get<Dashboard[]>('/api/dashboards'),
    staleTime: 30_000
  })
}

export function useDefaultDashboard() {
  return useQuery<DashboardWithWidgets>({
    queryKey: ['dashboards', 'default'],
    queryFn: () => api.get<DashboardWithWidgets>('/api/dashboards/default'),
    staleTime: 30_000
  })
}

export function useDashboard(id: string) {
  return useQuery<DashboardWithWidgets>({
    queryKey: ['dashboards', id],
    queryFn: () => api.get<DashboardWithWidgets>(`/api/dashboards/${id}`),
    enabled: !!id,
    staleTime: 30_000
  })
}

export function useCreateDashboard() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateDashboardInput) => api.post<Dashboard>('/api/dashboards', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboards'] })
      toast.success('Dashboard created')
    }
  })
}

export function useUpdateDashboard() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: { id: string } & UpdateDashboardInput) =>
      api.put<Dashboard>(`/api/dashboards/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboards'] })
      toast.success('Dashboard updated')
    }
  })
}

export function useDeleteDashboard() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/dashboards/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dashboards'] })
      toast.success('Dashboard deleted')
    },
    onError: () => {
      toast.error('Failed to delete dashboard')
    }
  })
}
