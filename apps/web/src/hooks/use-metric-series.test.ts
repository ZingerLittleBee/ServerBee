import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import type { ServerMetricRecord } from '@/lib/api-schema'
import { useMetricSeries } from './use-metric-series'

function record(time: string, cpu: number): ServerMetricRecord {
  return {
    time,
    cpu,
    mem_used: 0,
    disk_used: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_io_json: null
  } as unknown as ServerMetricRecord
}

function server(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'srv',
    online: true,
    cpu: 50,
    mem_used: 0,
    mem_total: 0,
    disk_used: 0,
    disk_total: 0,
    swap_used: 0,
    swap_total: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    ...overrides
  } as unknown as ServerMetrics
}

describe('useMetricSeries', () => {
  it('returns null stats when records are empty', () => {
    const { result } = renderHook(() => useMetricSeries({ records: [], server: server(), metric: 'cpu' }))
    expect(result.current.points).toHaveLength(1)
    expect(result.current.peak).toBe(50)
    expect(result.current.avg).toBe(50)
    expect(result.current.oneHourDelta).toBeNull()
  })

  it('computes peak and avg from records + live tick', () => {
    const now = Date.now()
    const records = [
      record(new Date(now - 60 * 60_000).toISOString(), 20),
      record(new Date(now - 30 * 60_000).toISOString(), 60),
      record(new Date(now - 5 * 60_000).toISOString(), 40)
    ]
    const { result } = renderHook(() => useMetricSeries({ records, server: server({ cpu: 80 }), metric: 'cpu' }))
    expect(result.current.current).toBe(80)
    expect(result.current.peak).toBe(80)
    expect(result.current.avg).toBeCloseTo((20 + 60 + 40 + 80) / 4)
  })

  it('computes 1h delta when a sample exists near 1h ago', () => {
    const now = Date.now()
    const records = [
      record(new Date(now - 62 * 60_000).toISOString(), 30),
      record(new Date(now - 1 * 60_000).toISOString(), 45)
    ]
    const { result } = renderHook(() => useMetricSeries({ records, server: server({ cpu: 50 }), metric: 'cpu' }))
    expect(result.current.oneHourDelta).toBeCloseTo(50 - 30)
  })

  it('returns null delta when no sample is old enough', () => {
    const now = Date.now()
    const records = [record(new Date(now - 5 * 60_000).toISOString(), 30)]
    const { result } = renderHook(() => useMetricSeries({ records, server: server({ cpu: 32 }), metric: 'cpu' }))
    expect(result.current.oneHourDelta).toBeNull()
  })

  it('aggregates network as in+out', () => {
    const { result } = renderHook(() =>
      useMetricSeries({
        records: [],
        server: server({ net_in_speed: 1000, net_out_speed: 2000 }),
        metric: 'network'
      })
    )
    expect(result.current.current).toBe(3000)
  })
})
