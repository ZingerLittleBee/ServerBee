import { create } from 'zustand'

export type UpgradeStage = 'downloading' | 'verifying' | 'pre_flight' | 'installing' | 'restarting'

export type UpgradeStatus = 'running' | 'succeeded' | 'failed' | 'timeout'

export interface UpgradeJob {
  backup_path: string | null
  error: string | null
  finished_at: string | null
  job_id: string
  server_id: string
  stage: UpgradeStage
  started_at: string
  status: UpgradeStatus
  target_version: string
}

interface UpgradeJobsState {
  clearJob: (serverId: string) => void
  getJob: (serverId: string) => UpgradeJob | undefined
  jobs: Map<string, UpgradeJob>
  setJob: (serverId: string, job: UpgradeJob) => void
  setJobs: (jobs: UpgradeJob[]) => void
}

const AUTO_CLEAR_DELAY = 5000

function isFinished(status: UpgradeStatus): boolean {
  return status === 'succeeded' || status === 'failed'
}

export const useUpgradeJobsStore = create<UpgradeJobsState>()((set, get) => ({
  jobs: new Map(),

  setJob: (serverId: string, job: UpgradeJob) => {
    set((state) => {
      const newJobs = new Map(state.jobs)
      newJobs.set(serverId, job)

      if (isFinished(job.status)) {
        setTimeout(() => {
          const currentJob = get().getJob(serverId)
          if (currentJob?.job_id === job.job_id) {
            get().clearJob(serverId)
          }
        }, AUTO_CLEAR_DELAY)
      }

      return { jobs: newJobs }
    })
  },

  clearJob: (serverId: string) => {
    set((state) => {
      const newJobs = new Map(state.jobs)
      newJobs.delete(serverId)
      return { jobs: newJobs }
    })
  },

  setJobs: (jobs: UpgradeJob[]) => {
    const newJobs = new Map<string, UpgradeJob>()
    for (const job of jobs) {
      newJobs.set(job.server_id, job)

      if (isFinished(job.status)) {
        setTimeout(() => {
          const currentJob = get().getJob(job.server_id)
          if (currentJob?.job_id === job.job_id) {
            get().clearJob(job.server_id)
          }
        }, AUTO_CLEAR_DELAY)
      }
    }
    set({ jobs: newJobs })
  },

  getJob: (serverId: string) => {
    return get().jobs.get(serverId)
  }
}))
