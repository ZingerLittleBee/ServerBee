import { describe, expect, it } from 'vitest'
import { mergeNetworkChartRecords } from './network-chart-records'
import type { NetworkProbeRecord, NetworkProbeResultData } from './network-types'

const seed: NetworkProbeRecord[] = [
  {
    id: 1,
    server_id: 'srv-1',
    target_id: 't-1',
    timestamp: '2026-05-29T10:00:00.000Z',
    avg_latency: 20,
    min_latency: 18,
    max_latency: 25,
    packet_loss: 0,
    packet_sent: 10,
    packet_received: 10
  }
]

const realtime: Record<string, NetworkProbeResultData[]> = {
  't-1': [
    {
      target_id: 't-1',
      timestamp: '2026-05-29T10:01:00.000Z',
      avg_latency: 22,
      min_latency: 19,
      max_latency: 28,
      packet_loss: 0,
      packet_sent: 10,
      packet_received: 10
    }
  ]
}

describe('mergeNetworkChartRecords', () => {
  it('returns historical records unchanged when not realtime', () => {
    const result = mergeNetworkChartRecords({
      isRealtime: false,
      historical: seed,
      seed: [],
      realtime: {},
      serverId: 'srv-1'
    })
    expect(result).toEqual(seed)
  })

  it('flattens realtime map and merges with seed in realtime mode', () => {
    const result = mergeNetworkChartRecords({ isRealtime: true, historical: [], seed, realtime, serverId: 'srv-1' })
    expect(result).toHaveLength(2)
    expect(result.map((r) => r.timestamp)).toEqual(['2026-05-29T10:00:00.000Z', '2026-05-29T10:01:00.000Z'])
  })

  it('dedupes by target_id + timestamp keeping the latest entry', () => {
    const dupRealtime: Record<string, NetworkProbeResultData[]> = {
      't-1': [
        {
          target_id: 't-1',
          timestamp: '2026-05-29T10:00:00.000Z',
          avg_latency: 99,
          min_latency: 99,
          max_latency: 99,
          packet_loss: 0,
          packet_sent: 10,
          packet_received: 10
        }
      ]
    }
    const result = mergeNetworkChartRecords({
      isRealtime: true,
      historical: [],
      seed,
      realtime: dupRealtime,
      serverId: 'srv-1'
    })
    expect(result).toHaveLength(1)
    expect(result[0].avg_latency).toBe(99)
  })
})
