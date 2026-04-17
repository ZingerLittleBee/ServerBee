import { describe, expect, it } from 'vitest'
import { useRecoveryJobsStore } from '@/stores/recovery-jobs-store'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import type { ServerMetrics } from './use-servers-ws'
import { handleWsMessage, mergeServerUpdate, setServerCapabilities, setServerOnlineStatus } from './use-servers-ws'

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'Test',
    online: true,
    last_active: 0,
    cpu: 50,
    mem_used: 8_000_000_000,
    mem_total: 16_000_000_000,
    swap_used: 0,
    swap_total: 4_000_000_000,
    disk_used: 100_000_000_000,
    disk_total: 500_000_000_000,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 10_000,
    net_out_transfer: 5000,
    load1: 1.5,
    load5: 1.2,
    load15: 1.0,
    tcp_conn: 100,
    udp_conn: 10,
    process_count: 200,
    uptime: 3600,
    cpu_name: 'Intel i7',
    os: 'Linux',
    region: 'US-East',
    country_code: 'US',
    group_id: 'g1',
    ...overrides
  }
}

function makeQueryClient() {
  const cache = new Map<string, unknown>()
  return {
    setQueryData: (key: unknown[], value: unknown | ((prev: unknown) => unknown)) => {
      const cacheKey = JSON.stringify(key)
      const prev = cache.get(cacheKey)
      const next = typeof value === 'function' ? (value as (prev: unknown) => unknown)(prev) : value
      cache.set(cacheKey, next)
    }
  }
}

describe('mergeServerUpdate', () => {
  it('updates dynamic fields', () => {
    const prev = [makeServer({ cpu: 50 })]
    const incoming = [makeServer({ cpu: 75, mem_used: 10_000_000_000 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].cpu).toBe(75)
    expect(result[0].mem_used).toBe(10_000_000_000)
  })

  it('preserves static fields when incoming is null', () => {
    const prev = [makeServer({ mem_total: 16_000_000_000, os: 'Linux', cpu_name: 'Intel i7' })]
    const incoming = [makeServer({ id: 's1', mem_total: 0, os: null, cpu_name: null })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].mem_total).toBe(16_000_000_000)
    expect(result[0].os).toBe('Linux')
    expect(result[0].cpu_name).toBe('Intel i7')
  })

  it('preserves static fields when incoming is 0', () => {
    const prev = [makeServer({ disk_total: 500_000_000_000, swap_total: 4_000_000_000 })]
    const incoming = [makeServer({ id: 's1', disk_total: 0, swap_total: 0 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].disk_total).toBe(500_000_000_000)
    expect(result[0].swap_total).toBe(4_000_000_000)
  })

  it('ignores updates for unknown server id', () => {
    const prev = [makeServer({ id: 's1' })]
    const incoming = [makeServer({ id: 'unknown', cpu: 99 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result).toEqual(prev)
  })

  it('returns copy of prev when incoming is empty', () => {
    const prev = [makeServer()]
    const result = mergeServerUpdate(prev, [])
    expect(result).toEqual(prev)
  })
})

describe('setServerOnlineStatus', () => {
  it('sets target server offline', () => {
    const prev = [makeServer({ id: 's1', online: true }), makeServer({ id: 's2', online: true })]
    const result = setServerOnlineStatus(prev, 's1', false)
    expect(result[0].online).toBe(false)
    expect(result[1].online).toBe(true)
  })

  it('sets target server online', () => {
    const prev = [makeServer({ id: 's1', online: false })]
    const result = setServerOnlineStatus(prev, 's1', true)
    expect(result[0].online).toBe(true)
  })

  it('leaves all unchanged for unknown server', () => {
    const prev = [makeServer({ id: 's1', online: true })]
    const result = setServerOnlineStatus(prev, 'unknown', false)
    expect(result[0].online).toBe(true)
  })
})

describe('setServerCapabilities', () => {
  it('updates configured, local, and effective capabilities together', () => {
    const prev = [makeServer({ id: 's1' })]
    const result = setServerCapabilities(prev, 's1', 64, 0, 0)

    expect(result[0].capabilities).toBe(64)
    expect(result[0].agent_local_capabilities).toBe(0)
    expect(result[0].effective_capabilities).toBe(0)
  })
})

describe('handleWsMessage upgrade messages', () => {
  it('hydrates upgrade jobs from full_sync', () => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
    const queryClient = makeQueryClient()

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
      queryClient as never
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
    const queryClient = makeQueryClient()

    handleWsMessage(
      {
        type: 'upgrade_progress',
        server_id: 'server-1',
        job_id: 'job-1',
        target_version: '1.2.3',
        stage: 'installing'
      },
      queryClient as never
    )

    expect(useUpgradeJobsStore.getState().jobs.get('server-1')?.stage).toBe('installing')
  })

  it('stores terminal upgrade result from upgrade_result', () => {
    useUpgradeJobsStore.setState({ jobs: new Map() })
    const queryClient = makeQueryClient()

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
      queryClient as never
    )

    const job = useUpgradeJobsStore.getState().jobs.get('server-1')
    expect(job?.status).toBe('failed')
    expect(job?.error).toBe('install failed')
    expect(job?.backup_path).toBe('/tmp/backup')
    expect(job?.finished_at).not.toBeNull()
  })
})

function baseServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 'srv-1',
    name: 'srv',
    online: true,
    country_code: null,
    cpu: 0,
    cpu_name: null,
    cpu_cores: null,
    disk_read_bytes_per_sec: 0,
    disk_total: 0,
    disk_used: 0,
    disk_write_bytes_per_sec: 0,
    group_id: null,
    last_active: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    mem_total: 0,
    mem_used: 0,
    net_in_speed: 0,
    net_in_transfer: 0,
    net_out_speed: 0,
    net_out_transfer: 0,
    os: null,
    process_count: 0,
    region: null,
    swap_total: 0,
    swap_used: 0,
    tags: [],
    tcp_conn: 0,
    udp_conn: 0,
    uptime: 0,
    features: [],
    ...overrides
  }
}

