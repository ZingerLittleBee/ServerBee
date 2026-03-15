import { describe, expect, it } from 'vitest'
import { toRealtimeDataPoint } from './use-realtime-metrics'
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
      mem_used: 4_000_000_000,
      mem_total: 16_000_000_000,
      disk_used: 250_000_000_000,
      disk_total: 500_000_000_000
    })
    const point = toRealtimeDataPoint(metrics)
    expect(point.cpu).toBe(75)
    expect(point.memory_pct).toBe(25)
    expect(point.disk_pct).toBe(50)
  })

  it('handles zero mem_total without division by zero', () => {
    const metrics = makeMetrics({ mem_total: 0, mem_used: 0 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.memory_pct).toBe(0)
  })

  it('handles zero disk_total without division by zero', () => {
    const metrics = makeMetrics({ disk_total: 0, disk_used: 0 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.disk_pct).toBe(0)
  })

  it('maps all metric fields correctly', () => {
    const metrics = makeMetrics({
      last_active: 1_710_500_000,
      net_in_speed: 2048,
      net_out_speed: 1024,
      net_in_transfer: 50_000,
      net_out_transfer: 25_000,
      load1: 2.5,
      load5: 2.0,
      load15: 1.5
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
