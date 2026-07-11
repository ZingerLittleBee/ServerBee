import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it } from 'vitest'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { handleWsMessage } from './use-servers-ws'

describe('handleWsMessage upgrade messages', () => {
  it('hydrates upgrade jobs from full_sync', () => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
    const queryClient = new QueryClient()

    handleWsMessage(
      {
        type: 'full_sync',
        servers: [],
        upgrades: [
          {
            server_id: 'server-1',
            job_id: 'job-1',
            target_version: '1.2.3',
            stage: 'downloading',
            status: 'running',
            error: null,
            backup_path: null,
            started_at: '2024-01-01T00:00:00Z',
            finished_at: null
          }
        ]
      },
      queryClient
    )

    expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.job_id).toBe('job-1')
  })

  it('updates existing upgrade stage from upgrade_progress', () => {
    useUpgradeJobsStore.setState({
      jobs: new Map([
        [
          'server-1',
          {
            server_id: 'server-1',
            job_id: 'job-1',
            target_version: '1.2.3',
            stage: 'downloading',
            status: 'running',
            error: null,
            backup_path: null,
            started_at: '2024-01-01T00:00:00Z',
            finished_at: null
          }
        ]
      ])
    })
    const queryClient = new QueryClient()

    handleWsMessage(
      {
        type: 'upgrade_progress',
        server_id: 'server-1',
        job_id: 'job-1',
        target_version: '1.2.3',
        stage: 'installing'
      },
      queryClient
    )

    expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.stage).toBe('installing')
  })

  it('stores terminal upgrade result from upgrade_result', () => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
    const queryClient = new QueryClient()

    handleWsMessage(
      {
        type: 'upgrade_result',
        server_id: 'server-1',
        job_id: 'job-1',
        target_version: '1.2.3',
        status: 'failed',
        stage: 'installing',
        error: 'install failed',
        backup_path: '/tmp/backup'
      },
      queryClient
    )

    const job = useUpgradeJobsStore.getState().jobs.get('server-1')
    expect(job?.status).toBe('failed')
    expect(job?.error).toBe('install failed')
    expect(job?.backup_path).toBe('/tmp/backup')
    expect(job?.finished_at).not.toBeNull()
  })
})
