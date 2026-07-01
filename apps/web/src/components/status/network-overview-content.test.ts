import { describe, expect, it } from 'vitest'
import type { NetworkOverviewSummary } from './network-overview-content'
import { serverHealth } from './network-overview-health'

function mk(online: boolean, targets: [number | null, number][]): NetworkOverviewSummary {
  return {
    anomaly_count: 0,
    last_probe_at: null,
    online,
    server_id: 's',
    server_name: 'n',
    targets: targets.map(([avg_latency, packet_loss], i) => ({
      avg_latency,
      packet_loss,
      target_id: String(i),
      target_name: `t${i}`
    }))
  }
}

describe('serverHealth', () => {
  it('returns offline regardless of readings when the server is offline', () => {
    expect(serverHealth(mk(false, [[20, 0]]))).toBe('offline')
  })

  it('returns unknown when online with no latency reading (empty targets)', () => {
    expect(serverHealth(mk(true, []))).toBe('unknown')
  })

  it('returns unknown when online but every target latency is null', () => {
    // Regression: loss=0 would otherwise resolve to "healthy" while the card body shows "no data".
    expect(serverHealth(mk(true, [[null, 0]]))).toBe('unknown')
  })

  it('returns healthy for low latency and no loss', () => {
    expect(serverHealth(mk(true, [[40, 0]]))).toBe('healthy')
  })

  it('returns warning when avg latency reaches the 300ms threshold', () => {
    expect(serverHealth(mk(true, [[320, 0]]))).toBe('warning')
  })

  it('returns warning when loss crosses the 1% warning threshold', () => {
    expect(serverHealth(mk(true, [[40, 0.02]]))).toBe('warning')
  })

  it('returns severe when loss crosses the 5% severe threshold', () => {
    expect(serverHealth(mk(true, [[40, 0.08]]))).toBe('severe')
  })

  it('returns severe when a target is fully unreachable (100% loss)', () => {
    expect(serverHealth(mk(true, [[40, 1]]))).toBe('severe')
  })
})
