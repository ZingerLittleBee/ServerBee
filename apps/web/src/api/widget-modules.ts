import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface ModuleSummary {
  code_sha256: string
  enabled: boolean
  entry_path: string
  id: string
  manifest: Record<string, unknown>
  source_type: string
  version: string
}

export interface ModuleInstallResult {
  id: string
  version: string
}

const LIST_KEY = ['widget-modules'] as const

export function useWidgetModules() {
  return useQuery<ModuleSummary[]>({
    queryKey: LIST_KEY,
    queryFn: () => api.get<ModuleSummary[]>('/api/widget-modules')
  })
}

async function readError(res: Response): Promise<string> {
  const text = await res.text().catch(() => '')
  if (text) {
    try {
      const parsed = JSON.parse(text)
      if (parsed && typeof parsed === 'object' && 'error' in parsed) {
        const err = (parsed as { error?: { message?: string } }).error
        if (err?.message) {
          return err.message
        }
      }
    } catch {
      // not JSON; fall through and use raw text
    }
    return text
  }
  return `request failed: ${res.status}`
}

export function useInstallFromUrl() {
  const qc = useQueryClient()
  return useMutation<ModuleInstallResult, Error, string>({
    mutationFn: async (url) => {
      const res = await fetch(`/api/widget-modules?url=${encodeURIComponent(url)}`, {
        method: 'POST',
        credentials: 'include'
      })
      if (!res.ok) {
        throw new Error(await readError(res))
      }
      const json = await res.json()
      return json.data as ModuleInstallResult
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: LIST_KEY }).catch(() => undefined)
    }
  })
}

export function useInstallFromFile() {
  const qc = useQueryClient()
  return useMutation<ModuleInstallResult, Error, File>({
    mutationFn: async (file) => {
      const fd = new FormData()
      fd.append('file', file)
      const res = await fetch('/api/widget-modules', {
        method: 'POST',
        credentials: 'include',
        body: fd
      })
      if (!res.ok) {
        throw new Error(await readError(res))
      }
      const json = await res.json()
      return json.data as ModuleInstallResult
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: LIST_KEY }).catch(() => undefined)
    }
  })
}

export function useUninstallWidgetModule() {
  const qc = useQueryClient()
  return useMutation<void, Error, string>({
    mutationFn: (id) => api.delete<void>(`/api/widget-modules/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: LIST_KEY }).catch(() => undefined)
    }
  })
}
