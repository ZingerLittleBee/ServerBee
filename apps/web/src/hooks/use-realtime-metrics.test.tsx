import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it } from 'vitest'
import { toRealtimeDataPoint, useRealtimeMetrics } from './use-realtime-metrics'
import type { ServerMetrics } from './use-servers-ws'

function makeMetrics(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    cpu: 50,
    cpu_name: 'Intel i7',
    country_code: 'US',
    disk_total: 500_000_000_000,
    disk_used: 100_000_000_000,
    group_id: 'g1',
    id: 's1',
    last_active: 1_710_500_000,
    load1: 1.5,
    load5: 1.2,
    load15: 1.0,
    mem_total: 16_000_000_000,
    mem_used: 8_000_000_000,
    name: 'Test',
    net_in_speed: 1000,
    net_in_transfer: 10_000,
    net_out_speed: 500,
    net_out_transfer: 5000,
    online: true,
    os: 'Linux',
    process_count: 200,
    region: 'US-East',
    swap_total: 4_000_000_000,
    swap_used: 0,
    tcp_conn: 100,
    udp_conn: 10,
    uptime: 3600,
    ...overrides
  }
}

describe('toRealtimeDataPoint', () => {
  it('converts with correct percentages', () => {
    const metrics = makeMetrics({
      cpu: 75,
      disk_total: 500_000_000_000,
      disk_used: 250_000_000_000,
      mem_total: 16_000_000_000,
      mem_used: 4_000_000_000
    })
    const point = toRealtimeDataPoint(metrics)
    expect(point.cpu).toBe(75)
    expect(point.memory_pct).toBe(25)
    expect(point.disk_pct).toBe(50)
  })

  it('handles zero mem_total without division by zero', () => {
    const metrics = makeMetrics({ mem_total: 0, mem_used: 1000 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.memory_pct).toBe(0)
  })

  it('handles zero disk_total without division by zero', () => {
    const metrics = makeMetrics({ disk_total: 0, disk_used: 1000 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.disk_pct).toBe(0)
  })

  it('maps all metric fields correctly', () => {
    const metrics = makeMetrics({
      last_active: 1_710_500_000,
      load1: 2.5,
      load5: 2.0,
      load15: 1.5,
      net_in_speed: 2048,
      net_in_transfer: 50_000,
      net_out_speed: 1024,
      net_out_transfer: 25_000
    })
    const point = toRealtimeDataPoint(metrics)
    expect(point.timestamp).toBe(new Date(1_710_500_000 * 1000).toISOString())
    expect(point.net_in_speed).toBe(2048)
    expect(point.net_out_speed).toBe(1024)
    expect(point.net_in_transfer).toBe(50_000)
    expect(point.net_out_transfer).toBe(25_000)
    expect(point.load1).toBe(2.5)
    expect(point.load5).toBe(2.0)
    expect(point.load15).toBe(1.5)
  })
})

describe('useRealtimeMetrics', () => {
  function createTestEnv() {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } }
    })
    function Wrapper({ children }: { children: ReactNode }) {
      return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    }
    return { queryClient, Wrapper }
  }

  it('seeds from existing cache on mount', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: 42, last_active: 1000 })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })

    expect(result.current).toHaveLength(1)
    expect(result.current[0].cpu).toBe(42)
    expect(result.current[0].timestamp).toBe(new Date(1000 * 1000).toISOString())
  })

  it('returns empty array when server is not in cache', () => {
    const { Wrapper } = createTestEnv()

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })

    expect(result.current).toHaveLength(0)
  })

  it('does not seed when server is offline', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', online: false })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })

    expect(result.current).toHaveLength(0)
  })

  it('does not seed when last_active is 0', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', last_active: 0 })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })

    expect(result.current).toHaveLength(0)
  })

  it('appends data point when last_active changes', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: 10, last_active: 1000 })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })
    expect(result.current).toHaveLength(1)

    act(() => {
      queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: 20, last_active: 1003 })])
    })

    expect(result.current).toHaveLength(2)
    expect(result.current[1].cpu).toBe(20)
  })

  it('deduplicates when last_active stays the same', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: 10, last_active: 1000 })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })
    expect(result.current).toHaveLength(1)

    // Same last_active, different cpu — should NOT append
    act(() => {
      queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: 99, last_active: 1000 })])
    })

    expect(result.current).toHaveLength(1)
    expect(result.current[0].cpu).toBe(10)
  })

  it('ignores updates for a different server', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(
      ['servers'],
      [makeMetrics({ id: 's1', last_active: 1000 }), makeMetrics({ id: 's2', last_active: 2000 })]
    )

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })
    expect(result.current).toHaveLength(1)

    // Update only s2
    act(() => {
      queryClient.setQueryData<ServerMetrics[]>(
        ['servers'],
        [makeMetrics({ id: 's1', last_active: 1000 }), makeMetrics({ id: 's2', last_active: 2003 })]
      )
    })

    expect(result.current).toHaveLength(1)
  })

  it('trims buffer when exceeding threshold', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', last_active: 1 })])

    const { result } = renderHook(() => useRealtimeMetrics('s1'), { wrapper: Wrapper })

    // Push 260 updates (seed=1, then 259 more → 260 total, exceeds 250 threshold)
    act(() => {
      for (let i = 2; i <= 260; i++) {
        queryClient.setQueryData<ServerMetrics[]>(['servers'], [makeMetrics({ id: 's1', cpu: i, last_active: i })])
      }
    })

    // After trim at 251 → 200, then 252-260 push 9 more → 209
    expect(result.current.length).toBe(209)
    // Oldest point should be ~52 (200 points kept from position 52-251, then 252-260 appended)
    expect(result.current[0].cpu).toBe(52)
    // The last element should have the most recent data
    expect(result.current.at(-1)?.cpu).toBe(260)
  })

  it('resets buffer when serverId changes', () => {
    const { queryClient, Wrapper } = createTestEnv()
    queryClient.setQueryData<ServerMetrics[]>(
      ['servers'],
      [makeMetrics({ id: 's1', cpu: 10, last_active: 1000 }), makeMetrics({ id: 's2', cpu: 20, last_active: 2000 })]
    )

    const { result, rerender } = renderHook(({ id }) => useRealtimeMetrics(id), {
      initialProps: { id: 's1' },
      wrapper: Wrapper
    })
    expect(result.current).toHaveLength(1)
    expect(result.current[0].cpu).toBe(10)

    rerender({ id: 's2' })
    expect(result.current).toHaveLength(1)
    expect(result.current[0].cpu).toBe(20)
  })
})
