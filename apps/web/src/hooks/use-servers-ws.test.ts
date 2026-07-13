import { QueryClient } from '@tanstack/react-query'
import { describe, expect, it } from 'vitest'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { handleWsMessage, subscribeBrowserMessage } from './use-servers-ws'

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

describe('handleWsMessage network probe updates', () => {
  it('dispatches validated network_probe_update frames to subscribers', () => {
    const queryClient = new QueryClient()
    const received: Record<string, unknown>[] = []
    const unsubscribe = subscribeBrowserMessage('network_probe_update', (msg) => received.push(msg))

    handleWsMessage(
      {
        type: 'network_probe_update',
        server_id: 's1',
        results: [{ latency_ms: 12, target_id: 't1' }]
      },
      queryClient
    )

    unsubscribe()
    expect(received).toHaveLength(1)
    expect(received[0].server_id).toBe('s1')
  })

  it('drops malformed network_probe_update frames before dispatch', () => {
    const queryClient = new QueryClient()
    const received: Record<string, unknown>[] = []
    const unsubscribe = subscribeBrowserMessage('network_probe_update', (msg) => received.push(msg))

    handleWsMessage({ results: [null], server_id: 's1', type: 'network_probe_update' }, queryClient)
    handleWsMessage({ results: [{}], type: 'network_probe_update' }, queryClient)

    unsubscribe()
    expect(received).toHaveLength(0)
  })
})

describe('handleWsMessage Agent Authority updates', () => {
  it('projects a canonical Agent Authority change into the live catalog', () => {
    const queryClient = new QueryClient()
    handleWsMessage(
      {
        type: 'full_sync',
        servers: [{ id: 'server-1', name: 'edge-1', online: true }]
      },
      queryClient
    )

    handleWsMessage(
      {
        type: 'agent_authority_changed',
        server_id: 'server-1',
        agent_authority: { outstanding_offer: null, status: 'unclaimed' }
      },
      queryClient
    )

    const servers = queryClient.getQueryData<
      Array<{ agent_authority?: { outstanding_offer: unknown; status: string }; has_token?: boolean; online: boolean }>
    >(['server-catalog', 'live'])
    expect(servers?.[0]).toMatchObject({
      agent_authority: { outstanding_offer: null, status: 'unclaimed' },
      has_token: false,
      online: false
    })
  })
})
