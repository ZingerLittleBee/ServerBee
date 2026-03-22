import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api } from '@/lib/api-client'
import type { Dashboard, DashboardWithWidgets } from '@/lib/widget-types'

export interface CreateDashboardInput {
  is_default?: boolean
  name: string
}

export interface WidgetInput {
  config_json: unknown
  grid_h: number
  grid_w: number
  grid_x: number
  grid_y: number
  id?: string
  sort_order: number
  title?: string | null
  widget_type: string
}

export interface UpdateDashboardInput {
  is_default?: boolean
  name?: string
  sort_order?: number
  widgets?: WidgetInput[]
}

export function useDashboards() {
  return useQuery<Dashboard[]>({
    queryKey: ['dashboards'],
    queryFn: () => api.get<Dashboard[]>('/api/dashboards'),
    staleTime: 30_000
  })
}

export function useDefaultDashboard() {
  const queryClient = useQueryClient()
  return useQuery<DashboardWithWidgets>({
    queryKey: ['dashboards', 'default'],
    queryFn: async () => {
      const data = await api.get<DashboardWithWidgets>('/api/dashboards/default')
      // Seed the per-id cache so useDashboard(id) won't re-fetch the same data
      queryClient.setQueryData(['dashboards', data.id], data)
      return data
    },
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
      api.put<DashboardWithWidgets>(`/api/dashboards/${id}`, input),
    onSuccess: updated => {
      queryClient.setQueryData(['dashboards', updated.id], updated)
      if (updated.is_default) {
        queryClient.setQueryData(['dashboards', 'default'], updated)
      }
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
