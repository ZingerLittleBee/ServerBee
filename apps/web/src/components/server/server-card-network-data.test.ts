import { describe, expect, it } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { buildServerCardNetworkState } from './server-card-network-data'

function makeSummary(overrides: Partial<NetworkServerSummary> = {}): NetworkServerSummary {
  return {
    anomaly_count: 0,
    last_probe_at: null,
    latency_sparkline: [],
    loss_sparkline: [],
    online: true,
    server_id: 'srv-1',
    server_name: 'test-server',
    targets: [],
    ...overrides
  }
}

describe('buildServerCardNetworkState', () => {
  it('aggregates multiple targets by timestamp and preserves tooltip details', () => {
    const summary = makeSummary({
      targets: [
        {
          availability: 0.99,
          avg_latency: 40,
          max_latency: 45,
          min_latency: 35,
          packet_loss: 0.01,
          provider: 'ct',
          target_id: 'target-1',
          target_name: 'Shanghai'
        },
        {
          availability: 0.98,
          avg_latency: 50,
          max_latency: 55,
          min_latency: 45,
          packet_loss: 0.02,
          provider: 'cu',
          target_id: 'target-2',
          target_name: 'Beijing'
        }
      ]
    })

    const state = buildServerCardNetworkState(summary, {
      'target-1': [
        {
          avg_latency: 50,
          max_latency: 55,
          min_latency: 45,
          packet_loss: 0.02,
          packet_received: 9,
          packet_sent: 10,
          target_id: 'target-1',
          timestamp: '2026-04-12T10:00:00Z'
        },
        {
          avg_latency: 70,
          max_latency: 75,
          min_latency: 65,
          packet_loss: 0.04,
          packet_received: 8,
          packet_sent: 10,
          target_id: 'target-1',
          timestamp: '2026-04-12T10:01:00Z'
        }
      ],
      'target-2': [
        {
          avg_latency: 30,
          max_latency: 35,
          min_latency: 25,
          packet_loss: 0.01,
          packet_received: 10,
          packet_sent: 10,
          target_id: 'target-2',
          timestamp: '2026-04-12T10:00:00Z'
        },
        {
          avg_latency: 90,
          max_latency: 95,
          min_latency: 85,
          packet_loss: 0.05,
          packet_received: 7,
          packet_sent: 10,
          target_id: 'target-2',
          timestamp: '2026-04-12T10:01:00Z'
        }
      ]
    })

    expect(state.currentAvgLatency).toBe(80)
    expect(state.currentAvgLossRatio).toBeCloseTo(0.045)
    expect(state.latencyPoints).toHaveLength(12)
    expect(state.latencyPoints.at(-2)?.value).toBe(40)
    expect(state.latencyPoints.at(-1)?.value).toBe(80)
    expect(state.latencyPoints.at(-1)?.targets).toEqual([
      { latency: 70, lossRatio: 0.04, targetId: 'target-1', targetName: 'Shanghai' },
      { latency: 90, lossRatio: 0.05, targetId: 'target-2', targetName: 'Beijing' }
    ])
    expect(state.lossPoints.at(-1)?.value).toBe(4.5)
  })

  it('falls back to summary sparkline data when realtime samples are absent', () => {
    const summary = makeSummary({
      latency_sparkline: [10, null, 40],
      loss_sparkline: [0.01, null, 0.03],
      targets: [
        {
          availability: 0.99,
          avg_latency: 40,
          max_latency: 45,
          min_latency: 35,
          packet_loss: 0.03,
          provider: 'ct',
          target_id: 'target-1',
          target_name: 'Shanghai'
        }
      ]
    })

    const state = buildServerCardNetworkState(summary, {})

    expect(state.currentAvgLatency).toBe(40)
    expect(state.currentAvgLossRatio).toBe(0.03)
    expect(state.latencyPoints.at(-1)?.value).toBe(40)
    expect(state.lossPoints.at(-1)?.value).toBe(3)
    expect(state.latencyPoints.at(-1)?.targets).toEqual([
      { latency: 40, lossRatio: 0.03, targetId: 'target-1', targetName: 'Shanghai' }
    ])
  })

  it('keeps backend seed data and appends realtime points when live samples arrive', () => {
    const summary = makeSummary({
      latency_sparkline: [10, 20, 30],
      loss_sparkline: [0.01, 0.02, 0.03],
      targets: [
        {
          availability: 0.99,
          avg_latency: 40,
          max_latency: 45,
          min_latency: 35,
          packet_loss: 0.03,
          provider: 'ct',
          target_id: 'target-1',
          target_name: 'Shanghai'
        }
      ]
    })

    const state = buildServerCardNetworkState(summary, {
      'target-1': [
        {
          avg_latency: 60,
          max_latency: 65,
          min_latency: 55,
          packet_loss: 0.04,
          packet_received: 8,
          packet_sent: 10,
          target_id: 'target-1',
          timestamp: '2026-04-12T10:00:00Z'
        }
      ]
    })

    expect(state.latencyPoints).toHaveLength(12)
    expect(state.latencyPoints.at(-4)?.value).toBe(10)
    expect(state.latencyPoints.at(-1)?.value).toBe(60)
    expect(state.currentAvgLatency).toBe(60)
    expect(state.currentAvgLossRatio).toBe(0.04)
  })

  it('left-pads trends so recent points stay right-aligned in a full-width chart', () => {
    const summary = makeSummary({
      targets: [
        {
          availability: 0.99,
          avg_latency: 40,
          max_latency: 45,
          min_latency: 35,
          packet_loss: 0.03,
          provider: 'ct',
          target_id: 'target-1',
          target_name: 'Shanghai'
        }
      ]
    })

    const state = buildServerCardNetworkState(summary, {
      'target-1': [
        {
          avg_latency: 60,
          max_latency: 65,
          min_latency: 55,
          packet_loss: 0.04,
          packet_received: 8,
          packet_sent: 10,
          target_id: 'target-1',
          timestamp: '2026-04-12T10:00:00Z'
        }
      ]
    })

    expect(state.latencyPoints).toHaveLength(12)
    expect(state.latencyPoints[0].value).toBeNull()
    expect(state.latencyPoints.at(-1)?.value).toBe(60)
    expect(state.lossPoints).toHaveLength(12)
    expect(state.lossPoints[0].value).toBeNull()
    expect(state.lossPoints.at(-1)?.value).toBe(4)
  })
})
