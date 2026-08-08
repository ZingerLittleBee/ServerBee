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
  Area: ({
    curve,
    dataKey,
    fillOpacity,
    stroke
  }: {
    curve?: unknown
    dataKey: string
    fillOpacity?: number
    stroke: string
  }) => (
    <div
      data-curve={curve == null ? 'default' : 'custom'}
      data-fill-opacity={fillOpacity}
      data-key={dataKey}
      data-stroke={stroke}
      data-testid="area-series"
    />
  )
}))
vi.mock('./grid', () => ({ Grid: () => null }))
vi.mock('./tooltip/chart-tooltip', () => ({
  ChartTooltip: ({
    formatDatePill,
    showDatePill
  }: {
    formatDatePill?: (date: Date) => string
    showDatePill?: boolean
  }) => (
    <div
      data-date-pill-label={formatDatePill?.(new Date('2026-08-03T10:15:00.000Z'))}
      data-show-date-pill={showDatePill === false ? 'false' : 'true'}
      data-testid="chart-tooltip"
    />
  )
}))
vi.mock('./tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('./x-axis', () => ({
  XAxis: ({ fadeOnHover, tickMode }: { fadeOnHover?: boolean; tickMode: string }) => (
    <div data-fade-on-hover={fadeOnHover === false ? 'false' : 'true'} data-testid="x-axis" data-tick-mode={tickMode} />
  )
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
        data={data}
        formatTime={(time) => time.slice(11, 16)}
        formatTooltipLabel={(time) => time}
        formatValue={(value) => `${value.toFixed(1)}%`}
        series={[{ dataKey: 'value', label: 'CPU usage', color: 'red' }]}
        timeLabel="Time"
        yDomain={[0, 100]}
      />
    )

    expect(screen.getByTestId('area-chart')).toHaveAttribute('data-y-domain', '[0,100]')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-key', 'value')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-stroke', 'red')
    expect(screen.getByTestId('x-axis')).toHaveAttribute('data-tick-mode', 'domain')
    expect(screen.getByTestId('x-axis')).toHaveAttribute('data-fade-on-hover', 'true')
    expect(screen.getByTestId('chart-tooltip')).toHaveAttribute('data-show-date-pill', 'true')
    expect(screen.getByTestId('chart-tooltip')).toHaveAttribute('data-date-pill-label', '10:15')
    expect(screen.getByRole('figure', { name: 'CPU usage' })).toHaveClass('min-w-0')
    expect(screen.getByRole('table', { name: 'CPU usage' })).toBeInTheDocument()
    expect(screen.getAllByRole('row')).toHaveLength(51)
    expect(screen.queryByRole('list')).not.toBeInTheDocument()
  })

  it('renders one area per series with a legend and a column per series', () => {
    render(
      <MetricAreaPlot
        ariaLabel="Traffic"
        className="h-full"
        data={[{ date: '2026-08-01', bytes_in: 10, bytes_out: 20 }]}
        formatTooltipLabel={(date) => date}
        formatValue={(value) => `${value} B`}
        series={[
          { dataKey: 'bytes_in', label: 'Inbound', color: 'blue' },
          { dataKey: 'bytes_out', label: 'Outbound', color: 'green' }
        ]}
        timeKey="date"
        timeLabel="Date"
      />
    )

    expect(screen.getAllByTestId('area-series')).toHaveLength(2)
    expect(screen.getByRole('list')).toBeInTheDocument()
    // legend entry + table header + table cell
    expect(screen.getAllByText('Inbound')).toHaveLength(2)
    expect(screen.getAllByRole('columnheader')).toHaveLength(3)
    expect(screen.getByRole('cell', { name: '2026-08-01' })).toBeInTheDocument()
  })

  it('forwards curve and fillOpacity to every series area', () => {
    const stepCurve = () => null

    render(
      <MetricAreaPlot
        ariaLabel="Traffic"
        className="h-full"
        curve={stepCurve as never}
        data={[{ date: '2026-08-01', bytes_in: 10, bytes_out: 20 }]}
        fillOpacity={0.3}
        formatTooltipLabel={(date) => date}
        formatValue={(value) => `${value} B`}
        series={[
          { dataKey: 'bytes_in', label: 'Inbound', color: 'blue' },
          { dataKey: 'bytes_out', label: 'Outbound', color: 'green' }
        ]}
        timeKey="date"
        timeLabel="Date"
      />
    )

    for (const series of screen.getAllByTestId('area-series')) {
      expect(series).toHaveAttribute('data-curve', 'custom')
      expect(series).toHaveAttribute('data-fill-opacity', '0.3')
    }
  })
})
