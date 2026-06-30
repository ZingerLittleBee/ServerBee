import type { UptimeColor } from '@/lib/widget-helpers'

export const SEGMENT_BACKGROUND_VALUE_MAP: Record<UptimeColor, string> = {
  green: 'var(--uptime-operational)',
  yellow: 'var(--uptime-degraded)',
  red: 'var(--uptime-down)',
  gray: 'var(--color-muted)'
}

const CSS_COORDINATE_PRECISION = 1000

interface TimelineGeometryInput {
  count: number
  gap: number
  width: number
}

export interface TimelineSegmentGeometry {
  width: number
  x: number
}

interface TimelineBackgroundInput {
  colors: UptimeColor[]
  geometry: TimelineSegmentGeometry[]
}

function roundCssCoordinate(value: number): number {
  return Math.round(value * CSS_COORDINATE_PRECISION) / CSS_COORDINATE_PRECISION
}

export function buildTimelineGeometry({ count, gap, width }: TimelineGeometryInput): TimelineSegmentGeometry[] {
  if (count <= 0 || width <= 0) {
    return []
  }

  const totalWidth = Math.max(0, Math.floor(width))
  const maxGap = count > 1 ? Math.max(0, (totalWidth - count) / (count - 1)) : 0
  const resolvedGap = count > 1 ? Math.min(Math.max(0, gap), maxGap) : 0
  const drawable = Math.max(0, totalWidth - resolvedGap * (count - 1))
  const segmentWidth = drawable / count

  let cursor = 0
  return Array.from({ length: count }, (_, index) => {
    const nextCursor = index === count - 1 ? totalWidth : cursor + segmentWidth
    const segment = {
      width: roundCssCoordinate(nextCursor - cursor),
      x: roundCssCoordinate(cursor)
    }
    cursor = nextCursor + resolvedGap
    return segment
  })
}

export function buildTimelineBackground({ colors, geometry }: TimelineBackgroundInput): string {
  const stops: string[] = []

  for (let index = 0; index < geometry.length; index += 1) {
    const segment = geometry[index]
    const color = colors[index]
    if (!(segment && color) || segment.width <= 0) {
      continue
    }

    if (stops.length > 0) {
      const previous = geometry[index - 1]
      if (previous) {
        const gapStart = previous.x + previous.width
        if (segment.x > gapStart) {
          stops.push(`transparent ${gapStart}px ${segment.x}px`)
        }
      }
    }

    stops.push(`${SEGMENT_BACKGROUND_VALUE_MAP[color]} ${segment.x}px ${segment.x + segment.width}px`)
  }

  return `linear-gradient(to right, ${stops.join(', ')})`
}
