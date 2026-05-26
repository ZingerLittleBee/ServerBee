import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface SpaThemeSummary {
  author?: string | null
  description?: string | null
  has_preview: boolean
  is_active: boolean
  is_superseded: boolean
  manifest_id: string
  name: string
  size_bytes: number
  uploaded_at: string
  uploaded_by: string
  uuid: string
  version: string
}

export interface UploadResult {
  is_upgrade_of: { previous_uuid: string; previous_version: string } | null
  manifest: Record<string, unknown>
  preview_url: string | null
  size_bytes: number
  uuid: string
}

export function useSpaThemes() {
  return useQuery<SpaThemeSummary[]>({
    queryKey: ['spa-themes'],
    queryFn: () => api.get<SpaThemeSummary[]>('/api/settings/spa-themes')
  })
}

export function useActiveSpaTheme() {
  return useQuery<{ theme_id: string | null }>({
    queryKey: ['active-spa-theme'],
    queryFn: () => api.get('/api/settings/active-spa-theme'),
    staleTime: 30_000
  })
}

export function useActivateSpaTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (themeId: string | null) =>
      api.put<{ theme_id: string | null }>('/api/settings/active-spa-theme', { theme_id: themeId }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['active-spa-theme'] })
      qc.invalidateQueries({ queryKey: ['spa-themes'] })
    }
  })
}

export function useDeleteSpaTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (uuid: string) => {
      const res = await fetch(`/api/settings/spa-themes/${uuid}`, { method: 'DELETE', credentials: 'include' })
      if (!res.ok) {
        const text = await res.text().catch(() => '')
        throw new Error(text || res.statusText)
      }
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] })
  })
}

export interface UploadError extends Error {
  code?: string
  details?: Record<string, unknown>
}

export function useUploadSpaTheme() {
  const qc = useQueryClient()
  return useMutation<UploadResult, UploadError, File>({
    mutationFn: async (file: File) => {
      const fd = new FormData()
      fd.append('package', file)
      const res = await fetch('/api/settings/spa-themes', {
        method: 'POST',
        credentials: 'include',
        body: fd
      })
      const text = await res.text()
      let parsed: unknown
      try {
        parsed = JSON.parse(text)
      } catch {
        parsed = null
      }
      if (!res.ok) {
        const err = new Error((parsed as { error?: { message?: string } })?.error?.message ?? text) as UploadError
        err.code = (parsed as { error?: { code?: string } })?.error?.code
        err.details = (parsed as { error?: { details?: Record<string, unknown> } })?.error?.details
        throw err
      }
      return (parsed as { data: UploadResult }).data
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] })
  })
}
