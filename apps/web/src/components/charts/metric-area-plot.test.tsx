import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('./area-chart', () => ({
  AreaChart: ({ children, yDomain }: { children: ReactNode; yDomain?: [number, number] }) => (
    <div data-testid="area-chart" data-y-domain={JSON.stringify(yDomain)}>
      {children}
    </div>
  )
}))
vi.mock('./area', () => ({
  Area: ({ dataKey, stroke }: { dataKey: string; stroke: string }) => (
    <div data-key={dataKey} data-stroke={stroke} data-testid="area-series" />
  )
}))
vi.mock('./grid', () => ({ Grid: () => null }))
vi.mock('./tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('./tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('./x-axis', () => ({
  XAxis: ({ tickMode }: { tickMode: string }) => <div data-testid="x-axis" data-tick-mode={tickMode} />
}))
vi.mock('./y-axis', () => ({ YAxis: () => null }))

const { MetricAreaPlot } = await import('./metric-area-plot')

describe('MetricAreaPlot', () => {
  it('forwards the fixed domain and exposes a bounded accessible table', () => {
    const data = Array.from({ length: 60 }, (_, index) => ({
      timestamp: `2026-08-03T10:${String(index).padStart(2, '0')}:00.000Z`,
      value: index
    }))

    render(
      <MetricAreaPlot
        ariaLabel="CPU usage"
        className="h-full"
        color="red"
        data={data}
        dataKey="value"
        formatTooltipLabel={(time) => time}
        formatValue={(value) => `${value.toFixed(1)}%`}
        timeLabel="Time"
        valueLabel="CPU usage"
        yDomain={[0, 100]}
      />
    )

    expect(screen.getByTestId('area-chart')).toHaveAttribute('data-y-domain', '[0,100]')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-key', 'value')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-stroke', 'red')
    expect(screen.getByTestId('x-axis')).toHaveAttribute('data-tick-mode', 'domain')
    expect(screen.getByRole('figure', { name: 'CPU usage' })).toHaveClass('min-w-0')
    expect(screen.getByRole('table', { name: 'CPU usage' })).toBeInTheDocument()
    expect(screen.getAllByRole('row')).toHaveLength(51)
  })
})
