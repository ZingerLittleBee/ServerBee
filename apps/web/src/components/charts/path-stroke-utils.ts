import { type RefObject, useEffect, useState } from 'react'

export function findPathLengthAtX(path: SVGPathElement | null, pathLength: number, targetX: number): number {
  if (!path || pathLength === 0) {
    return 0
  }
  let low = 0
  let high = pathLength
  const tolerance = 0.5

  while (high - low > tolerance) {
    const mid = (low + high) / 2
    const point = path.getPointAtLength(mid)
    if (point.x < targetX) {
      low = mid
    } else {
      high = mid
    }
  }
  return (low + high) / 2
}

interface PathStrokeMetrics {
  pathD: string | null
  pathLength: number
}

const EMPTY_METRICS: PathStrokeMetrics = { pathD: null, pathLength: 0 }

export function usePathStrokeMetrics(pathRef: RefObject<SVGPathElement | null>): PathStrokeMetrics {
  const [metrics, setMetrics] = useState<PathStrokeMetrics>(EMPTY_METRICS)

  useEffect(() => {
    const path = pathRef.current
    if (!path) {
      return
    }
    const d = path.getAttribute('d')
    const len = d ? path.getTotalLength() : 0
    setMetrics((prev) => (prev.pathD === d && prev.pathLength === len ? prev : { pathD: d, pathLength: len }))
  })

  return metrics
}

export function resolveDashTailBounds(dashFromIndex: number | undefined, dataLength: number): boolean {
  return dashFromIndex != null && dashFromIndex >= 0 && dashFromIndex < dataLength - 1
}

export function resolveDashStartX(
  data: Record<string, unknown>[],
  dashFromIndex: number,
  xScale: (value: Date | number) => number | undefined,
  xAccessor: (datum: Record<string, unknown>) => Date | number
): number {
  const dashFromPoint = data[dashFromIndex]
  if (!dashFromPoint) {
    return 0
  }
  return xScale(xAccessor(dashFromPoint)) ?? 0
}
