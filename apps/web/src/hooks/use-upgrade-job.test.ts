import { beforeEach, describe, expect, it } from 'vitest'
import { type UpgradeJob, useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'

describe('useUpgradeJob store integration', () => {
  beforeEach(() => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
  })

  function makeJob(overrides: Partial<UpgradeJob> = {}): UpgradeJob {
    return {
      server_id: 'server-1',
      job_id: 'job-1',
      target_version: '1.0.0',
      stage: 'downloading',
      status: 'running',
      error: null,
      backup_path: null,
      started_at: '2024-01-01T00:00:00Z',
      finished_at: null,
      ...overrides
    }
  }

  it('returns undefined when no job exists', () => {
    const job = useUpgradeJobsStore.getState().getJob('server-1')
    expect(job).toBeUndefined()
  })

  it('returns job from store when job exists', () => {
    const job = makeJob()
    useUpgradeJobsStore.getState().setJob('server-1', job)

    const retrieved = useUpgradeJobsStore.getState().getJob('server-1')
    expect(retrieved).toEqual(job)
  })

  it('updates job stage via store with different job_id', () => {
    const job = makeJob({ job_id: 'job-1', stage: 'downloading' })
    useUpgradeJobsStore.getState().setJob('server-1', job)

    const updatedJob = makeJob({ job_id: 'job-2', stage: 'installing' })
    useUpgradeJobsStore.getState().setJob('server-1', updatedJob)

    const retrieved = useUpgradeJobsStore.getState().getJob('server-1')
    expect(retrieved?.stage).toBe('installing')
  })

  it('updates job status via store with different job_id', () => {
    const job = makeJob({ job_id: 'job-1', status: 'running' })
    useUpgradeJobsStore.getState().setJob('server-1', job)

    const updatedJob = makeJob({ job_id: 'job-2', status: 'succeeded', finished_at: '2024-01-01T00:01:00Z' })
    useUpgradeJobsStore.getState().setJob('server-1', updatedJob)

    const retrieved = useUpgradeJobsStore.getState().getJob('server-1')
    expect(retrieved?.status).toBe('succeeded')
  })

  it('clears job via store', () => {
    const job = makeJob()
    useUpgradeJobsStore.getState().setJob('server-1', job)
    expect(useUpgradeJobsStore.getState().getJob('server-1')).toBeDefined()

    useUpgradeJobsStore.getState().clearJob('server-1')
    expect(useUpgradeJobsStore.getState().getJob('server-1')).toBeUndefined()
  })
})
