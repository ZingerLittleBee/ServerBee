import { useMutation, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { UpgradeRequest } from '@/lib/api-schema'
import { invalidateServerDetail } from '@/lib/server-catalog'
import { type UpgradeJob, useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

interface UseUpgradeJobResult {
  isLoading: boolean
  job: UpgradeJob | null
  triggerUpgrade: (version: string) => void
}

export function useUpgradeJob(serverId: string): UseUpgradeJobResult {
  const queryClient = useQueryClient()
  const storeJob = useUpgradeJobsStore((state) => state.jobs.get(serverId))

  const mutation = useMutation({
    mutationFn: (version: string) => {
      const body: UpgradeRequest = { version }
      return api.post<void>(`/api/servers/${serverId}/upgrade`, body)
    },
    onSuccess: () => {
      invalidateServerDetail(queryClient, serverId).catch(() => undefined)
    }
  })

  const job = storeJob ?? null

  return {
    job,
    triggerUpgrade: mutation.mutate,
    isLoading: mutation.isPending
  }
}
