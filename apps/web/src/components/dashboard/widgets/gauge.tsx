import { Activity, Cpu, Gauge as GaugeIcon, HardDrive, MemoryStick, Network } from 'lucide-react'
import { useId, useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { extractLiveMetric, METRIC_LABELS } from '@/lib/widget-helpers'
import type { GaugeConfig } from '@/lib/widget-types'

interface GaugeWidgetProps {
  config: GaugeConfig
  servers: ServerMetrics[]
}

// SVG geometry constants (viewBox is 100x100)
const VIEWBOX = 100
const CENTER = VIEWBOX / 2
// Radius pushes the ring close to the viewBox edge so the gauge fills the
// card; clamped so the round linecap (STROKE/2 outward) still fits inside.
const RADIUS = 44
const STROKE = 8
// 270° sweep, gap centered at bottom. Angles in degrees, clockwise from 12 o'clock.
// Start at 7:30 (225°), sweep clockwise past 9, 12, 3 to 4:30 (135°), leaving a
// 90° gap centered at 6 o'clock — matches the reference horseshoe orientation.
const START_ANGLE = 225
const SWEEP = 270

interface Gradient {
  end: string
  start: string
}

function getGaugeGradient(value: number): Gradient {
  if (value >= 90) {
    return { start: 'var(--chart-4)', end: 'var(--chart-3)' }
  }
  if (value >= 70) {
    return { start: 'var(--chart-3)', end: 'var(--chart-5)' }
  }
  return { start: 'var(--chart-1)', end: 'var(--chart-2)' }
}

function getMetricIcon(metric: string) {
  switch (metric) {
    case 'cpu':
      return Cpu
    case 'memory':
    case 'swap':
      return MemoryStick
    case 'disk':
      return HardDrive
    case 'load1':
    case 'load5':
    case 'load15':
      return Activity
    case 'net_in':
    case 'net_out':
    case 'bandwidth':
      return Network
    default:
      return GaugeIcon
  }
}

// Convert a polar coordinate (angle in degrees, clockwise from 12 o'clock) to cartesian SVG coords.
function polarToCartesian(cx: number, cy: number, r: number, angleDeg: number) {
  const angleRad = ((angleDeg - 90) * Math.PI) / 180
  return {
    x: cx + r * Math.cos(angleRad),
    y: cy + r * Math.sin(angleRad)
  }
}

// SVG arc path between two angles (clockwise from 12 o'clock). Sweeps the short or long way
// as needed so that any 0-360° span is drawable.
function arcPath(cx: number, cy: number, r: number, startAngle: number, endAngle: number): string {
  const start = polarToCartesian(cx, cy, r, startAngle)
  const end = polarToCartesian(cx, cy, r, endAngle)
  const span = (((endAngle - startAngle) % 360) + 360) % 360
  const largeArcFlag = span > 180 ? 1 : 0
  return `M ${start.x} ${start.y} A ${r} ${r} 0 ${largeArcFlag} 1 ${end.x} ${end.y}`
}

export function GaugeWidget({ config, servers }: GaugeWidgetProps) {
  const gradientId = useId()
  const server_id = config.server_id ?? ''
  const { metric } = config
  const max = config.max ?? 100

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const value = useMemo(() => {
    if (!server) {
      return 0
    }
    return Math.min(max, Math.max(0, extractLiveMetric(server, metric)))
  }, [server, metric, max])

  if (!server) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
        Server not found
      </div>
    )
  }

  const label = config.label ?? METRIC_LABELS[metric] ?? metric
  const gradient = getGaugeGradient(value)
  const Icon = getMetricIcon(metric)

  const progressSweep = max > 0 ? (value / max) * SWEEP : 0
  const trackEnd = START_ANGLE + SWEEP
  const progressEnd = START_ANGLE + progressSweep

  const trackPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, trackEnd)
  const progressPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, progressEnd)

  const showProgress = value > 0

  const mainText = (
    <>
      <Icon
        aria-hidden="true"
        className="@[14rem]:h-4 @[20rem]:h-5 h-3.5 @[14rem]:w-4 @[20rem]:w-5 w-3.5"
        style={{ color: gradient.start }}
      />
      <p
        className="mt-1 truncate font-medium @[14rem]:text-sm text-xs"
        data-testid="gauge-label"
        style={{ color: gradient.start }}
      >
        {label}
      </p>
      <p
        className="mt-1 font-bold @[14rem]:text-2xl @[20rem]:text-3xl @[26rem]:text-4xl text-foreground text-xl tabular-nums"
        data-testid="gauge-value"
      >
        {value.toFixed(1)}
        <span className="ml-0.5 font-medium text-[0.6em] text-muted-foreground/70">%</span>
      </p>
    </>
  )

  // Server name sits at the bottom of the card, inside the arc gap (the 90°
  // opening centered at 6 o'clock). Anchored via absolute positioning so it
  // doesn't shift the centered icon/label/value stack.
  const subtitle = (
    <p
      className="absolute inset-x-3 bottom-[8%] z-10 truncate text-center text-muted-foreground text-xs"
      data-testid="gauge-subtitle"
    >
      {server.name}
    </p>
  )

  const svg = (
    <svg
      aria-hidden="true"
      className="absolute inset-0 h-full w-full"
      data-testid="gauge-svg"
      viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
    >
      <defs>
        <linearGradient data-testid="gauge-gradient" id={gradientId} x1="0%" x2="100%" y1="0%" y2="100%">
          <stop offset="0%" stopColor={gradient.start} />
          <stop offset="100%" stopColor={gradient.end} />
        </linearGradient>
      </defs>
      <path
        d={trackPathD}
        data-testid="gauge-track"
        fill="none"
        stroke="var(--color-muted)"
        strokeLinecap="round"
        strokeOpacity={0.35}
        strokeWidth={STROKE}
      />
      {showProgress && (
        <path
          d={progressPathD}
          data-testid="gauge-progress"
          fill="none"
          stroke={`url(#${gradientId})`}
          strokeLinecap="round"
          strokeWidth={STROKE}
        />
      )}
    </svg>
  )

  // The grid cell is generally not square (12-col grid step ≠ row step), so we
  // size the visible card to min(cell_w, cell_h) via container-size queries on
  // the wrapper. The card itself is always aspect-square; the cell may have
  // empty space on one side that's transparent.
  return (
    <div className="grid h-full w-full place-items-center" style={{ containerType: 'size' }}>
      <div
        className="@container/gauge relative aspect-square overflow-hidden rounded-lg border bg-card"
        style={{ width: 'min(100cqi, 100cqb)', height: 'min(100cqi, 100cqb)' }}
      >
        <div className="absolute inset-3">{svg}</div>
        <div className="absolute inset-0 z-10 flex min-w-0 flex-col items-center justify-center p-3 text-center">
          {mainText}
        </div>
        {subtitle}
      </div>
    </div>
  )
}
