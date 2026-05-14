import { act, render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import { NetworkSquareGrid } from './network-square-grid'
import type { ServerCardMetricPoint } from './server-card-network-data'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

type ObserveCallback = (entries: Array<{ contentRect: { width: number } }>) => void

const observers: ObserveCallback[] = []

class TestResizeObserver {
  constructor(cb: ObserveCallback) {
    observers.push(cb)
  }
  observe(): void {
    // no-op: the mock invokes the callback manually from the test
  }
  unobserve(): void {
    // no-op
  }
  disconnect(): void {
    // no-op
  }
}

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
    timestamp: new Date().toISOString(),
    value
  }
}

describe('NetworkSquareGrid', () => {
  afterEach(() => {
    observers.length = 0
  })

  it('renders no more squares than the container can fit', () => {
    // @ts-expect-error inject mock
    globalThis.ResizeObserver = TestResizeObserver

    const points = Array.from({ length: 30 }, (_, i) => makePoint(50 + i))

    const { container } = render(
      <TooltipProvider>
        <NetworkSquareGrid kind="latency" points={points} />
      </TooltipProvider>
    )

    // Simulate a container width of 80px → fits floor((80 + 2) / 8) = 10 squares.
    act(() => {
      observers[0]?.([{ contentRect: { width: 80 } }])
    })

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(10)
  })

  it('renders at least one square even at zero width', () => {
    // @ts-expect-error inject mock
    globalThis.ResizeObserver = TestResizeObserver

    const points = [makePoint(50)]

    const { container } = render(
      <TooltipProvider>
        <NetworkSquareGrid kind="loss" points={points} />
      </TooltipProvider>
    )

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(1)
  })
})
