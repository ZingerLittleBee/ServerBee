import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { components } from '@/lib/api-types'

export type ThemeResolved = components['schemas']['ThemeResolved']
export type ActiveThemeResponse = components['schemas']['ActiveThemeResponse']
export type CreateThemeInput = components['schemas']['CreateThemeInput']
export type ExportPayload = components['schemas']['ExportPayload']
export type FullTheme = components['schemas']['Theme']
export type ThemeReferences = components['schemas']['ThemeReferences']
export type ThemeSummary = components['schemas']['ThemeSummary']
export type UpdateThemeInput = components['schemas']['UpdateThemeInput']

export function useActiveTheme() {
  return useQuery<ActiveThemeResponse>({
    queryKey: ['active-theme'],
    queryFn: () => api.get<ActiveThemeResponse>('/api/settings/active-theme'),
    staleTime: 30_000
  })
}

export function useSetActiveTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (ref: string) => api.put<ActiveThemeResponse>('/api/settings/active-theme', { ref }),
    onSuccess: (data) => {
      queryClient.setQueryData(['active-theme'], data)
      queryClient.invalidateQueries({ queryKey: ['active-theme'] }).catch(() => undefined)
    }
  })
}

export function useCustomThemes() {
  return useQuery<ThemeSummary[]>({
    queryKey: ['themes'],
    queryFn: () => api.get<ThemeSummary[]>('/api/settings/themes')
  })
}

export function useThemeQuery(id: number) {
  return useQuery<FullTheme>({
    queryKey: ['themes', id],
    queryFn: () => api.get<FullTheme>(`/api/settings/themes/${id}`),
    enabled: Number.isInteger(id) && id > 0
  })
}

export function useCreateTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateThemeInput) => api.post<FullTheme>('/api/settings/themes', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['themes'] }).catch(() => undefined)
    }
  })
}

export function useUpdateTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, body }: { body: UpdateThemeInput; id: number }) =>
      api.put<FullTheme>(`/api/settings/themes/${id}`, body),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: ['themes'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['themes', variables.id] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['active-theme'] }).catch(() => undefined)
    }
  })
}

export function useThemeReferences(id: number) {
  return useQuery<ThemeReferences>({
    queryKey: ['themes', id, 'references'],
    queryFn: () => api.get<ThemeReferences>(`/api/settings/themes/${id}/references`),
    enabled: Number.isInteger(id) && id > 0
  })
}

export function useDeleteTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: number) => api.delete<string>(`/api/settings/themes/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['themes'] }).catch(() => undefined)
      queryClient.invalidateQueries({ queryKey: ['active-theme'] }).catch(() => undefined)
    }
  })
}

export function useDuplicateTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: number) => api.post<FullTheme>(`/api/settings/themes/${id}/duplicate`, {}),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['themes'] }).catch(() => undefined)
    }
  })
}

export function useExportTheme(id: number) {
  return useQuery<ExportPayload>({
    queryKey: ['themes', id, 'export'],
    queryFn: () => api.get<ExportPayload>(`/api/settings/themes/${id}/export`),
    enabled: false
  })
}

export function useImportTheme() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (payload: ExportPayload) => api.post<FullTheme>('/api/settings/themes/import', payload),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['themes'] }).catch(() => undefined)
    }
  })
}
