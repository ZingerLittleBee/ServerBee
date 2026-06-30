import { Tooltip as TooltipPrimitive } from '@base-ui/react/tooltip'
import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeUptimeColor, formatUptimeTooltip, type UptimeColor } from '@/lib/widget-helpers'

export interface UptimeTimelineProps {
  days: UptimeDailyEntry[]
  height?: number
  rangeDays: number
  redThreshold?: number
  showLabels?: boolean
  showLegend?: boolean
  yellowThreshold?: number
}

const SEGMENT_BACKGROUND_VALUE_MAP: Record<UptimeColor, string> = {
  green: 'var(--uptime-operational)',
  yellow: 'var(--uptime-degraded)',
  red: 'var(--uptime-down)',
  gray: 'var(--color-muted)'
}

const SEGMENT_GAP = 1.5
const FALLBACK_SEGMENT_WIDTH = 4
const CSS_COORDINATE_PRECISION = 1000

// Promotes the painted track to its own compositor layer so the browser snaps
// the gradient to whole device pixels. Without this, when the track lands on
// a fractional CSS y position (eg. after a scroll), the AA at the top/bottom
// rows blends each color with the page background. In dark mode the resulting
// blend luminance differs per color (emerald vs amber vs red), which reads as
// "different colors have different heights".
const PIXEL_SNAP_TRANSFORM = 'translateZ(0)'

const POPUP_CLASS =
  'data-[side=bottom]:slide-in-from-top-2 data-[side=top]:slide-in-from-bottom-2 data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:fade-in-0 data-open:zoom-in-95 data-closed:fade-out-0 data-closed:zoom-out-95 z-50 inline-flex w-fit max-w-xs origin-(--transform-origin) flex-col rounded-md border bg-popover px-3 py-1.5 text-popover-foreground text-xs shadow-md data-[state=delayed-open]:animate-in data-closed:animate-out data-open:animate-in'

interface TimelineGeometryInput {
  count: number
  gap: number
  width: number
}

interface TimelineSegmentGeometry {
  width: number
  x: number
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

interface TimelineBackgroundInput {
  colors: UptimeColor[]
  geometry: TimelineSegmentGeometry[]
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

export function UptimeTimeline({
  days,
  rangeDays,
  yellowThreshold = 100,
  redThreshold = 95,
  showLabels = false,
  showLegend = false,
  height = 28
}: UptimeTimelineProps) {
  const { t } = useTranslation('status')
  const timelineRef = useRef<HTMLDivElement>(null)
  const [timelineWidth, setTimelineWidth] = useState(0)

  // One handle per timeline instance — lets the 90 detached triggers share a
  // single tooltip popup instead of each spawning its own Root/Portal/Popup.
  const [handle] = useState(() => TooltipPrimitive.createHandle<UptimeDailyEntry>())

  useLayoutEffect(() => {
    const element = timelineRef.current
    if (!element) {
      return undefined
    }

    const measure = () => {
      setTimelineWidth(Math.max(0, Math.floor(element.getBoundingClientRect().width)))
    }

    measure()

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure)
      return () => window.removeEventListener('resize', measure)
    }

    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [])

  const segments = useMemo(() => {
    const slice = days.slice(-rangeDays)
    const padCount = rangeDays - slice.length
    const padded: UptimeDailyEntry[] = Array.from({ length: padCount }, () => ({
      date: '',
      online_minutes: 0,
      total_minutes: 0,
      downtime_incidents: 0
    }))
    return [...padded, ...slice]
  }, [days, rangeDays])

  const drawableWidth =
    timelineWidth || segments.length * FALLBACK_SEGMENT_WIDTH + Math.max(0, segments.length - 1) * SEGMENT_GAP
  const geometry = useMemo(
    () => buildTimelineGeometry({ count: segments.length, gap: SEGMENT_GAP, width: drawableWidth }),
    [segments.length, drawableWidth]
  )
  const segmentColors = useMemo(
    () =>
      segments.map((entry) =>
        computeUptimeColor(entry.online_minutes, entry.total_minutes, yellowThreshold, redThreshold)
      ),
    [segments, yellowThreshold, redThreshold]
  )
  const trackBackground = useMemo(
    () => buildTimelineBackground({ colors: segmentColors, geometry }),
    [geometry, segmentColors]
  )
  const timelineTitle = `${t('uptime_days_ago', { count: rangeDays })} - ${t('uptime_today')}`

  return (
    <div className="w-full">
      {showLabels && (
        <div className="mb-1 flex justify-between text-muted-foreground text-xs">
          <span>{t('uptime_days_ago', { count: rangeDays })}</span>
          <span>{t('uptime_today')}</span>
        </div>
      )}

      <TooltipPrimitive.Root handle={handle}>
        {({ payload: entry }) => {
          const tooltip = entry ? formatUptimeTooltip(entry) : null
          return (
            <TooltipPrimitive.Portal>
              <TooltipPrimitive.Positioner align="center" className="isolate z-50" side="top" sideOffset={4}>
                <TooltipPrimitive.Popup className={POPUP_CLASS}>
                  <p className="font-medium">{tooltip?.date || t('uptime_no_data')}</p>
                  {tooltip && (
                    <>
                      <p className="text-muted-foreground">
                        {tooltip.percentage} &middot; {tooltip.duration}
                      </p>
                      <p className="text-muted-foreground">{tooltip.incidents}</p>
                    </>
                  )}
                </TooltipPrimitive.Popup>
              </TooltipPrimitive.Positioner>
            </TooltipPrimitive.Portal>
          )
        }}
      </TooltipPrimitive.Root>

      <figure
        aria-label={timelineTitle}
        className="w-full"
        data-uptime-timeline=""
        ref={timelineRef}
        style={{ height, margin: 0 }}
      >
        <div className="relative h-full w-full">
          <div
            aria-hidden
            className="absolute inset-0 overflow-hidden rounded-[4px]"
            data-uptime-track-paint=""
            style={{ backgroundImage: trackBackground, transform: PIXEL_SNAP_TRANSFORM }}
          />
          {segments.map((entry, i) => {
            const color = segmentColors[i]
            const segment = geometry[i]
            if (!(color && segment) || segment.width <= 0) {
              return null
            }
            return (
              <TooltipPrimitive.Trigger
                data-segment={color}
                handle={handle}
                key={`segment-${entry.date || `pad-${i.toString()}`}`}
                payload={entry}
                render={
                  <div
                    className="absolute top-0 h-full rounded-none focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
                    style={{ left: segment.x, width: segment.width }}
                  />
                }
              />
            )
          })}
        </div>
      </figure>

      {showLegend && (
        <div className="mt-2 flex gap-4 text-muted-foreground text-xs">
          <span className="flex items-center gap-1">
            <span
              className="inline-block size-2.5 rounded-[2px]"
              style={{ backgroundColor: SEGMENT_BACKGROUND_VALUE_MAP.green }}
            />
            {t('uptime_operational')}
          </span>
          <span className="flex items-center gap-1">
            <span
              className="inline-block size-2.5 rounded-[2px]"
              style={{ backgroundColor: SEGMENT_BACKGROUND_VALUE_MAP.yellow }}
            />
            {t('uptime_degraded')}
          </span>
          <span className="flex items-center gap-1">
            <span
              className="inline-block size-2.5 rounded-[2px]"
              style={{ backgroundColor: SEGMENT_BACKGROUND_VALUE_MAP.red }}
            />
            {t('uptime_down')}
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2.5 rounded-[2px] bg-muted" />
            {t('uptime_no_data')}
          </span>
        </div>
      )}
    </div>
  )
}
