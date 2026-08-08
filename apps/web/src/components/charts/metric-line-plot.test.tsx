import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('./line-chart', () => ({
  LineChart: ({ children, yDomain }: { children: ReactNode; yDomain?: [number, number] }) => (
    <div data-testid="line-chart" data-y-domain={JSON.stringify(yDomain)}>
      {children}
    </div>
  )
}))
vi.mock('./line', () => ({
  Line: ({
    animate = true,
    connectNulls,
    dataKey,
    fadeEdges,
    stroke
  }: {
    animate?: boolean
    connectNulls: boolean
    dataKey: string
    fadeEdges: boolean
    stroke: string
  }) => (
    <div
      data-animate={String(animate)}
      data-connect-nulls={String(connectNulls)}
      data-fade-edges={String(fadeEdges)}
      data-key={dataKey}
      data-stroke={stroke}
      data-testid="line-series"
    />
  )
}))
vi.mock('./grid', () => ({ Grid: () => null }))
vi.mock('./tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('./tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('./x-axis', () => ({
  XAxis: ({ tickMode }: { tickMode: string }) => <div data-testid="x-axis" data-tick-mode={tickMode} />
}))
vi.mock('./y-axis', () => ({ YAxis: () => null }))

const { MetricLinePlot } = await import('./metric-line-plot')

describe('MetricLinePlot', () => {
  it('renders bounded accessible rows and morphable series for smooth data updates', () => {
    const data = Array.from({ length: 60 }, (_, index) => ({
      timestamp: `2026-08-03T10:${String(index).padStart(2, '0')}:00.000Z`,
      read: index,
      write: index * 2
    }))

    render(
      <MetricLinePlot
        ariaLabel="Disk I/O"
        className="h-full"
        data={data}
        formatTooltipLabel={(time) => time}
        formatValue={(value) => `${value} B/s`}
        series={[
          { dataKey: 'read', label: 'Read', color: 'red' },
          { dataKey: 'write', label: 'Write', color: 'blue' }
        ]}
        timeLabel="Time"
        yDomain={[0, 100]}
      />
    )

    expect(screen.getByRole('figure', { name: 'Disk I/O' })).toHaveClass('min-w-0')
    expect(screen.getByTestId('line-chart')).toHaveAttribute('data-y-domain', '[0,100]')
    expect(screen.getByTestId('x-axis')).toHaveAttribute('data-tick-mode', 'domain')
    expect(screen.getAllByTestId('line-series')).toHaveLength(2)
    for (const line of screen.getAllByTestId('line-series')) {
      // animate defaults on so range changes morph without remounting the chart.
      expect(line).toHaveAttribute('data-animate', 'true')
      expect(line).toHaveAttribute('data-connect-nulls', 'true')
      expect(line).toHaveAttribute('data-fade-edges', 'false')
    }
    expect(screen.getAllByText('Read')).toHaveLength(2)
    expect(screen.getAllByText('Write')).toHaveLength(2)
    expect(screen.getByRole('table', { name: 'Disk I/O' })).toBeInTheDocument()
    expect(screen.getAllByRole('row')).toHaveLength(51)
  })
})
