import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { NetworkSquareGrid } from './network-square-grid'
import { buildServerCardNetworkState, type ServerCardMetricPoint } from './server-card-network-data'

function makePoint(value: number, lossRatio = 0): ServerCardMetricPoint {
  return {
    synthetic: false,
    targets: [
      {
        latency: value,
        lossRatio,
        targetId: 't1',
        targetName: 'Tokyo'
      }
    ],
    timestamp: new Date(Date.UTC(2026, 0, 1, 0, 0, value)).toISOString(),
    value
  }
}

function makeLossPoint(value: number | null, index: number): ServerCardMetricPoint {
  return {
    synthetic: false,
    targets: [],
    timestamp: new Date(Date.UTC(2026, 0, 1, 0, 0, index)).toISOString(),
    value
  }
}

function renderGrid(kind: 'latency' | 'loss', points: readonly ServerCardMetricPoint[], tooltips?: boolean) {
  return render(
    <TooltipProvider>
      <NetworkSquareGrid kind={kind} points={points} tooltips={tooltips} />
    </TooltipProvider>
  )
}

describe('NetworkSquareGrid', () => {
  it('renders all points inside the clipped grid', () => {
    const points = Array.from({ length: 30 }, (_, i) => makePoint(50 + i))

    const { container } = renderGrid('latency', points)

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(30)
    expect(container.firstElementChild).toHaveClass('overflow-hidden')
  })

  it('colors synthetic backend-history points that carry a real value', () => {
    const historyPoint: ServerCardMetricPoint = {
      synthetic: true,
      targets: [],
      timestamp: 'synthetic-0',
      value: 42
    }

    const { container } = renderGrid('latency', [historyPoint])

    const square = container.querySelector<HTMLElement>('[data-testid="square"]')
    expect(square?.style.backgroundColor).not.toBe('var(--network-grid-unknown)')
  })

  it('renders the unknown encoding for points without a value', () => {
    const emptyPoint: ServerCardMetricPoint = {
      synthetic: true,
      targets: [],
      timestamp: 'padding-0',
      value: null
    }

    const { container } = renderGrid('latency', [emptyPoint])

    const square = container.querySelector<HTMLElement>('[data-testid="square"]')
    expect(square).toHaveAttribute('data-severity', 'unknown')
    expect(square?.style.backgroundColor).toBe('var(--network-grid-unknown)')
    expect(square?.style.height).toBe('18px')
  })

  it('renders at least one square even at zero width', () => {
    const { container } = renderGrid('loss', [makePoint(50)])

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(1)
  })

  it('exposes the grid as a single image with a summary label instead of per-square tab stops', () => {
    const points = Array.from({ length: 30 }, (_, i) => makePoint(50 + i))

    const { container } = renderGrid('latency', points)

    expect(screen.getByRole('img')).toHaveAccessibleName(
      'Latency history: 30 samples, latest 79ms, 0 abnormal, 0 warning, 0 severe, 0 failed, 0 unknown'
    )
    expect(container.querySelectorAll('[tabindex], button, a')).toHaveLength(0)
  })

  it('skips tooltip wrappers when tooltips are disabled', () => {
    const points = [makePoint(50), makePoint(80)]

    const { container } = renderGrid('latency', points, false)

    expect(container.querySelectorAll('[data-testid="square"]')).toHaveLength(2)
    // TooltipTrigger would add interactive wrappers; plain markers stay non-interactive.
    expect(container.querySelectorAll('[tabindex], button, a, [data-slot="tooltip-trigger"]')).toHaveLength(0)
  })

  it('summarizes the abnormal total and per-severity breakdown in the accessible name', () => {
    const points = [makePoint(50), makePoint(800), makePoint(120, 1)]

    renderGrid('latency', points)

    // abnormal is the warning + severe + failed total derived from the same counts.
    expect(screen.getByRole('img')).toHaveAccessibleName(
      'Latency history: 3 samples, latest 120ms, 2 abnormal, 1 warning, 0 severe, 1 failed, 0 unknown'
    )
  })

  it('summarizes packet loss as a percentage', () => {
    renderGrid('loss', [makeLossPoint(0, 0), makeLossPoint(0.12, 1)])

    expect(screen.getByRole('img')).toHaveAccessibleName(
      'Packet loss history: 2 samples, latest 12.0%, 1 abnormal, 0 warning, 1 severe, 0 failed, 0 unknown'
    )
  })

  it('renders every marker as the same fixed narrow bar regardless of severity', () => {
    const points = [
      makeLossPoint(0, 0),
      makeLossPoint(0.01, 1),
      makeLossPoint(0.05, 2),
      makeLossPoint(1, 3),
      makeLossPoint(null, 4)
    ]

    const { container } = renderGrid('loss', points)

    // One marker per severity (healthy, warning, severe, failed, unknown), each rendered
    // at the identical 5x18 size with a subtle 1px radius and a 3px rhythm between markers.
    expect(screen.getByRole('img')).toHaveClass('h-[18px]')
    expect(screen.getByRole('img')).toHaveStyle({ gap: '3px' })
    const squares = Array.from(container.querySelectorAll<HTMLElement>('[data-testid="square"]'))
    expect(squares).toHaveLength(5)
    for (const square of squares) {
      expect(square.style.width).toBe('5px')
      expect(square.style.height).toBe('18px')
      expect(square).toHaveClass('rounded-[1px]')
    }
  })

  it('encodes loss severity through color and data-severity at the threshold boundaries', () => {
    const points = [
      makeLossPoint(0, 0),
      makeLossPoint(0.005, 1),
      makeLossPoint(0.01, 2),
      makeLossPoint(0.05, 3),
      makeLossPoint(1, 4),
      makeLossPoint(null, 5)
    ]

    const { container } = renderGrid('loss', points)

    // The grid is row-reversed, so the latest point renders first.
    const expected = [
      { severity: 'unknown', color: 'var(--network-grid-unknown)' },
      { severity: 'failed', color: 'var(--network-grid-failed)' },
      { severity: 'severe', color: 'var(--network-grid-severe)' },
      { severity: 'warning', color: 'var(--network-grid-warning)' },
      { severity: 'healthy', color: 'var(--network-grid-healthy)' },
      { severity: 'healthy', color: 'var(--network-grid-healthy)' }
    ]
    const squares = Array.from(container.querySelectorAll<HTMLElement>('[data-testid="square"]'))
    expect(squares).toHaveLength(expected.length)
    expected.forEach((exp, index) => {
      const square = squares[index]
      expect(square).toHaveAttribute('data-severity', exp.severity)
      expect(square?.style.backgroundColor).toBe(exp.color)
    })
    // The four-step progression keeps severe and failed visually distinct.
    expect(squares[2]?.style.backgroundColor).not.toBe(squares[1]?.style.backgroundColor)
  })

  it('marks latency squares with total packet loss as failed', () => {
    const { container } = renderGrid('latency', [makePoint(120, 1)])

    const square = container.querySelector<HTMLElement>('[data-testid="square"]')
    expect(square).toHaveAttribute('data-severity', 'failed')
    expect(square?.style.backgroundColor).toBe('var(--network-grid-failed)')
    expect(square?.style.height).toBe('18px')
  })

  describe('contract with buildServerCardNetworkState', () => {
    it('renders a 2% realtime loss sample as a warning square with a correct summary', () => {
      const state = buildServerCardNetworkState(
        {
          last_probe_at: null,
          latency_sparkline: [],
          loss_sparkline: [],
          targets: [
            {
              availability: 0.98,
              avg_latency: 50,
              max_latency: 55,
              min_latency: 45,
              packet_loss: 0.02,
              provider: 'ct',
              target_id: 'target-1',
              target_name: 'Shanghai'
            }
          ]
        },
        {
          'target-1': [
            {
              avg_latency: 50,
              max_latency: 55,
              min_latency: 45,
              packet_loss: 0.02,
              packet_received: 8,
              packet_sent: 10,
              target_id: 'target-1',
              timestamp: '2026-04-12T10:00:00Z'
            }
          ]
        }
      )

      const { container } = renderGrid('loss', state.lossPoints)

      // 29 null padding slots read as unknown; only the realtime 2% sample is a warning.
      expect(screen.getByRole('img')).toHaveAccessibleName(
        'Packet loss history: 30 samples, latest 2.0%, 1 abnormal, 1 warning, 0 severe, 0 failed, 29 unknown'
      )
      // The latest sample renders first inside the row-reversed grid.
      const latestSquare = container.querySelector<HTMLElement>('[data-testid="square"]')
      expect(latestSquare).toHaveAttribute('data-severity', 'warning')
      expect(latestSquare?.style.backgroundColor).toBe('var(--network-grid-warning)')
      expect(latestSquare?.style.height).toBe('18px')
      expect(container.querySelectorAll('[tabindex], button, a')).toHaveLength(0)
    })
  })
})
