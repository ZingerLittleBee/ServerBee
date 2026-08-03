import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeAggregateUptime } from '@/lib/widget-helpers'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, options?: { count?: number }) => {
      switch (key) {
        case 'uptime_days_ago':
          return `${options?.count ?? 0} days ago`
        case 'uptime_today':
          return 'Today'
        case 'uptime_operational':
          return 'Operational'
        case 'uptime_degraded':
          return 'Degraded'
        case 'uptime_down':
          return 'Down'
        case 'uptime_no_data':
          return 'No data'
        case 'uptime_date':
          return 'Date'
        case 'uptime_status':
          return 'Status'
        default:
          return key
      }
    }
  })
}))

vi.mock('@/components/charts/bar-chart', () => ({
  BarChart: ({
    children,
    data,
    valueDomain
  }: {
    children: ReactNode
    data: Record<string, unknown>[]
    valueDomain?: [number, number]
  }) => (
    <div data-count={data.length} data-testid="bar-chart" data-value-domain={JSON.stringify(valueDomain)}>
      {children}
    </div>
  )
}))
vi.mock('@/components/charts/bar', () => ({
  Bar: ({ dataKey, fill, lineCap }: { dataKey: string; fill: string; lineCap: number }) => (
    <div data-fill={fill} data-key={dataKey} data-line-cap={lineCap} data-testid="bar-series" />
  )
}))
vi.mock('@/components/charts/tooltip/chart-tooltip', () => ({ ChartTooltip: () => null }))
vi.mock('@/components/charts/tooltip/tooltip-content', () => ({ TooltipContent: () => null }))

const { UptimeTimeline } = await import('./uptime-timeline')

function makeEntry(overrides: Partial<UptimeDailyEntry> = {}): UptimeDailyEntry {
  return {
    date: '2026-03-20',
    online_minutes: 1440,
    total_minutes: 1440,
    downtime_incidents: 0,
    ...overrides
  }
}

function makeEntries(count: number, overrides: Partial<UptimeDailyEntry> = {}): UptimeDailyEntry[] {
  return Array.from({ length: count }, (_, i) =>
    makeEntry({
      date: `2026-03-${String(i + 1).padStart(2, '0')}`,
      ...overrides
    })
  )
}

/** Status cells of the screen-reader table, one row per rendered day. */
function statusCells(): string[] {
  return screen
    .getAllByRole('row')
    .slice(1)
    .map((row) => row.querySelectorAll('td')[1]?.textContent ?? '')
}

describe('UptimeTimeline', () => {
  it('renders one full-height column per day across the four status series', () => {
    render(<UptimeTimeline days={makeEntries(30)} rangeDays={30} />)

    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-count', '30')
    // Bars must span the whole plot height, so the domain skips the auto headroom.
    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-value-domain', '[0,1]')

    const series = screen.getAllByTestId('bar-series')
    expect(series.map((s) => s.getAttribute('data-key'))).toEqual(['green', 'yellow', 'red', 'gray'])
    // Square segment ends, matching the previous painted track.
    expect(series[0]).toHaveAttribute('data-line-cap', '0')
    expect(series[0]).toHaveAttribute('data-fill', 'var(--uptime-operational)')
  })

  it('classifies each day into a status series', () => {
    const days = [
      makeEntry({ date: '2026-03-01', online_minutes: 1440, total_minutes: 1440 }),
      makeEntry({ date: '2026-03-02', online_minutes: 1400, total_minutes: 1440 }),
      makeEntry({ date: '2026-03-03', online_minutes: 1000, total_minutes: 1440 }),
      makeEntry({ date: '2026-03-04', online_minutes: 0, total_minutes: 0 })
    ]
    render(<UptimeTimeline days={days} rangeDays={4} />)

    expect(statusCells()).toEqual(['Operational', 'Degraded', 'Down', 'No data'])
  })

  it('respects custom thresholds', () => {
    // 98.6% uptime is operational once the yellow threshold drops to 98.
    render(
      <UptimeTimeline
        days={makeEntries(3, { online_minutes: 1420, total_minutes: 1440 })}
        rangeDays={3}
        redThreshold={90}
        yellowThreshold={98}
      />
    )

    expect(statusCells()).toEqual(['Operational', 'Operational', 'Operational'])
  })

  it('pads missing days with no-data columns', () => {
    render(<UptimeTimeline days={makeEntries(5)} rangeDays={10} />)

    expect(screen.getByTestId('bar-chart')).toHaveAttribute('data-count', '10')
    expect(statusCells().filter((status) => status === 'No data')).toHaveLength(5)
  })

  it('exposes the timeline as a labelled figure with a screen-reader table', () => {
    render(<UptimeTimeline days={makeEntries(90)} rangeDays={90} />)

    expect(screen.getByRole('figure', { name: '90 days ago - Today' })).toBeInTheDocument()
    // 50-row accessibility sample plus the header row.
    expect(screen.getAllByRole('row')).toHaveLength(51)
  })

  it('shows labels when showLabels is true', () => {
    render(<UptimeTimeline days={makeEntries(90)} rangeDays={90} showLabels />)

    expect(screen.getByText('90 days ago')).toBeInTheDocument()
    expect(screen.getByText('Today')).toBeInTheDocument()
  })

  it('shows legend when showLegend is true', () => {
    render(<UptimeTimeline days={makeEntries(30)} rangeDays={30} showLegend />)

    // Each label also appears in the screen-reader table, so match the swatch row.
    const legend = screen.getByText('Down').closest('div')
    expect(legend).toHaveTextContent('Operational')
    expect(legend).toHaveTextContent('Degraded')
    expect(legend).toHaveTextContent('No data')
  })
})

describe('computeAggregateUptime', () => {
  it('returns null when all total_minutes are 0', () => {
    const days = makeEntries(3, { online_minutes: 0, total_minutes: 0 })
    expect(computeAggregateUptime(days)).toBeNull()
  })

  it('computes correct aggregate percentage', () => {
    const days = [
      makeEntry({ online_minutes: 1440, total_minutes: 1440 }),
      makeEntry({ online_minutes: 1400, total_minutes: 1440 })
    ]
    const result = computeAggregateUptime(days)
    expect(result).toBeCloseTo(98.61, 1)
  })

  it('returns 100 for full uptime', () => {
    const days = makeEntries(5)
    expect(computeAggregateUptime(days)).toBe(100)
  })
})
