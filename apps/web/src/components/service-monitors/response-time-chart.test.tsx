import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('@/components/charts/area-chart', () => ({
  AreaChart: ({ children, data }: { children?: ReactNode; data: Record<string, unknown>[] }) => (
    <div data-chart-data={JSON.stringify(data)} data-testid="area-chart">
      {children}
    </div>
  )
}))

vi.mock('@/components/charts/area', () => ({
  Area: ({ dataKey }: { dataKey: string }) => <div data-key={dataKey} data-testid="area-series" />
}))

vi.mock('@/components/charts/grid', () => ({ Grid: () => null }))
vi.mock('@/components/charts/tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('@/components/charts/tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('@/components/charts/x-axis', () => ({ XAxis: () => null }))
vi.mock('@/components/charts/y-axis', () => ({ YAxis: () => null }))

const { ResponseTimeChart } = await import('./response-time-chart')

describe('ResponseTimeChart', () => {
  it('preserves chronological order and failed-check gaps for Bklit', () => {
    render(
      <ResponseTimeChart
        records={[
          { latency: 0, success: false, time: '2026-08-03T10:01:00Z' },
          { latency: 12.3, success: true, time: '2026-08-03T10:00:00Z' }
        ]}
        t={(key) => key}
      />
    )

    const chartData = JSON.parse(screen.getByTestId('area-chart').getAttribute('data-chart-data') ?? '[]')
    expect(chartData).toEqual([
      { latency: 12.3, success: true, timestamp: '2026-08-03T10:00:00Z' },
      { latency: null, success: false, timestamp: '2026-08-03T10:01:00Z' }
    ])
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-key', 'latency')
  })

  it('provides an accessible data table because Bklit hides its SVG', () => {
    render(
      <ResponseTimeChart records={[{ latency: null, success: false, time: '2026-08-03T10:01:00Z' }]} t={(key) => key} />
    )

    expect(screen.getByRole('figure', { name: 'chart.responseTime' })).toBeInTheDocument()
    expect(screen.getByRole('table', { name: 'chart.responseTime' })).toBeInTheDocument()
    expect(screen.getAllByRole('cell', { name: 'history.status.fail' })).toHaveLength(2)
  })
})
