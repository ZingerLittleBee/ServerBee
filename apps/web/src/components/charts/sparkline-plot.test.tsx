import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('./area-chart', () => ({
  AreaChart: ({ children, xDataKey }: { children: ReactNode; xDataKey?: string }) => (
    <div data-testid="area-chart" data-x-data-key={xDataKey}>
      {children}
    </div>
  )
}))
vi.mock('./area', () => ({
  Area: ({ animate, dataKey, stroke }: { animate: boolean; dataKey: string; stroke: string }) => (
    <div data-animate={String(animate)} data-key={dataKey} data-stroke={stroke} data-testid="area-series" />
  )
}))

const { SparklinePlot } = await import('./sparkline-plot')

describe('SparklinePlot', () => {
  it('renders a single decorative series hidden from assistive tech', () => {
    render(
      <SparklinePlot
        className="h-full"
        color="var(--chart-1)"
        data={[
          { t: 1, v: 10 },
          { t: 2, v: 20 }
        ]}
        dataKey="v"
      />
    )

    expect(screen.getByTestId('bklit-sparkline')).toHaveAttribute('aria-hidden', 'true')
    expect(screen.getByTestId('area-chart')).toHaveAttribute('data-x-data-key', 't')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-key', 'v')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-animate', 'false')
    expect(screen.getByTestId('area-series')).toHaveAttribute('data-stroke', 'var(--chart-1)')
  })
})
