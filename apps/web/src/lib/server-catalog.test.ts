import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { createElement, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { OutstandingEnrollmentSummary, ServerResponse } from '@/lib/api-schema'
import {
  invalidateServerDetail,
  type LiveMetrics,
  projectServerCatalog,
  readLiveServers,
  refreshServerCatalog,
  type ServerMetrics,
  subscribeLiveServers,
  useLiveServers,
  useServerDetail,
  useServerList
} from './server-catalog'

function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false }
    }
  })
}

function createWrapper(queryClient: QueryClient) {
  return function QueryWrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children)
  }
}

function apiResponse<T>(data: T): Response {
  return new Response(JSON.stringify({ data }), {
    headers: { 'Content-Type': 'application/json' },
    status: 200
  })
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolvePromise: ((value: T) => void) | null = null
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve
  })
  return {
    promise,
    resolve: (value) => {
      if (!resolvePromise) {
        throw new Error('Deferred promise was not initialized')
      }
      resolvePromise(value)
    }
  }
}

function readServerList(queryClient: QueryClient): ServerResponse[] | undefined {
  const { result, unmount } = renderHook(() => useServerList({ enabled: false }), {
    wrapper: createWrapper(queryClient)
  })
  const data = result.current.data
  unmount()
  return data
}

function readServerDetail(queryClient: QueryClient, serverId: string): ServerResponse | undefined {
  const { result, unmount } = renderHook(() => useServerDetail(serverId, { enabled: false }), {
    wrapper: createWrapper(queryClient)
  })
  const data = result.current.data
  unmount()
  return data
}

function makeRestServer(overrides: Partial<ServerResponse> = {}): ServerResponse {
  return {
    capabilities: 1852,
    country_code: 'US',
    cpu_cores: 8,
    cpu_name: 'AMD EPYC',
    created_at: '2026-07-01T00:00:00Z',
    disk_total: 500_000,
    effective_capabilities: 1852,
    features: ['docker'],
    geo_manual: false,
    group_id: 'edge',
    has_token: true,
    hidden: false,
    id: 'server-1',
    mem_total: 32_000,
    name: 'Edge One',
    os: 'Linux',
    outstanding_enrollment: null,
    protocol_version: 2,
    region: 'us-east',
    swap_total: 4000,
    temporary: [],
    updated_at: '2026-07-01T00:00:00Z',
    weight: 100,
    ...overrides
  }
}

function makeMetrics(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    country_code: 'US',
    cpu: 42,
    cpu_cores: 8,
    cpu_name: 'AMD EPYC',
    disk_read_bytes_per_sec: 100,
    disk_total: 500_000,
    disk_used: 125_000,
    disk_write_bytes_per_sec: 50,
    features: ['docker'],
    group_id: 'edge',
    has_token: true,
    id: 'server-1',
    last_active: 1000,
    load1: 1,
    load5: 0.8,
    load15: 0.6,
    mem_total: 32_000,
    mem_used: 16_000,
    name: 'Edge One',
    net_in_speed: 1000,
    net_in_transfer: 10_000,
    net_out_speed: 500,
    net_out_transfer: 5000,
    online: true,
    os: 'Linux',
    process_count: 120,
    region: 'us-east',
    swap_total: 4000,
    swap_used: 100,
    tags: ['prod'],
    tcp_conn: 30,
    udp_conn: 5,
    uptime: 3600,
    ...overrides
  }
}

function makeLiveMetrics(overrides: Partial<LiveMetrics> = {}): LiveMetrics {
  return {
    cpu: 42,
    disk_read_bytes_per_sec: 100,
    disk_used: 125_000,
    disk_write_bytes_per_sec: 50,
    id: 'server-1',
    last_active: 1000,
    load1: 1,
    load5: 0.8,
    load15: 0.6,
    mem_used: 16_000,
    net_in_speed: 1000,
    net_in_transfer: 10_000,
    net_out_speed: 500,
    net_out_transfer: 5000,
    online: true,
    process_count: 120,
    swap_used: 100,
    tcp_conn: 30,
    udp_conn: 5,
    uptime: 3600,
    ...overrides
  }
}

const OUTSTANDING_ENROLLMENT: OutstandingEnrollmentSummary = {
  code_prefix: 'abcdef',
  created_at: '2026-07-11T00:00:00Z',
  expires_at: '2026-07-11T00:10:00Z',
  id: 'enrollment-1'
}

