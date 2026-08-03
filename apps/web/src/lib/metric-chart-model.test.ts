import { describe, expect, it } from 'vitest'
import type { PublicMetricsPoint } from '@/lib/api-schema'
import {
  buildGpuChartRows,
  buildRealtimeTickLabels,
  deriveNetworkLabels,
  type MetricSeriesPoint,
  makeTickFormatter,
  makeTooltipFormatter,
  pct,
  toMetricChartRow
} from './metric-chart-model'

const HH_MM = /^\d{2}:\d{2}$/
const MM_DD = /^\d{2}-\d{2}$/
const HH_MM_SS = /^\d{2}:\d{2}:\d{2}$/
const MM_DD_HH_MM = /^\d{2}-\d{2} \d{2}:\d{2}$/

function makePoint(overrides: Partial<MetricSeriesPoint> = {}): MetricSeriesPoint {
  return {
    cpu: 42,
    disk_used: 250,
    load1: 1,
    load5: 0.8,
    load15: 0.6,
    mem_used: 512,
    net_in_speed: 1000,
    net_in_transfer: 10_000,
    net_out_speed: 500,
    net_out_transfer: 5000,
    temperature: 55,
    time: '2026-07-11T10:00:00Z',
    ...overrides
  }
}

function makePublicPoint(overrides: Partial<PublicMetricsPoint> = {}): PublicMetricsPoint {
  return {
    ...makePoint(),
    gpu_usage: null,
    process_count: 100,
    tcp_conn: 10,
    temperature: 55,
    udp_conn: 2,
    ...overrides
  }
}

describe('pct', () => {
  it('computes a percentage against the total', () => {
    expect(pct(512, 1024)).toBe(50)
  })

  it('guards zero and negative totals to 0 instead of NaN/Infinity', () => {
    expect(pct(512, 0)).toBe(0)
    expect(pct(512, -1)).toBe(0)
    expect(pct(0, 0)).toBe(0)
  })
})

describe('toMetricChartRow', () => {
  it('maps a series point into a chart row with derived percentages', () => {
    const row = toMetricChartRow(makePoint(), 1024, 1000)
    expect(row.timestamp).toBe('2026-07-11T10:00:00Z')
    expect(row.cpu).toBe(42)
    expect(row.memory_pct).toBe(50)
    expect(row.disk_pct).toBe(25)
    expect(row.net_in_speed).toBe(1000)
    expect(row.load15).toBe(0.6)
    expect(row.temperature).toBe(55)
  })

  it('renders 0% when the server row has no totals yet', () => {
    const row = toMetricChartRow(makePoint(), 0, 0)
    expect(row.memory_pct).toBe(0)
    expect(row.disk_pct).toBe(0)
  })
})

describe('buildRealtimeTickLabels', () => {
  it('labels only the first point of each minute', () => {
    const labels = buildRealtimeTickLabels([
      { timestamp: '2026-07-11T10:00:05Z' },
      { timestamp: '2026-07-11T10:00:35Z' },
      { timestamp: '2026-07-11T10:01:05Z' }
    ])
    expect(labels.get('2026-07-11T10:00:05Z')).toMatch(HH_MM)
    expect(labels.get('2026-07-11T10:00:35Z')).toBe('')
    expect(labels.get('2026-07-11T10:01:05Z')).toMatch(HH_MM)
  })

  it('skips rows without a string timestamp', () => {
    const labels = buildRealtimeTickLabels([{ timestamp: 42 }, {}])
    expect(labels.size).toBe(0)
  })
})

describe('makeTickFormatter', () => {
  it('falls back to HH:MM for unknown realtime timestamps', () => {
    const format = makeTickFormatter(true, 0, [])
    expect(format?.('2026-07-11T10:00:00Z')).toMatch(HH_MM)
  })

  it('uses MM-DD labels for ranges of 7 days and longer', () => {
    const format = makeTickFormatter(false, 168, [])
    expect(format?.('2026-07-11T10:00:00Z')).toMatch(MM_DD)
  })

  it('leaves short historical ranges to the chart default', () => {
    expect(makeTickFormatter(false, 24, [])).toBeUndefined()
  })
})

describe('makeTooltipFormatter', () => {
  it('shows seconds in realtime mode', () => {
    const format = makeTooltipFormatter(true, 0)
    expect(format?.('2026-07-11T10:00:07Z')).toMatch(HH_MM_SS)
  })

  it('shows date and time for long ranges', () => {
    const format = makeTooltipFormatter(false, 720)
    expect(format?.('2026-07-11T10:00:00Z')).toMatch(MM_DD_HH_MM)
  })

  it('leaves short historical ranges to the chart default', () => {
    expect(makeTooltipFormatter(false, 24)).toBeUndefined()
  })
})

describe('buildGpuChartRows', () => {
  it('derives gpu memory percentage with the zero-total guard', () => {
    const rows = buildGpuChartRows(
      true,
      [
        { gpu_usage_avg: 80, mem_total_avg: 16, mem_used_avg: 8, temperature_avg: 70, time: 't1' },
        { gpu_usage_avg: 10, mem_total_avg: 0, mem_used_avg: 0, temperature_avg: 40, time: 't2' }
      ],
      undefined
    )
    expect(rows).toEqual([
      { gpu_mem_pct: 50, gpu_temp: 70, gpu_usage: 80, timestamp: 't1' },
      { gpu_mem_pct: 0, gpu_temp: 40, gpu_usage: 10, timestamp: 't2' }
    ])
  })

  it('filters public points without gpu data', () => {
    const rows = buildGpuChartRows(false, undefined, [
      makePublicPoint({ gpu_usage: 33, time: 'with-gpu' }),
      makePublicPoint({ gpu_usage: null, time: 'without-gpu' })
    ])
    expect(rows).toEqual([{ gpu_usage: 33, timestamp: 'with-gpu' }])
  })

  it('returns empty rows when no source data exists', () => {
    expect(buildGpuChartRows(true, undefined, undefined)).toEqual([])
    expect(buildGpuChartRows(true, [], undefined)).toEqual([])
    expect(buildGpuChartRows(false, undefined, undefined)).toEqual([])
  })
})

describe('deriveNetworkLabels', () => {
  const snapshot = {
    net_in_transfer: 1024,
    net_out_transfer: 2048
  } as Parameters<typeof deriveNetworkLabels>[2]

  it('renders placeholders while admin live data is missing', () => {
    expect(deriveNetworkLabels(true, undefined, null)).toEqual({
      netInLabel: '—',
      netOutLabel: '—',
      netTotalLabel: '—'
    })
  })

  it('sums cumulative transfer for the admin total', () => {
    const live = { net_in_transfer: 1024, net_out_transfer: 2048 } as Parameters<typeof deriveNetworkLabels>[1]
    const labels = deriveNetworkLabels(true, live, null)
    expect(labels.netInLabel).toBe('1.0 KB')
    expect(labels.netOutLabel).toBe('2.0 KB')
    expect(labels.netTotalLabel).toBe('3.0 KB')
  })

  it('uses the public snapshot transfer totals, omitting the total when absent', () => {
    expect(deriveNetworkLabels(false, undefined, null)).toEqual({
      netInLabel: '—',
      netOutLabel: '—',
      netTotalLabel: null
    })
    const labels = deriveNetworkLabels(false, undefined, snapshot)
    expect(labels.netInLabel).toBe('1.0 KB')
    expect(labels.netTotalLabel).toBe('3.0 KB')
  })
})
