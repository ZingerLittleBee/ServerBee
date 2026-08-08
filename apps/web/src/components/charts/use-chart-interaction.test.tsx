import { act, renderHook } from '@testing-library/react'
import { scaleLinear, scaleTime } from '@visx/scale'
import { bisector } from 'd3-array'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { LineConfig } from './chart-context'
import { useChartInteraction } from './use-chart-interaction'

const localPoint = vi.fn()

vi.mock('@visx/event', () => ({
  localPoint: (...args: unknown[]) => localPoint(...args)
}))

const data = [
  { timestamp: '2026-08-09T10:00:00.000Z', cpu: 42 },
  { timestamp: '2026-08-09T10:01:00.000Z', cpu: null }
]
const lines: LineConfig[] = [{ dataKey: 'cpu', stroke: 'red', strokeWidth: 2 }]
const xAccessor = (point: Record<string, unknown>) => new Date(String(point.timestamp))
const bisectDate = bisector<Record<string, unknown>, Date>((point) => xAccessor(point)).left
const xScale = scaleTime<number>({
  domain: data.map(xAccessor),
  range: [0, 100]
})
const yScale = scaleLinear<number>({ domain: [0, 100], range: [100, 0] })

function renderInteraction() {
  return renderHook(() =>
    useChartInteraction({
      bisectDate,
      canInteract: true,
      data,
      lines,
      margin: { top: 0, right: 0, bottom: 0, left: 0 },
      xAccessor,
      xScale,
      yScale,
      yScales: { left: yScale }
    })
  )
}

function moveTo(result: ReturnType<typeof renderInteraction>['result'], x: number) {
  localPoint.mockReturnValue({ x, y: 0 })
  act(() => {
    result.current.interactionHandlers.onMouseMove?.({} as React.MouseEvent<SVGGElement>)
    vi.runAllTimers()
  })
}

describe('useChartInteraction', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    localPoint.mockReset()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('does not keep tooltip chrome active for a point without finite series values', () => {
    const { result } = renderInteraction()

    moveTo(result, 100)
    expect(result.current.tooltipData).toBeNull()

    moveTo(result, 0)
    expect(result.current.tooltipData?.point.cpu).toBe(42)

    moveTo(result, 100)
    expect(result.current.tooltipData).toBeNull()
  })
})
