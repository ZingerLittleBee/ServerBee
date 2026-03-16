import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

function base64Decode(base64: string): string {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0))
  return new TextDecoder().decode(bytes)
}

function base64Encode(text: string): string {
  const bytes = new TextEncoder().encode(text)
  return btoa(Array.from(bytes, (b) => String.fromCharCode(b)).join(''))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FileEntry {
  file_type: 'Directory' | 'File' | 'Symlink'
  group: string | null
  modified: number
  name: string
  owner: string | null
  path: string
  permissions: string | null
  size: number
}

interface ListFilesResponse {
  entries: FileEntry[]
}

interface StatResponse {
  entry: FileEntry
}

interface ReadResponse {
  content: string
}

interface SuccessResponse {
  success: boolean
}

interface DownloadResponse {
  status: string
  transfer_id: string
}

export interface TransferInfo {
  bytes_transferred: number
  created_at_secs_ago: number
  direction: string
  file_path: string
  file_size: number | null
  server_id: number
  status: string
  transfer_id: string
}

interface TransfersResponse {
  transfers: TransferInfo[]
}

// ---------------------------------------------------------------------------
// Query hooks (read operations)
// ---------------------------------------------------------------------------

export function useFileList(serverId: string, path: string) {
  return useQuery<FileEntry[]>({
    queryKey: ['files', serverId, 'list', path],
    queryFn: async () => {
      const res = await api.post<ListFilesResponse>(`/api/files/${serverId}/list`, { path })
      return res.entries
    },
    enabled: serverId.length > 0
  })
}

export function useFileStat(serverId: string, path: string, enabled?: boolean) {
  return useQuery<FileEntry>({
    queryKey: ['files', serverId, 'stat', path],
    queryFn: async () => {
      const res = await api.post<StatResponse>(`/api/files/${serverId}/stat`, { path })
      return res.entry
    },
    enabled: serverId.length > 0 && (enabled ?? true)
  })
}

export function useFileRead(serverId: string, path: string, enabled?: boolean) {
  return useQuery<string>({
    queryKey: ['files', serverId, 'read', path],
    queryFn: async () => {
      const res = await api.post<ReadResponse>(`/api/files/${serverId}/read`, { path })
      return base64Decode(res.content)
    },
    enabled: serverId.length > 0 && (enabled ?? true)
  })
}

export function useFileTransfers() {
  return useQuery<TransferInfo[]>({
    queryKey: ['files', 'transfers'],
    queryFn: async () => {
      const res = await api.get<TransfersResponse>('/api/files/transfers')
      return res.transfers
    },
    refetchInterval: 3000
  })
}

// ---------------------------------------------------------------------------
// Mutation hooks (write operations)
// ---------------------------------------------------------------------------

export function useFileWriteMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { path: string; content: string }) =>
      api.post<SuccessResponse>(`/api/files/${serverId}/write`, {
        path: input.path,
        content: base64Encode(input.content)
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', serverId] })
    }
  })
}

export function useFileDeleteMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { path: string; recursive?: boolean }) =>
      api.post<SuccessResponse>(`/api/files/${serverId}/delete`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', serverId] })
    }
  })
}

export function useFileMkdirMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { path: string }) => api.post<SuccessResponse>(`/api/files/${serverId}/mkdir`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', serverId] })
    }
  })
}

export function useFileMoveMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { from: string; to: string }) =>
      api.post<SuccessResponse>(`/api/files/${serverId}/move`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', serverId] })
    }
  })
}

export function useStartDownloadMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { path: string }) => api.post<DownloadResponse>(`/api/files/${serverId}/download`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', 'transfers'] })
    }
  })
}

export function useUploadFileMutation(serverId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: { path: string; file: File }) => {
      const formData = new FormData()
      formData.append('path', input.path)
      formData.append('file', input.file)

      const response = await fetch(`/api/files/${serverId}/upload`, {
        method: 'POST',
        credentials: 'include',
        body: formData
      })

      if (!response.ok) {
        const text = await response.text().catch(() => response.statusText)
        throw new Error(text)
      }

      const json = await response.json()
      if (json && typeof json === 'object' && 'data' in json) {
        return json.data as SuccessResponse
      }
      return json as SuccessResponse
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', serverId] })
      queryClient.invalidateQueries({ queryKey: ['files', 'transfers'] })
    }
  })
}

export function useCancelTransferMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (transferId: string) => api.delete<SuccessResponse>(`/api/files/transfers/${transferId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['files', 'transfers'] })
    }
  })
}
