import { render, screen } from '@testing-library/react'
import { Children, isValidElement, type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/lib/server-catalog'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: Record<string, string | number>) => {
      if (key === 'widgets.topN.title') {
        return `Top ${options?.metric ?? ''}`
      }
      if (key === 'widgets.topN.empty.noServers') {
        return 'No online servers'
      }
      if (key === 'common.metrics.cpu') {
        return 'CPU'
      }
      if (key === 'common.labels.server') {
        return 'Server'
      }
      return key
    }
  })
}))

function chartChildrenWithoutBarLabels(children: ReactNode): ReactNode[] {
  const kept: ReactNode[] = []
  Children.forEach(children, (child) => {
    if (!isValidElement(child)) {
      kept.push(child)
      return
    }
    const type = child.type as { displayName?: string; name?: string }
    if (type.displayName === 'TopNBarLabels' || type.name === 'TopNBarLabels') {
      return
    }
    kept.push(child)
  })
  return kept
}

vi.mock('@/components/charts/bar-chart', () => ({
  BarChart: ({
    children,
    data,
    margin,
    orientation,
    valueDomain,
    xDataKey
  }: {
    children?: ReactNode
    data: Record<string, unknown>[]
    margin?: { left?: number }
    orientation?: string
    valueDomain?: [number, number]
    xDataKey?: string
  }) => (
    <div
      data-margin-left={margin?.left ?? ''}
      data-orientation={orientation}
      data-rows={data.length}
      data-testid="bar-chart"
      data-value-domain={valueDomain ? JSON.stringify(valueDomain) : ''}
      data-x-key={xDataKey}
    >
      {/* Drop post-overlay labels — they need a live ChartProvider. */}
      {chartChildrenWithoutBarLabels(children)}
    </div>
  )
}))

vi.mock('@/components/charts/bar', () => ({
  Bar: ({ dataKey, fill, lineCap }: { dataKey: string; fill?: string; lineCap?: number }) => (
    <div data-fill={fill} data-key={dataKey} data-line-cap={lineCap} data-testid="bar-series" />
  )
}))

vi.mock('@/components/charts/bar-value-axis', () => ({ BarValueAxis: () => <div data-testid="bar-value-axis" /> }))
vi.mock('@/components/charts/grid', () => ({ Grid: () => null }))
vi.mock('@/components/charts/tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('@/components/charts/tooltip/tooltip-content', () => ({ TooltipContent: () => null }))
vi.mock('@/components/charts/chart-context', () => ({
  useChart: () => {
    throw new Error('useChart should not run in unit tests for TopNWidget')
  },
  useChartStable: () => {
    throw new Error('useChartStable should not run in unit tests for TopNWidget')
  }
}))

const { TopNWidget } = await import('./top-n')

function makeServer(id: string, overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id,
    name: `Server ${id}`,
    online: true,
    cpu: 42,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    swap_used: 0,
    swap_total: 0,
    disk_used: 20_000_000_000,
    disk_total: 40_000_000_000,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: 1024,
    net_out_speed: 2048,
    net_in_transfer: 1,
    net_out_transfer: 1,
    load1: 0.5,
    load5: 0.4,
    load15: 0.3,
    tcp_conn: 10,
    udp_conn: 5,
    process_count: 100,
    uptime: 86_400,
    country_code: 'US',
    os: 'Linux',
    cpu_name: 'Test CPU',
    last_active: Date.now(),
    region: null,
    group_id: null,
    ...overrides
  }
}

describe('TopNWidget', () => {
  it('renders a horizontal bar chart ranked by the selected metric', () => {
    render(
      <TopNWidget
        config={{ metric: 'cpu', count: 3, sort: 'desc' }}
        servers={[
          makeServer('a', { cpu: 10, name: 'Low' }),
          makeServer('b', { cpu: 90, name: 'High' }),
          makeServer('c', { cpu: 50, name: 'Mid' }),
          makeServer('d', { online: false, cpu: 99, name: 'Offline' })
        ]}
      />
    )

    expect(screen.getByRole('heading', { name: 'Top CPU' })).toBeInTheDocument()
    expect(screen.getByTestId('top-n-bar-chart')).toBeInTheDocument()

    const chart = screen.getByTestId('bar-chart')
    expect(chart).toHaveAttribute('data-orientation', 'horizontal')
    expect(chart).toHaveAttribute('data-rows', '3')
    expect(chart).toHaveAttribute('data-x-key', 'id')
    expect(chart).toHaveAttribute('data-value-domain', '[0,100]')
    // No reserved category axis — names render on the bars via TopNBarLabels.
    expect(chart).toHaveAttribute('data-margin-left', '8')
    expect(screen.queryByTestId('bar-y-axis')).not.toBeInTheDocument()

    expect(screen.getByTestId('bar-series')).toHaveAttribute('data-key', 'value')
    expect(screen.getByTestId('bar-series')).toHaveAttribute('data-fill', 'var(--chart-1)')
    expect(screen.getByTestId('bar-series')).toHaveAttribute('data-line-cap', '5')
    expect(screen.getByTestId('bar-value-axis')).toBeInTheDocument()

    // Accessible table lists rank order high → low (desc).
    const rows = screen.getAllByRole('row')
    expect(rows[1]).toHaveTextContent('High')
    expect(rows[1]).toHaveTextContent('90.0%')
    expect(rows[2]).toHaveTextContent('Mid')
    expect(rows[3]).toHaveTextContent('Low')
  })

  it('shows an empty state when no online servers are available', () => {
    render(<TopNWidget config={{ metric: 'cpu' }} servers={[makeServer('offline', { online: false })]} />)

    expect(screen.getByText('No online servers')).toBeInTheDocument()
    expect(screen.queryByTestId('top-n-bar-chart')).not.toBeInTheDocument()
  })

  it('uses an open value domain for bandwidth metrics', () => {
    render(
      <TopNWidget
        config={{ metric: 'bandwidth', count: 2 }}
        servers={[
          makeServer('a', { net_in_speed: 1000, net_out_speed: 2000 }),
          makeServer('b', { net_in_speed: 500, net_out_speed: 500 })
        ]}
      />
    )

    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-value-domain', '')
  })
})
