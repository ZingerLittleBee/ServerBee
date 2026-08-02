import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { NetworkSquareGrid } from './network-square-grid'
import type { ServerCardMetricPoint } from './server-card-network-data'

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

function renderGrid(kind: 'latency' | 'loss', points: readonly ServerCardMetricPoint[]) {
  return render(
    <TooltipProvider>
      <NetworkSquareGrid kind={kind} points={points} />
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
    expect(square?.style.backgroundColor).not.toBe('var(--color-border)')
  })

  it('renders the unknown color for points without a value', () => {
    const emptyPoint: ServerCardMetricPoint = {
      synthetic: true,
      targets: [],
      timestamp: 'padding-0',
      value: null
    }

    const { container } = renderGrid('latency', [emptyPoint])

    const square = container.querySelector<HTMLElement>('[data-testid="square"]')
    expect(square?.style.backgroundColor).toBe('var(--color-border)')
  })

  it('renders at least one square even at zero width', () => {
    const { container } = renderGrid('loss', [makePoint(50)])

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(1)
  })

  it('exposes the grid as a single image with a summary label instead of per-square tab stops', () => {
    const points = Array.from({ length: 30 }, (_, i) => makePoint(50 + i))

    const { container } = renderGrid('latency', points)

    expect(screen.getByRole('img')).toHaveAccessibleName('Latency history: 30 samples, latest 79ms, 0 abnormal')
    expect(container.querySelectorAll('[tabindex], button, a')).toHaveLength(0)
  })

  it('counts squares that are neither healthy nor unknown as abnormal', () => {
    const points = [makePoint(50), makePoint(800), makePoint(120, 1)]

    renderGrid('latency', points)

    expect(screen.getByRole('img')).toHaveAccessibleName('Latency history: 3 samples, latest 120ms, 2 abnormal')
  })

  it('summarizes packet loss as a percentage', () => {
    const points = [makePoint(50, 0), makePoint(50, 0.12)]

    renderGrid('loss', [
      { ...points[0], value: 0 },
      { ...points[1], value: 0.12 }
    ])

    expect(screen.getByRole('img')).toHaveAccessibleName('Packet loss history: 2 samples, latest 12.0%, 1 abnormal')
  })
})
