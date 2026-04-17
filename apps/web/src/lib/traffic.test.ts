import { describe, expect, it } from 'vitest'
import type { TrafficOverviewItem } from '@/hooks/use-traffic-overview'
import { computeTrafficQuota, DEFAULT_TRAFFIC_LIMIT_BYTES } from './traffic'

const GB = 1024 ** 3
const TB = 1024 ** 4

function entry(overrides: Partial<TrafficOverviewItem>): TrafficOverviewItem {
  return {
    billing_cycle: null,
    cycle_in: 0,
    cycle_out: 0,
    days_remaining: null,
    name: 'srv',
    percent_used: null,
    server_id: 'srv-1',
    traffic_limit: null,
    ...overrides
  }
}

describe('computeTrafficQuota', () => {
  it('uses cycle_in + cycle_out when entry present', () => {
    const result = computeTrafficQuota({
      entry: entry({ cycle_in: 50 * GB, cycle_out: 43.2 * GB, traffic_limit: 1 * TB }),
      netInTransfer: 999,
      netOutTransfer: 999
    })
    expect(result.used).toBe(50 * GB + 43.2 * GB)
    expect(result.limit).toBe(1 * TB)
    expect(result.pct).toBeCloseTo(((50 + 43.2) / 1024) * 100, 1)
  })

  it('falls back to net_in_transfer + net_out_transfer when entry is undefined', () => {
    const result = computeTrafficQuota({
      entry: undefined,
      netInTransfer: 10 * GB,
      netOutTransfer: 5 * GB
    })
    expect(result.used).toBe(15 * GB)
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
    expect(DEFAULT_TRAFFIC_LIMIT_BYTES).toBe(TB)
  })

  it('falls back to default limit when traffic_limit is null', () => {
    const result = computeTrafficQuota({
      entry: entry({ traffic_limit: null }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
  })

  it('falls back to default limit when traffic_limit <= 0', () => {
    const result = computeTrafficQuota({
      entry: entry({ traffic_limit: 0 }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)

    const negative = computeTrafficQuota({
      entry: entry({ traffic_limit: -1 }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(negative.limit).toBe(DEFAULT_TRAFFIC_LIMIT_BYTES)
  })

  it('clamps pct to 100 when used exceeds limit', () => {
    const result = computeTrafficQuota({
      entry: entry({ cycle_in: 2 * TB, cycle_out: 0, traffic_limit: 1 * TB }),
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.pct).toBe(100)
  })

  it('returns 0 pct when limit resolves to the default and used is 0', () => {
    const result = computeTrafficQuota({
      entry: undefined,
      netInTransfer: 0,
      netOutTransfer: 0
    })
    expect(result.pct).toBe(0)
  })
})
