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

/**
 * Structured error envelope returned by the SPA theme endpoints.
 *
 * The server responds with `{ error: { code, message, details } }` for known failure modes;
 * we surface those fields so callers can render `t(\`errors.${code}\`, details)`.
 */
export interface ApiError extends Error {
  code?: string
  details?: Record<string, unknown>
}

interface ErrorEnvelope {
  error?: { code?: string; message?: string; details?: Record<string, unknown> }
}

async function parseErrorBody(res: Response): Promise<ApiError> {
  const text = await res.text().catch(() => '')
  let parsed: unknown = null
  try {
    parsed = text ? JSON.parse(text) : null
  } catch {
    parsed = null
  }
  const envelope = (parsed as ErrorEnvelope | null)?.error
  const err = new Error(envelope?.message ?? text ?? res.statusText) as ApiError
  err.code = envelope?.code
  err.details = envelope?.details
  return err
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
  return useMutation<void, ApiError, string>({
    mutationFn: async (uuid: string) => {
      const res = await fetch(`/api/settings/spa-themes/${uuid}`, { method: 'DELETE', credentials: 'include' })
      if (!res.ok) {
        throw await parseErrorBody(res)
      }
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] })
  })
}

export function useUploadSpaTheme() {
  const qc = useQueryClient()
  return useMutation<UploadResult, ApiError, File>({
    mutationFn: async (file: File) => {
      const fd = new FormData()
      fd.append('package', file)
      const res = await fetch('/api/settings/spa-themes', {
        method: 'POST',
        credentials: 'include',
        body: fd
      })
      if (!res.ok) {
        throw await parseErrorBody(res)
      }
      const text = await res.text()
      const parsed = text ? (JSON.parse(text) as { data: UploadResult }) : null
      if (!parsed) {
        throw new Error('Upload succeeded but response body was empty')
      }
      return parsed.data
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] })
  })
}
