import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useUpgradeJobsStore } from './upgrade-jobs-store'

describe('useUpgradeJobsStore', () => {
  beforeEach(() => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.runAllTimers()
    vi.useRealTimers()
  })

  function makeJob(
    overrides: Partial<{
      server_id: string
      job_id: string
      target_version: string
      stage: string
      status: string
      error: string | null
      backup_path: string | null
      started_at: string
      finished_at: string | null
    }> = {}
  ) {
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

  describe('setJob', () => {
    it('adds a new job to the store', () => {
      const job = makeJob()
      useUpgradeJobsStore.getState().setJob('server-1', job)

      const storedJob = useUpgradeJobsStore.getState().jobs.get('server-1')
      expect(storedJob).toEqual(job)
    })

    it('updates existing job with different job_id', () => {
      const job1 = makeJob({ job_id: 'job-1', target_version: '1.0.0' })
      useUpgradeJobsStore.getState().setJob('server-1', job1)

      const job2 = makeJob({ job_id: 'job-2', target_version: '2.0.0' })
      useUpgradeJobsStore.getState().setJob('server-1', job2)

      const storedJob = useUpgradeJobsStore.getState().jobs.get('server-1')
      expect(storedJob?.job_id).toBe('job-2')
      expect(storedJob?.target_version).toBe('2.0.0')
    })

    it('updates existing job when incoming job_id matches', () => {
      const job1 = makeJob({ job_id: 'job-1', target_version: '1.0.0' })
      useUpgradeJobsStore.getState().setJob('server-1', job1)

      const job2 = makeJob({ job_id: 'job-1', target_version: '2.0.0', stage: 'installing' })
      useUpgradeJobsStore.getState().setJob('server-1', job2)

      const storedJob = useUpgradeJobsStore.getState().jobs.get('server-1')
      expect(storedJob?.target_version).toBe('2.0.0')
      expect(storedJob?.stage).toBe('installing')
    })

    it('stores jobs keyed by server_id', () => {
      const job1 = makeJob({ server_id: 'server-1', job_id: 'job-1' })
      const job2 = makeJob({ server_id: 'server-2', job_id: 'job-2' })

      useUpgradeJobsStore.getState().setJob('server-1', job1)
      useUpgradeJobsStore.getState().setJob('server-2', job2)

      expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.job_id).toBe('job-1')
      expect(useUpgradeJobsStore.getState().jobs.get('server-2')?.job_id).toBe('job-2')
    })
  })

  describe('clearJob', () => {
    it('removes job for specific server', () => {
      const job = makeJob()
      useUpgradeJobsStore.getState().setJob('server-1', job)

      useUpgradeJobsStore.getState().clearJob('server-1')

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(false)
    })

    it('does nothing for non-existent server', () => {
      const job = makeJob()
      useUpgradeJobsStore.getState().setJob('server-1', job)

      useUpgradeJobsStore.getState().clearJob('server-nonexistent')

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(true)
    })
  })

  describe('setJobs', () => {
    it('batch updates multiple jobs', () => {
      const jobs = [
        makeJob({ server_id: 'server-1', job_id: 'job-1' }),
        makeJob({ server_id: 'server-2', job_id: 'job-2' })
      ]

      useUpgradeJobsStore.getState().setJobs(jobs)

      expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.job_id).toBe('job-1')
      expect(useUpgradeJobsStore.getState().jobs.get('server-2')?.job_id).toBe('job-2')
    })

    it('replaces existing jobs with new batch', () => {
      // Set initial job
      useUpgradeJobsStore.getState().setJob('server-1', makeJob({ server_id: 'server-1', job_id: 'job-old' }))

      // Batch update with new jobs
      const jobs = [makeJob({ server_id: 'server-2', job_id: 'job-new' })]
      useUpgradeJobsStore.getState().setJobs(jobs)

      // server-1 job should be replaced (cleared) by the batch
      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(false)
      expect(useUpgradeJobsStore.getState().jobs.get('server-2')?.job_id).toBe('job-new')
    })
  })

  describe('auto-clear finished jobs', () => {
    it('auto-clears succeeded jobs after 5 seconds', () => {
      const job = makeJob({ status: 'succeeded', finished_at: '2024-01-01T00:01:00Z' })
      useUpgradeJobsStore.getState().setJob('server-1', job)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(true)

      vi.advanceTimersByTime(5000)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(false)
    })

    it('auto-clears failed jobs after 5 seconds', () => {
      const job = makeJob({ status: 'failed', error: 'Download failed', finished_at: '2024-01-01T00:01:00Z' })
      useUpgradeJobsStore.getState().setJob('server-1', job)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(true)

      vi.advanceTimersByTime(5000)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(false)
    })

    it('does not auto-clear running jobs', () => {
      const job = makeJob({ status: 'running' })
      useUpgradeJobsStore.getState().setJob('server-1', job)

      vi.advanceTimersByTime(5000)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(true)
    })

    it('does not auto-clear timeout jobs', () => {
      const job = makeJob({ status: 'timeout', finished_at: '2024-01-01T00:01:00Z' })
      useUpgradeJobsStore.getState().setJob('server-1', job)

      vi.advanceTimersByTime(5000)

      expect(useUpgradeJobsStore.getState().jobs.has('server-1')).toBe(true)
    })

    it('does not let an old finished-job timer clear a newer running job', () => {
      const finishedJob = makeJob({ job_id: 'job-1', status: 'succeeded', finished_at: '2024-01-01T00:01:00Z' })
      useUpgradeJobsStore.getState().setJob('server-1', finishedJob)

      const runningJob = makeJob({ job_id: 'job-2', status: 'running', finished_at: null })
      useUpgradeJobsStore.getState().setJob('server-1', runningJob)

      vi.advanceTimersByTime(5000)

      expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.job_id).toBe('job-2')
    })
  })

  describe('getJob', () => {
    it('returns job for specific server', () => {
      const job = makeJob()
      useUpgradeJobsStore.getState().setJob('server-1', job)

      const retrieved = useUpgradeJobsStore.getState().getJob('server-1')
      expect(retrieved).toEqual(job)
    })

    it('returns undefined for non-existent server', () => {
      const retrieved = useUpgradeJobsStore.getState().getJob('server-nonexistent')
      expect(retrieved).toBeUndefined()
    })
  })
})
