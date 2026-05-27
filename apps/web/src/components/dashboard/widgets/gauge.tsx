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
const RADIUS = 38
const STROKE = 8
const BALL_R = 5.5
const BALL_R_INNER = 2
// 270° sweep, gap centered at top. Angles in degrees, clockwise from 12 o'clock.
const START_ANGLE = 135
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
  const startCap = polarToCartesian(CENTER, CENTER, RADIUS, START_ANGLE)
  const endCap = polarToCartesian(CENTER, CENTER, RADIUS, progressEnd)

  const showProgress = value > 0

  return (
    <div className="@container/gauge relative flex h-full flex-col items-center justify-center overflow-hidden rounded-lg border bg-card p-3">
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
        {showProgress && (
          <g data-testid="gauge-endcaps">
            <circle cx={startCap.x} cy={startCap.y} fill="white" r={BALL_R} />
            <circle cx={startCap.x} cy={startCap.y} fill={gradient.start} r={BALL_R_INNER} />
            <circle cx={endCap.x} cy={endCap.y} fill="white" r={BALL_R} />
            <circle cx={endCap.x} cy={endCap.y} fill={gradient.end} r={BALL_R_INNER} />
          </g>
        )}
      </svg>

      <div className="relative z-10 flex flex-col items-center text-center">
        <Icon
          aria-hidden="true"
          className="@[10rem]:h-4 @[14rem]:h-5 h-3.5 @[10rem]:w-4 @[14rem]:w-5 w-3.5"
          style={{ color: gradient.start }}
        />
        <p
          className="mt-1 truncate font-medium @[10rem]:text-sm text-xs"
          data-testid="gauge-label"
          style={{ color: gradient.start }}
        >
          {label}
        </p>
        <p
          className="mt-1 font-bold @[10rem]:text-3xl @[14rem]:text-4xl text-2xl text-foreground tabular-nums"
          data-testid="gauge-value"
        >
          {value.toFixed(1)}%
        </p>
        <p
          className="mt-1 @[8rem]:block hidden max-w-[80%] truncate text-muted-foreground text-xs"
          data-testid="gauge-subtitle"
        >
          {server.name}
        </p>
      </div>
    </div>
  )
}
