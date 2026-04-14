import { fireEvent, render, screen, within } from '@testing-library/react'
import { createContext, type ReactNode, useContext, useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { TrafficCard } from './traffic-card'

const mockUseTraffic = vi.fn()
const TabsContext = createContext<{ setValue: (value: string) => void; value: string } | null>(null)
const translations: Record<string, string> = {
  traffic_title: 'Traffic Statistics',
  traffic_tab_today: 'Today',
  traffic_tab_cycle: 'Monthly'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) => translations[key] ?? options?.defaultValue ?? key
  })
}))

vi.mock('@/hooks/use-traffic', () => ({
  useTraffic: (...args: unknown[]) => mockUseTraffic(...args)
}))

vi.mock('@/components/ui/tabs', () => ({
  Tabs: ({ children, defaultValue }: { children?: ReactNode; defaultValue?: string }) => {
    const [value, setValue] = useState(defaultValue ?? '')
    return <TabsContext.Provider value={{ setValue, value }}>{children}</TabsContext.Provider>
  },
  TabsList: ({ children }: { children?: ReactNode }) => <div data-testid="tabs-list">{children}</div>,
  TabsTrigger: ({ children, value }: { children?: ReactNode; value: string }) => {
    const context = useContext(TabsContext)
    if (!context) {
      return null
    }

    return (
      <button data-testid={`tab-${value}`} onClick={() => context.setValue(value)} type="button">
        {children}
      </button>
    )
  },
  TabsContent: ({ children, value }: { children?: ReactNode; value: string }) => {
    const context = useContext(TabsContext)
    if (!context || context.value !== value) {
      return null
    }

    return <div data-testid={`tab-content-${value}`}>{children}</div>
  }
}))

vi.mock('recharts', () => {
  const createWrapper =
    (testId: string) =>
    ({ children }: { children?: ReactNode }) => <div data-testid={testId}>{children}</div>

  return {
    ResponsiveContainer: createWrapper('responsive-container'),
    Tooltip: ({ children, cursor }: { children?: ReactNode; cursor?: boolean }) => (
      <div data-cursor={String(cursor)} data-testid="chart-tooltip">
        {children}
      </div>
    ),
    Legend: createWrapper('chart-legend'),
    CartesianGrid: () => <div data-testid="cartesian-grid" />,
    XAxis: () => <div data-testid="x-axis" />,
    YAxis: () => <div data-testid="y-axis" />,
    BarChart: ({
      children,
      data,
      maxBarSize
    }: {
      children?: ReactNode
      data?: Record<string, unknown>[]
      maxBarSize?: number
    }) => {
      const chartKind = data?.[0] && 'hour' in data[0] ? 'hourly' : 'daily'
      return (
        <div data-max-bar-size={maxBarSize} data-testid={`bar-chart-${chartKind}`}>
          {children}
        </div>
      )
    },
    Bar: ({ dataKey, stackId }: { dataKey: string; stackId?: string }) => (
      <div data-key={dataKey} data-stack-id={stackId} data-testid={`bar-${dataKey}`} />
    )
  }
})

describe('TrafficCard', () => {
  it('renders one traffic card with tabs that switch between hourly and daily charts', () => {
    mockUseTraffic.mockReturnValue({
      isLoading: false,
      data: {
        cycle_start: '2026-03-01',
        cycle_end: '2026-03-31',
        bytes_in: 1500,
        bytes_out: 1000,
        bytes_total: 2500,
        traffic_limit: null,
        traffic_limit_type: null,
        usage_percent: null,
        prediction: null,
        daily: [{ date: '2026-03-15', bytes_in: 1000, bytes_out: 500 }],
        hourly: [{ hour: '2026-03-18T10:00:00Z', bytes_in: 300, bytes_out: 200 }]
      }
    })

    render(<TrafficCard serverId="srv-1" />)

    const card = screen.getByText('Traffic Statistics').closest('[data-slot="card"]')
    const cardHeader = within(card as HTMLElement)
      .getByText('Traffic Statistics')
      .closest('[data-slot="card-header"]')
    const cardFooter = (card as HTMLElement).querySelector('[data-slot="card-footer"]')

    expect(card).not.toBeNull()
    expect(cardHeader).not.toBeNull()
    expect(cardFooter).not.toBeNull()
    expect(screen.getByTestId('tabs-list')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Today' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Monthly' })).toBeInTheDocument()
    expect(screen.getByTestId('tab-content-hourly')).toBeInTheDocument()
    const hourlyChart = screen.getByTestId('bar-chart-hourly')

    expect(hourlyChart).toHaveAttribute('data-max-bar-size', '40')
    expect(screen.queryByTestId('bar-chart-daily')).not.toBeInTheDocument()
    expect(within(hourlyChart).getByTestId('chart-tooltip')).toHaveAttribute('data-cursor', 'false')
    expect(within(hourlyChart).getByTestId('y-axis')).toBeInTheDocument()
    expect(within(cardHeader as HTMLElement).queryByText('2026-03-01 ~ 2026-03-31')).not.toBeInTheDocument()
    expect(cardFooter?.firstElementChild).toHaveTextContent('2026-03-01 ~ 2026-03-31')
    expect(cardFooter?.lastElementChild).toHaveTextContent('↓ In 1.5 KB')
    expect(cardFooter?.lastElementChild).toHaveTextContent('↑ Out 1000.0 B')
    expect(cardFooter?.lastElementChild).toHaveTextContent('Total 2.4 KB')

    fireEvent.click(screen.getByRole('button', { name: 'Monthly' }))

    expect(screen.getByTestId('tab-content-daily')).toBeInTheDocument()
    const dailyChart = screen.getByTestId('bar-chart-daily')

    expect(dailyChart).toHaveAttribute('data-max-bar-size', '40')
    expect(screen.queryByTestId('bar-chart-hourly')).not.toBeInTheDocument()
    expect(within(dailyChart).getByTestId('chart-tooltip')).toHaveAttribute('data-cursor', 'false')
    expect(within(dailyChart).getByTestId('y-axis')).toBeInTheDocument()
    expect(within(card as HTMLElement).getByTestId('bar-bytes_in')).toHaveAttribute('data-stack-id', 'traffic')
    expect(within(card as HTMLElement).getByTestId('bar-bytes_out')).toHaveAttribute('data-stack-id', 'traffic')
  })
})
