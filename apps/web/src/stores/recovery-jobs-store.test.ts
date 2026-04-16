import { describe, expect, it } from 'vitest'
import { useRecoveryJobsStore } from './recovery-jobs-store'

function makeJob(overrides: Partial<ReturnType<typeof buildJob>> = {}) {
  return {
    ...buildJob(),
    ...overrides
  }
}

function buildJob() {
  return {
    job_id: 'job-1',
    target_server_id: 'target-1',
    source_server_id: 'source-1',
    status: 'running' as const,
    stage: 'rebinding' as const,
    error: null,
    started_at: '2026-04-16T00:00:00Z',
    created_at: '2026-04-16T00:00:00Z',
    updated_at: '2026-04-16T00:00:00Z',
    last_heartbeat_at: null
  }
}

describe('useRecoveryJobsStore', () => {
  it('stores jobs keyed by target server id', () => {
    useRecoveryJobsStore.setState({ jobs: new Map() })

    useRecoveryJobsStore.getState().setJob('target-1', makeJob())

    expect(useRecoveryJobsStore.getState().getJob('target-1')?.job_id).toBe('job-1')
  })

  it('replaces the whole map on setJobs', () => {
    useRecoveryJobsStore.setState({ jobs: new Map() })
    useRecoveryJobsStore.getState().setJob('old-target', makeJob({ target_server_id: 'old-target' }))

    useRecoveryJobsStore
      .getState()
      .setJobs([makeJob(), makeJob({ job_id: 'job-2', target_server_id: 'target-2', source_server_id: 'source-2' })])

    expect(useRecoveryJobsStore.getState().getJob('old-target')).toBeUndefined()
    expect(useRecoveryJobsStore.getState().getJob('target-2')?.job_id).toBe('job-2')
  })

  it('clears a job by target server id', () => {
    useRecoveryJobsStore.setState({ jobs: new Map() })
    useRecoveryJobsStore.getState().setJob('target-1', makeJob())

    useRecoveryJobsStore.getState().clearJob('target-1')

    expect(useRecoveryJobsStore.getState().getJob('target-1')).toBeUndefined()
  })
})
