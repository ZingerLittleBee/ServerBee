import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { components } from '@/lib/api-types'

export type ThemeResolved = components['schemas']['ThemeResolved']
export type ActiveThemeResponse = components['schemas']['ActiveThemeResponse']
export type ThemeSummary = components['schemas']['ThemeSummary']

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
