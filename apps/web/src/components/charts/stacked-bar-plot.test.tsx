import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('./bar-chart', () => ({
  BarChart: ({
    children,
    maxBarWidth,
    orientation,
    stacked,
    xDataKey
  }: {
    children: ReactNode
    maxBarWidth?: number
    orientation?: string
    stacked?: boolean
    xDataKey?: string
  }) => (
    <div
      data-max-bar-width={maxBarWidth}
      data-orientation={orientation}
      data-stacked={String(stacked)}
      data-testid="bar-chart"
      data-x-data-key={xDataKey}
    >
      {children}
    </div>
  )
}))
vi.mock('./bar', () => ({
  Bar: ({ animate, dataKey, fill }: { animate: boolean; dataKey: string; fill: string }) => (
    <div data-animate={String(animate)} data-fill={fill} data-testid={`bar-${dataKey}`} />
  )
}))
vi.mock('./grid', () => ({
  Grid: ({ horizontal, vertical }: { horizontal: boolean; vertical: boolean }) => (
    <div data-horizontal={String(horizontal)} data-testid="grid" data-vertical={String(vertical)} />
  )
}))
vi.mock('./tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('./tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('./bar-x-axis', () => ({ BarXAxis: () => <div data-testid="bar-x-axis" /> }))
vi.mock('./bar-y-axis', () => ({ BarYAxis: () => <div data-testid="bar-y-axis" /> }))
vi.mock('./bar-value-axis', () => ({ BarValueAxis: () => <div data-testid="bar-value-axis" /> }))
vi.mock('./y-axis', () => ({ YAxis: () => <div data-testid="y-axis" /> }))

const { StackedBarPlot } = await import('./stacked-bar-plot')

const series = [
  { dataKey: 'bytes_in', label: 'Inbound', color: 'red' },
  { dataKey: 'bytes_out', label: 'Outbound', color: 'blue' }
]

describe('StackedBarPlot', () => {
  it('stacks every series and mirrors the data in a screen-reader table', () => {
    const data = Array.from({ length: 60 }, (_, index) => ({
      date: `2026-08-${String((index % 28) + 1).padStart(2, '0')}-${index}`,
      bytes_in: index,
      bytes_out: index * 2
    }))

    render(
      <StackedBarPlot
        ariaLabel="Traffic"
        categoryKey="date"
        categoryLabel="Date"
        className="h-full"
        data={data}
        formatValue={(value) => `${value} B`}
        series={series}
      />
    )

    const chart = screen.getByTestId('bar-chart')

    expect(chart).toHaveAttribute('data-stacked', 'true')
    expect(chart).toHaveAttribute('data-orientation', 'vertical')
    expect(chart).toHaveAttribute('data-x-data-key', 'date')
    expect(chart).toHaveAttribute('data-max-bar-width', '40')
    expect(screen.getByTestId('bar-bytes_in')).toHaveAttribute('data-animate', 'false')
    expect(screen.getByTestId('bar-bytes_out')).toHaveAttribute('data-fill', 'blue')
    expect(screen.getByTestId('grid')).toHaveAttribute('data-horizontal', 'true')
    expect(screen.getByTestId('bar-x-axis')).toBeInTheDocument()
    expect(screen.getByTestId('y-axis')).toBeInTheDocument()
    expect(screen.getAllByText('Inbound')).toHaveLength(2)
    expect(screen.getByRole('table', { name: 'Traffic' })).toBeInTheDocument()
    // 50 sampled rows + header
    expect(screen.getAllByRole('row')).toHaveLength(51)
  })

  it('switches to the categorical y-axis and value x-axis when horizontal', () => {
    render(
      <StackedBarPlot
        ariaLabel="Cycles"
        categoryKey="period"
        categoryLabel="Billing cycle"
        className="h-[260px]"
        data={[{ period: '2026-07', bytes_in: 1, bytes_out: 2 }]}
        formatValue={(value) => `${value} B`}
        maxBarWidth={24}
        orientation="horizontal"
        series={series}
      />
    )

    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-orientation', 'horizontal')
    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-max-bar-width', '24')
    expect(screen.getByTestId('grid')).toHaveAttribute('data-vertical', 'true')
    expect(screen.getByTestId('bar-y-axis')).toBeInTheDocument()
    expect(screen.getByTestId('bar-value-axis')).toBeInTheDocument()
    expect(screen.queryByTestId('y-axis')).not.toBeInTheDocument()
  })
})
