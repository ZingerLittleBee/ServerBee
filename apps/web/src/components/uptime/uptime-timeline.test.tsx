import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import { UptimeTimeline } from './uptime-timeline'

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
        default:
          return key
      }
    }
  })
}))

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

describe('UptimeTimeline', () => {
  it('renders correct number of segments', () => {
    const days = makeEntries(30)
    const { container } = render(<UptimeTimeline days={days} rangeDays={30} />)
    const segments = container.querySelectorAll('[data-segment]')
    expect(segments).toHaveLength(30)
  })

  it('renders green segments for 100% uptime', () => {
    const days = makeEntries(10)
    const { container } = render(<UptimeTimeline days={days} rangeDays={10} />)
    const segments = container.querySelectorAll('[data-segment="green"]')
    expect(segments).toHaveLength(10)
  })

  it('renders yellow segments for degraded uptime', () => {
    const days = makeEntries(5, { online_minutes: 1400, total_minutes: 1440 })
    const { container } = render(<UptimeTimeline days={days} rangeDays={5} />)
    const segments = container.querySelectorAll('[data-segment="yellow"]')
    expect(segments).toHaveLength(5)
  })

  it('renders red segments for low uptime', () => {
    const days = makeEntries(5, { online_minutes: 1000, total_minutes: 1440 })
    const { container } = render(<UptimeTimeline days={days} rangeDays={5} />)
    const segments = container.querySelectorAll('[data-segment="red"]')
    expect(segments).toHaveLength(5)
  })

  it('renders gray segments for no data', () => {
    const days = makeEntries(5, { online_minutes: 0, total_minutes: 0 })
    const { container } = render(<UptimeTimeline days={days} rangeDays={5} />)
    const segments = container.querySelectorAll('[data-segment="gray"]')
    expect(segments).toHaveLength(5)
  })

  it('respects custom thresholds', () => {
    // 98.6% uptime should be green with threshold of 98
    const days = makeEntries(3, { online_minutes: 1420, total_minutes: 1440 })
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} redThreshold={90} yellowThreshold={98} />)
    const segments = container.querySelectorAll('[data-segment="green"]')
    expect(segments).toHaveLength(3)
  })

  it('shows labels when showLabels is true', () => {
    const days = makeEntries(90)
    render(<UptimeTimeline days={days} rangeDays={90} showLabels />)
    expect(screen.getByText('90 days ago')).toBeInTheDocument()
    expect(screen.getByText('Today')).toBeInTheDocument()
  })

  it('shows legend when showLegend is true', () => {
    const days = makeEntries(30)
    render(<UptimeTimeline days={days} rangeDays={30} showLegend />)
    expect(screen.getByText('Operational')).toBeInTheDocument()
    expect(screen.getByText('Degraded')).toBeInTheDocument()
    expect(screen.getByText('Down')).toBeInTheDocument()
    expect(screen.getByText('No data')).toBeInTheDocument()
  })

  it('pads with gray when fewer days than rangeDays', () => {
    const days = makeEntries(5)
    const { container } = render(<UptimeTimeline days={days} rangeDays={10} />)
    const allSegments = container.querySelectorAll('[data-segment]')
    expect(allSegments).toHaveLength(10)
    const graySegments = container.querySelectorAll('[data-segment="gray"]')
    expect(graySegments).toHaveLength(5)
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