describe('mergeServerUpdate static-fields guard', () => {
  it('preserves prior tags when incoming frame carries tags: []', () => {
    const prev = [baseServer({ tags: ['prod', 'web'] })]
    const incoming = [baseServer({ tags: [], cpu: 42 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].tags).toEqual(['prod', 'web'])
    expect(result[0].cpu).toBe(42)
  })

  it('preserves prior features when incoming frame carries features: []', () => {
    const prev = [baseServer({ features: ['docker'] })]
    const incoming = [baseServer({ features: [], cpu: 10 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].features).toEqual(['docker'])
    expect(result[0].cpu).toBe(10)
  })

  it('preserves prior cpu_cores when incoming frame carries cpu_cores: null', () => {
    const prev = [baseServer({ cpu_cores: 8 })]
    const incoming = [baseServer({ cpu_cores: null, cpu: 5 })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].cpu_cores).toBe(8)
  })

  it('overwrites prior tags with non-empty incoming array', () => {
    const prev = [baseServer({ tags: ['old'] })]
    const incoming = [baseServer({ tags: ['new-a', 'new-b'] })]
    const result = mergeServerUpdate(prev, incoming)
    expect(result[0].tags).toEqual(['new-a', 'new-b'])
  })
})

describe('handleWsMessage recovery messages', () => {
  it('hydrates recovery jobs from full_sync', () => {
    useRecoveryJobsStore.setState({ jobs: new Map() })
    const queryClient = makeQueryClient()

    handleWsMessage(
      {
        type: 'full_sync',
        servers: [],
        recoveries: [
          {
            job_id: 'job-1',
            target_server_id: 'target-1',
            source_server_id: 'source-1',
            status: 'running',
            stage: 'rebinding',
            error: null,
            started_at: '2026-04-16T00:00:00Z',
            created_at: '2026-04-16T00:00:00Z',
            updated_at: '2026-04-16T00:00:00Z',
            last_heartbeat_at: null
          }
        ]
      },
      queryClient as never
    )

    expect(useRecoveryJobsStore.getState().getJob('target-1')?.job_id).toBe('job-1')
  })

  it('updates recovery jobs only when update payload includes recoveries', () => {
    useRecoveryJobsStore.setState({ jobs: new Map() })
    useRecoveryJobsStore.getState().setJob('target-1', {
      job_id: 'job-1',
      target_server_id: 'target-1',
      source_server_id: 'source-1',
      status: 'running',
      stage: 'rebinding',
      error: null,
      started_at: '2026-04-16T00:00:00Z',
      created_at: '2026-04-16T00:00:00Z',
      updated_at: '2026-04-16T00:00:00Z',
      last_heartbeat_at: null
    })

    const queryClient = makeQueryClient()
    handleWsMessage(
      {
        type: 'update',
        servers: []
      },
      queryClient as never
    )

    expect(useRecoveryJobsStore.getState().getJob('target-1')?.job_id).toBe('job-1')

    handleWsMessage(
      {
        type: 'update',
        servers: [],
        recoveries: [
          {
            job_id: 'job-2',
            target_server_id: 'target-2',
            source_server_id: 'source-2',
            status: 'failed',
            stage: 'failed',
            error: 'boom',
            started_at: '2026-04-16T00:00:00Z',
            created_at: '2026-04-16T00:00:00Z',
            updated_at: '2026-04-16T00:00:00Z',
            last_heartbeat_at: null
          }
        ]
      },
      queryClient as never
    )

    expect(useRecoveryJobsStore.getState().getJob('target-1')).toBeUndefined()
    expect(useRecoveryJobsStore.getState().getJob('target-2')?.job_id).toBe('job-2')
  })
})
