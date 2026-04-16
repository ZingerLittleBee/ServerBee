import { create } from 'zustand'
import type { RecoveryJobResponse } from '@/lib/api-schema'

interface RecoveryJobsState {
  clearJob: (targetServerId: string) => void
  getJob: (targetServerId: string) => RecoveryJobResponse | undefined
  jobs: Map<string, RecoveryJobResponse>
  setJob: (targetServerId: string, job: RecoveryJobResponse) => void
  setJobs: (jobs: RecoveryJobResponse[]) => void
}

export const useRecoveryJobsStore = create<RecoveryJobsState>()((set, get) => ({
  jobs: new Map(),

  setJob: (targetServerId: string, job: RecoveryJobResponse) => {
    set((state) => {
      const next = new Map(state.jobs)
      next.set(targetServerId, job)
      return { jobs: next }
    })
  },

  clearJob: (targetServerId: string) => {
    set((state) => {
      const next = new Map(state.jobs)
      next.delete(targetServerId)
      return { jobs: next }
    })
  },

  setJobs: (jobs: RecoveryJobResponse[]) => {
    const next = new Map<string, RecoveryJobResponse>()
    for (const job of jobs) {
      next.set(job.target_server_id, job)
    }
    set({ jobs: next })
  },

  getJob: (targetServerId: string) => get().jobs.get(targetServerId)
}))
