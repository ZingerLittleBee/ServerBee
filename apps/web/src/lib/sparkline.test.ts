import { describe, expect, it } from 'vitest'
import type { NetworkServerSummary } from './network-types'
import { SPARKLINE_LENGTH, seedFromSummary, summaryStats, toBarData } from './sparkline'

function makeSeries(entries: [number, number | null][]): (number | null)[] {
  const series = Array.from({ length: SPARKLINE_LENGTH }, (): number | null => null)

  for (const [index, value] of entries) {
    series[index] = value
  }

  return series
}

function makeSummary(latency_sparkline: (number | null)[], loss_sparkline: (number | null)[]): NetworkServerSummary {
  return {
    anomaly_count: 0,
    last_probe_at: null,
    latency_sparkline,
    loss_sparkline,
    online: true,
    server_id: 'server-1',
    server_name: 'Server 1',
    targets: []
  }
}

describe('seedFromSummary', () => {
  it('returns 30 sparkline points', () => {
    const points = seedFromSummary(makeSummary(makeSeries([]), makeSeries([])))

    expect(points).toHaveLength(SPARKLINE_LENGTH)
  })

  it('preserves fixed-slot nulls and values from the summary arrays', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [27, null],
          [28, 50],
          [29, 100]
        ]),
        makeSeries([
          [27, null],
          [28, 0.01],
          [29, 0.05]
        ])
      )
    )

    expect(points[0]).toEqual({ latency: null, loss: null })
    expect(points[27]).toEqual({ latency: null, loss: null })
    expect(points[28]).toEqual({ latency: 50, loss: 0.01 })
    expect(points[29]).toEqual({ latency: 100, loss: 0.05 })
  })

  it('zips latency and loss values at matching positions', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [3, 10],
          [18, 20]
        ]),
        makeSeries([
          [3, 0.1],
          [18, 0.2]
        ])
      )
    )

    expect(points[3]).toEqual({ latency: 10, loss: 0.1 })
    expect(points[18]).toEqual({ latency: 20, loss: 0.2 })
  })
})

describe('toBarData', () => {
  it('extracts latency values and preserves nulls', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [4, null],
          [5, 50]
        ]),
        makeSeries([
          [4, null],
          [5, 0.01]
        ])
      )
    )
    const data = toBarData(points, 'latency')

    expect(data[4]).toBeNull()
    expect(data[5]).toBe(50)
  })

  it('converts loss to percent and preserves nulls', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [4, 50],
          [5, 60]
        ]),
        makeSeries([
          [4, null],
          [5, 0.05]
        ])
      )
    )
    const data = toBarData(points, 'lossPercent')

    expect(data[4]).toBeNull()
    expect(data[5]).toBe(5)
  })
})

describe('summaryStats', () => {
  it('averages non-null latency and loss values only', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [10, 100],
          [20, 200]
        ]),
        makeSeries([
          [10, 0.1],
          [20, 0.2]
        ])
      )
    )

    const { avgLatency, avgLoss } = summaryStats(points)

    expect(avgLatency).toBe(150)
    expect(avgLoss).toBeCloseTo(0.15)
  })

  it('returns null avgLatency when all latency values are null', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([]),
        makeSeries([
          [10, 0.1],
          [20, 0.2]
        ])
      )
    )

    expect(summaryStats(points).avgLatency).toBeNull()
  })

  it('returns null avgLoss when all loss values are null', () => {
    const points = seedFromSummary(
      makeSummary(
        makeSeries([
          [10, 100],
          [20, 200]
        ]),
        makeSeries([])
      )
    )

    expect(summaryStats(points).avgLoss).toBeNull()
  })
})
