import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import { buildTimelineBackground, buildTimelineGeometry, UptimeTimeline } from './uptime-timeline'

const LEFT_PIXEL_STYLE_RE = /left: \d+(?:\.\d+)?px/
const WIDTH_PIXEL_STYLE_RE = /width: \d+(?:\.\d+)?px/
const PIXEL_SNAP_TRANSFORM_RE = /transform:\s*translateZ\(0\)/

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

  it('renders every segment as a pixel-stable DOM tracker block', () => {
    const days = [
      makeEntry({ date: '2026-03-01', online_minutes: 1440, total_minutes: 1440 }),
      makeEntry({ date: '2026-03-02', online_minutes: 1400, total_minutes: 1440 }),
      makeEntry({ date: '2026-03-03', online_minutes: 1000, total_minutes: 1440 })
    ]
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} />)
    const tracker = container.querySelector('[data-uptime-timeline]')
    const paintLayer = container.querySelector('[data-uptime-track-paint]')
    const segments = container.querySelectorAll('[data-segment]')

    expect(container.querySelector('svg')).toBeNull()
    expect(tracker).toHaveStyle({ height: '28px' })
    expect(paintLayer).toBeInTheDocument()
    expect(paintLayer?.getAttribute('class')).toContain('absolute')
    expect(paintLayer?.getAttribute('class')).toContain('inset-0')
    // Composite to a GPU layer so the gradient is pixel-snapped after scroll,
    // otherwise dark-mode AA blends each color differently and segments look
    // like they have different heights.
    expect(paintLayer?.getAttribute('style')).toMatch(PIXEL_SNAP_TRANSFORM_RE)
    expect(segments).toHaveLength(3)
    for (const segment of segments) {
      expect(segment.getAttribute('class')).toContain('absolute')
      expect(segment.getAttribute('class')).toContain('top-0')
      expect(segment.getAttribute('class')).toContain('h-full')
      expect(segment.getAttribute('class')).toContain('rounded-none')
      expect(segment.getAttribute('class')).not.toContain('rounded-[1px]')
      expect(segment.getAttribute('class')).not.toContain('bg-')
      expect(segment.getAttribute('style')).toMatch(LEFT_PIXEL_STYLE_RE)
      expect(segment.getAttribute('style')).toMatch(WIDTH_PIXEL_STYLE_RE)
    }
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

describe('buildTimelineGeometry', () => {
  it('preserves the requested visual gap between adjacent segments', () => {
    const geometry = buildTimelineGeometry({ count: 5, gap: 1.5, width: 101 })

    expect(geometry).toHaveLength(5)
    for (let i = 0; i < geometry.length - 1; i += 1) {
      const gap = geometry[i + 1].x - (geometry[i].x + geometry[i].width)
      expect(gap).toBeCloseTo(1.5)
    }
  })

  it('keeps the final segment inside the measured track width', () => {
    const geometry = buildTimelineGeometry({ count: 5, gap: 1.5, width: 100 })
    const lastSegment = geometry.at(-1)

    expect(lastSegment).toBeDefined()
    expect((lastSegment?.x ?? 0) + (lastSegment?.width ?? 0)).toBeCloseTo(100)
  })

  it('returns an empty array when count or width is non-positive', () => {
    expect(buildTimelineGeometry({ count: 0, gap: 1.5, width: 100 })).toEqual([])
    expect(buildTimelineGeometry({ count: 5, gap: 1.5, width: 0 })).toEqual([])
  })
})

describe('buildTimelineBackground', () => {
  it('renders all colored blocks in one hard-stop background layer', () => {
    const geometry = [
      { x: 0, width: 10 },
      { x: 12, width: 10 },
      { x: 24, width: 10 }
    ]

    expect(buildTimelineBackground({ colors: ['green', 'yellow', 'red'], geometry })).toBe(
      'linear-gradient(to right, var(--uptime-operational) 0px 10px, transparent 10px 12px, var(--uptime-degraded) 12px 22px, transparent 22px 24px, var(--uptime-down) 24px 34px)'
    )
  })

  it('uses the muted token for no-data blocks', () => {
    expect(buildTimelineBackground({ colors: ['gray'], geometry: [{ x: 0, width: 10 }] })).toBe(
      'linear-gradient(to right, var(--color-muted) 0px 10px)'
    )
  })
})