describe('server catalog projection', () => {
  it('preserves WS runtime data while an authoritative REST snapshot applies null clears and membership', () => {
    const queryClient = createQueryClient()
    const live = makeMetrics({ cpu: 73, mem_used: 20_000, online: true })
    const priorDetail = makeRestServer()
    const cleared = makeRestServer({
      country_code: null,
      cpu_cores: null,
      cpu_name: null,
      disk_total: null,
      features: [],
      group_id: null,
      mem_total: null,
      name: 'Renamed Edge',
      os: null,
      region: null,
      swap_total: null
    })
    const added = makeRestServer({ id: 'server-2', name: 'Pending Two' })

    projectServerCatalog(queryClient, { kind: 'ws_full_sync', servers: [live] })
    projectServerCatalog(queryClient, { kind: 'server_saved', server: priorDetail })
    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [cleared, added] })

    const projected = readLiveServers(queryClient)
    expect(projected).toHaveLength(2)
    expect(projected?.[0]).toMatchObject({
      country_code: null,
      cpu: 73,
      cpu_cores: null,
      cpu_name: null,
      disk_total: 0,
      features: [],
      group_id: null,
      mem_total: 0,
      mem_used: 20_000,
      name: 'Renamed Edge',
      online: true,
      os: null,
      region: null,
      swap_total: 0
    })
    expect(projected?.[1]).toMatchObject({ cpu: 0, id: 'server-2', name: 'Pending Two', online: false })
    expect(readServerList(queryClient)).toEqual([cleared, added])
    expect(readServerDetail(queryClient, 'server-1')).toEqual(cleared)
  })

  it('merges live-only update frames while REST and mutation-owned static fields survive', () => {
    const queryClient = createQueryClient()
    const rest = makeRestServer({
      has_token: false,
      name: 'Edited Name',
      outstanding_enrollment: OUTSTANDING_ENROLLMENT
    })

    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [rest] })
    projectServerCatalog(queryClient, { kind: 'tags_changed', serverId: rest.id, tags: ['prod', 'edge'] })
    projectServerCatalog(queryClient, {
      kind: 'ws_update',
      servers: [makeLiveMetrics({ cpu: 91, mem_used: 24_000 })]
    })

    expect(readLiveServers(queryClient)?.[0]).toMatchObject({
      country_code: 'US',
      cpu: 91,
      cpu_cores: 8,
      cpu_name: 'AMD EPYC',
      disk_total: 500_000,
      features: ['docker'],
      group_id: 'edge',
      has_token: false,
      mem_total: 32_000,
      mem_used: 24_000,
      name: 'Edited Name',
      os: 'Linux',
      outstanding_enrollment: OUTSTANDING_ENROLLMENT,
      region: 'us-east',
      swap_total: 4000,
      tags: ['prod', 'edge']
    })
  })

  it('drops update frames until full sync or REST seeds the catalog', () => {
    const queryClient = createQueryClient()

    projectServerCatalog(queryClient, { kind: 'ws_update', servers: [makeLiveMetrics()] })

    expect(readLiveServers(queryClient)).toBeUndefined()
  })

  it('uses full sync as authoritative membership and static state while preserving out-of-band metadata', () => {
    const queryClient = createQueryClient()
    const first = makeRestServer({
      disk_total: null,
      mem_total: null,
      outstanding_enrollment: OUTSTANDING_ENROLLMENT,
      swap_total: null
    })
    const second = makeRestServer({ id: 'server-2', name: 'REST Only' })

    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [first, second] })
    projectServerCatalog(queryClient, { kind: 'server_saved', server: first })
    projectServerCatalog(queryClient, { kind: 'server_saved', server: second })
    projectServerCatalog(queryClient, {
      agentLocalCapabilities: 64,
      capabilities: 64,
      effectiveCapabilities: 64,
      kind: 'capabilities_changed',
      serverId: first.id,
      temporary: []
    })
    projectServerCatalog(queryClient, {
      kind: 'ws_full_sync',
      servers: [
        makeMetrics({
          country_code: null,
          cpu: 87,
          cpu_cores: null,
          cpu_name: null,
          disk_total: 0,
          features: [],
          group_id: null,
          has_token: false,
          mem_total: 0,
          name: 'Authoritative Full Sync',
          os: null,
          outstanding_enrollment: null,
          region: null,
          swap_total: 0,
          tags: []
        })
      ]
    })

    const projected = readLiveServers(queryClient)
    expect(projected?.map((server) => server.id)).toEqual([first.id])
    expect(projected?.[0]).toMatchObject({
      capabilities: 64,
      country_code: null,
      cpu: 87,
      cpu_cores: null,
      cpu_name: null,
      disk_total: 0,
      features: [],
      group_id: null,
      has_token: false,
      mem_total: 0,
      name: 'Authoritative Full Sync',
      os: null,
      outstanding_enrollment: null,
      region: null,
      swap_total: 0,
      tags: []
    })
    expect(readServerList(queryClient)?.map((server) => server.id)).toEqual([first.id])
    expect(readServerList(queryClient)?.[0]).toMatchObject({
      country_code: null,
      disk_total: null,
      features: [],
      has_token: false,
      mem_total: null,
      name: 'Authoritative Full Sync',
      swap_total: null
    })
    expect(readServerDetail(queryClient, second.id)).toBeUndefined()
  })

  it('propagates capability, agent-info, and Docker changes to live, list, and existing detail projections', () => {
    const queryClient = createQueryClient()
    const rest = makeRestServer({ features: [] })
    const temporary = [{ cap: 'CAP_TERMINAL', expires_at: 2000, granted_at: 1000 }]

    projectServerCatalog(queryClient, { kind: 'server_saved', server: rest })
    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [rest] })
    projectServerCatalog(queryClient, {
      agentLocalCapabilities: 64,
      capabilities: 64,
      effectiveCapabilities: 65,
      kind: 'capabilities_changed',
      serverId: rest.id,
      temporary
    })
    projectServerCatalog(queryClient, {
      agentVersion: '1.4.0',
      kind: 'agent_info_changed',
      protocolVersion: 3,
      serverId: rest.id
    })
    projectServerCatalog(queryClient, {
      available: true,
      kind: 'docker_availability_changed',
      serverId: rest.id
    })

    const expected = {
      agent_local_capabilities: 64,
      agent_version: '1.4.0',
      capabilities: 64,
      effective_capabilities: 65,
      features: ['docker'],
      protocol_version: 3,
      temporary
    }
    expect(readLiveServers(queryClient)?.[0]).toMatchObject(expected)
    expect(readServerList(queryClient)?.[0]).toMatchObject(expected)
    expect(readServerDetail(queryClient, rest.id)).toMatchObject(expected)

    projectServerCatalog(queryClient, {
      available: false,
      kind: 'docker_availability_changed',
      serverId: rest.id
    })
    expect(readLiveServers(queryClient)?.[0].features).toEqual([])
    expect(readServerList(queryClient)?.[0].features).toEqual([])
    expect(readServerDetail(queryClient, rest.id)?.features).toEqual([])
  })

  it('keeps edit, enrollment, tags, and removal outcomes consistent without erasing runtime data', () => {
    const queryClient = createQueryClient()
    const first = makeRestServer()
    const second = makeRestServer({ id: 'server-2', name: 'Keep Me' })
    const saved = makeRestServer({ group_id: null, name: 'Edited Name' })

    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [first, second] })
    projectServerCatalog(queryClient, { kind: 'online_changed', online: true, serverId: first.id })
    projectServerCatalog(queryClient, { kind: 'server_saved', server: saved })
    projectServerCatalog(queryClient, { kind: 'tags_changed', serverId: first.id, tags: ['edited'] })
    projectServerCatalog(queryClient, {
      kind: 'enrollment_changed',
      outstandingEnrollment: OUTSTANDING_ENROLLMENT,
      serverId: first.id,
      tokenRevoked: true
    })

    expect(readLiveServers(queryClient)?.[0]).toMatchObject({
      group_id: null,
      has_token: false,
      name: 'Edited Name',
      online: false,
      outstanding_enrollment: OUTSTANDING_ENROLLMENT,
      tags: ['edited']
    })
    expect(readServerList(queryClient)?.[0]).toMatchObject({
      group_id: null,
      has_token: false,
      name: 'Edited Name',
      outstanding_enrollment: OUTSTANDING_ENROLLMENT
    })
    expect(readServerDetail(queryClient, first.id)).toMatchObject({
      has_token: false,
      name: 'Edited Name',
      outstanding_enrollment: OUTSTANDING_ENROLLMENT
    })

    projectServerCatalog(queryClient, { kind: 'servers_removed', serverIds: [first.id] })
    expect(readLiveServers(queryClient)?.map((server) => server.id)).toEqual([second.id])
    expect(readServerList(queryClient)?.map((server) => server.id)).toEqual([second.id])
    expect(readServerDetail(queryClient, first.id)).toBeUndefined()
  })

  it('removes detail projections missing from an authoritative REST snapshot', () => {
    const queryClient = createQueryClient()
    const removed = makeRestServer()
    const kept = makeRestServer({ id: 'server-2', name: 'Kept' })

    projectServerCatalog(queryClient, { kind: 'server_saved', server: removed })
    expect(readServerDetail(queryClient, removed.id)).toEqual(removed)

    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [kept] })
    expect(readServerDetail(queryClient, removed.id)).toBeUndefined()
  })

  it('notifies live subscribers only for the exact live projection', async () => {
    const queryClient = createQueryClient()
    const server = makeRestServer()
    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [server] })
    projectServerCatalog(queryClient, { kind: 'server_saved', server })
    const listener = vi.fn()
    const unsubscribe = subscribeLiveServers(queryClient, listener)

    const liveObserver = renderHook(() => useLiveServers({ enabled: false }), {
      wrapper: createWrapper(queryClient)
    })
    liveObserver.unmount()

    await invalidateServerDetail(queryClient, server.id)
    queryClient.setQueryData(['servers', server.id, 'records', 24], [{ cpu: 1 }])
    expect(listener).not.toHaveBeenCalled()

    projectServerCatalog(queryClient, { kind: 'online_changed', online: true, serverId: server.id })
    expect(listener).toHaveBeenCalledTimes(1)
    expect(listener).toHaveBeenLastCalledWith([expect.objectContaining({ id: server.id, online: true })])

    unsubscribe()
  })

  it('retains unobserved live membership beyond the default query garbage-collection window', () => {
    vi.useFakeTimers()
    try {
      const queryClient = createQueryClient()
      const server = makeMetrics()
      projectServerCatalog(queryClient, { kind: 'ws_full_sync', servers: [server] })

      vi.advanceTimersByTime(6 * 60 * 1000)

      expect(readLiveServers(queryClient)?.map((entry) => entry.id)).toEqual([server.id])
    } finally {
      vi.useRealTimers()
    }
  })

  it('projects a normal REST list query through the catalog owner', async () => {
    const queryClient = createQueryClient()
    const server = makeRestServer({ id: 'server-from-http', name: 'Fetched Server' })
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(apiResponse([server]))

    const hook = renderHook(() => useServerList(), { wrapper: createWrapper(queryClient) })
    await waitFor(() => expect(hook.result.current.isSuccess).toBe(true))

    expect(fetchMock).toHaveBeenCalledWith('/api/servers', expect.objectContaining({ method: 'GET' }))
    expect(readLiveServers(queryClient)?.[0]).toMatchObject({
      cpu: 0,
      id: server.id,
      name: server.name,
      online: false
    })
    expect(hook.result.current.data).toEqual([server])

    hook.unmount()
    fetchMock.mockRestore()
  })

  it('projects a REST detail query through live and list views', async () => {
    const queryClient = createQueryClient()
    const stale = makeRestServer({ name: 'Stale Name' })
    const fresh = makeRestServer({ name: 'Fresh Name' })
    projectServerCatalog(queryClient, { kind: 'rest_snapshot', servers: [stale] })
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(apiResponse(fresh))

    const hook = renderHook(() => useServerDetail(fresh.id), { wrapper: createWrapper(queryClient) })
    await waitFor(() => expect(hook.result.current.isSuccess).toBe(true))

    expect(readLiveServers(queryClient)?.[0].name).toBe('Fresh Name')
    expect(readServerList(queryClient)?.[0].name).toBe('Fresh Name')
    expect(readServerDetail(queryClient, fresh.id)?.name).toBe('Fresh Name')

    hook.unmount()
    fetchMock.mockRestore()
  })

  it('prevents an older overlapping REST refresh from replacing a newer snapshot', async () => {
    const queryClient = createQueryClient()
    const oldResponse = deferred<Response>()
    const newResponse = deferred<Response>()
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementationOnce(() => oldResponse.promise)
      .mockImplementationOnce(() => newResponse.promise)

    const oldRefresh = refreshServerCatalog(queryClient)
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1))
    const newRefresh = refreshServerCatalog(queryClient)
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2))

    newResponse.resolve(apiResponse([makeRestServer({ name: 'New Snapshot' })]))
    await newRefresh
    oldResponse.resolve(apiResponse([makeRestServer({ name: 'Old Snapshot' })]))
    await oldRefresh

    expect(readLiveServers(queryClient)?.[0].name).toBe('New Snapshot')
    expect(readServerList(queryClient)?.[0].name).toBe('New Snapshot')
    fetchMock.mockRestore()
  })

  it('forces an explicit refresh even while the cached list is globally fresh', async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 30_000 } }
    })
    projectServerCatalog(queryClient, {
      kind: 'rest_snapshot',
      servers: [makeRestServer({ name: 'Cached Snapshot' })]
    })
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(apiResponse([makeRestServer({ name: 'Refreshed Snapshot' })]))

    await refreshServerCatalog(queryClient)

    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(readLiveServers(queryClient)?.[0].name).toBe('Refreshed Snapshot')
    expect(readServerList(queryClient)?.[0].name).toBe('Refreshed Snapshot')
    fetchMock.mockRestore()
  })
})
