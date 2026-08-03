import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkProbeRecord } from '@/lib/network-types'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key })
}))

vi.mock('@/components/charts/area-chart', () => ({
  AreaChart: ({ children, data }: { children?: ReactNode; data: Record<string, unknown>[] }) => (
    <div data-chart-data={JSON.stringify(data)} data-testid="area-chart">
      {children}
    </div>
  )
}))

vi.mock('@/components/charts/area', () => ({
  Area: ({ dataKey, stroke }: { dataKey: string; stroke: string }) => (
    <div data-key={dataKey} data-stroke={stroke} data-testid="area-series" />
  )
}))

vi.mock('@/components/charts/grid', () => ({ Grid: () => null }))
vi.mock('@/components/charts/tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('@/components/charts/tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('@/components/charts/x-axis', () => ({ XAxis: () => null }))
vi.mock('@/components/charts/y-axis', () => ({ YAxis: () => null }))

const { buildLatencyChartData, LatencyChartContent } = await import('./latency-chart-content')

const targets = [
  { color: 'red', id: 'target-a', name: 'Target A', visible: true },
  { color: 'blue', id: 'target-b', name: 'Target B', visible: false }
]

function record(overrides: Partial<NetworkProbeRecord>): NetworkProbeRecord {
  return {
    avg_latency: 20,
    id: 1,
    max_latency: 25,
    min_latency: 18,
    packet_loss: 0,
    packet_received: 10,
    packet_sent: 10,
    server_id: 'server-1',
    target_id: 'target-a',
    timestamp: '2026-08-03T10:00:10.000Z',
    ...overrides
  }
}

describe('LatencyChartContent', () => {
  it('buckets known targets and rejects invalid, unknown, and future records', () => {
    const data = buildLatencyChartData(
      [
        record({ avg_latency: 21, target_id: 'target-a' }),
        record({ avg_latency: 35, target_id: 'target-b' }),
        record({ target_id: 'unknown' }),
        record({ timestamp: 'invalid' }),
        record({ timestamp: '2026-08-03T10:02:00.000Z' })
      ],
      targets,
      new Date('2026-08-03T10:01:00.000Z').getTime()
    )

    expect(data).toEqual([
      {
        target_0: 21,
        target_1: 35,
        timestamp: '2026-08-03T10:00:00.000Z'
      }
    ])
  })

  it('renders only visible Bklit series and exposes an accessible sampled table', () => {
    render(
      <LatencyChartContent
        embedded
        records={[record({ avg_latency: 21 }), record({ avg_latency: 35, target_id: 'target-b' })]}
        targets={targets}
      />
    )

    const chartData = JSON.parse(screen.getByTestId('area-chart').getAttribute('data-chart-data') ?? '[]')
    expect(chartData).toEqual([
      {
        target_0: 21,
        target_1: 35,
        timestamp: '2026-08-03T10:00:00.000Z'
      }
    ])
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-key', 'target_0')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-stroke', 'red')
    expect(screen.getByRole('figure', { name: 'latency_title' })).toBeInTheDocument()
    expect(screen.getByTestId('bklit-latency-chart')).toHaveClass('h-full', 'min-h-0', 'w-full')
    expect(screen.getByRole('table', { name: 'latency_title' })).toBeInTheDocument()
    expect(screen.getByRole('columnheader', { name: 'Target A' })).toBeInTheDocument()
    expect(screen.queryByRole('columnheader', { name: 'Target B' })).not.toBeInTheDocument()
  })
})
