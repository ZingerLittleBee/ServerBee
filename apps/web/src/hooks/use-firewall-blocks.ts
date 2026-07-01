import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type {
  BlockListItem,
  BlockListResponse,
  CreateBlockReq,
  FirewallBlocksFilters,
  FirewallStats
} from '@/types/firewall'

function buildQuery(params: Record<string, unknown>): string {
  const out = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v === null || v === undefined || v === '') {
      continue
    }
    out.set(k, String(v))
  }
  const s = out.toString()
  return s ? `?${s}` : ''
}

export function useFirewallBlocks(filters: FirewallBlocksFilters = {}) {
  return useInfiniteQuery({
    queryKey: ['firewall', 'blocks', filters],
    initialPageParam: '' as string,
    queryFn: ({ pageParam }) => {
      const qs = buildQuery({ ...filters, cursor: pageParam || null })
      return api.get<BlockListResponse>(`/api/firewall/blocks${qs}`)
    },
    getNextPageParam: (last) => last.next_cursor ?? undefined
  })
}

export function useFirewallStats() {
  return useQuery({
    queryKey: ['firewall', 'stats'],
    queryFn: () => api.get<FirewallStats>('/api/firewall/stats')
  })
}

export function useCreateBlock() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateBlockReq) => api.post<BlockListItem>('/api/firewall/blocks', input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['firewall'] }).catch(() => undefined)
    }
  })
}

export function useDeleteBlock() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete<boolean>(`/api/firewall/blocks/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['firewall'] }).catch(() => undefined)
    }
  })
}
