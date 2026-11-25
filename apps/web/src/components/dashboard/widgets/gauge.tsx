import { useId, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { splitValueUnit, WidgetMessage } from '@/components/dashboard/widget-chrome'
import type { ServerMetrics } from '@/lib/server-catalog'
import { getUtilizationRingColor, getUtilizationSeverity } from '@/lib/utilization-colors'
import { cn } from '@/lib/utils'
import { normalizeWidgetColor } from '@/lib/widget-color'
import { extractLiveMetric, metricLabel } from '@/lib/widget-helpers'
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
const STROKE = 6
// 270° sweep, gap centered at bottom. Angles in degrees, clockwise from 12 o'clock.
// Start at 7:30 (225°), sweep clockwise past 9, 12, 3 to 4:30 (135°), leaving a
// 90° gap centered at 6 o'clock — matches the reference horseshoe orientation.
const START_ANGLE = 225
const SWEEP = 270

interface Gradient {
  end: string
  start: string
}

/** Solid severity tone (same fill at both stops) so gauges match ring/bar language. */
function getGaugeGradient(value: number, overrideColor?: string): Gradient {
  const custom = normalizeWidgetColor(overrideColor)
  if (custom) {
    return { start: custom, end: custom }
  }
  const color = getUtilizationRingColor(value)
  return { start: color, end: color }
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
  const { t } = useTranslation('dashboard')
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
      <DashboardCard className="flex h-full bg-card">
        <WidgetMessage title={t('widgets.common.empty.serverNotFound')} />
      </DashboardCard>
    )
  }

  const label = config.label ?? metricLabel(metric, t)
  const gradient = getGaugeGradient(value, config.color)
  const percent = max > 0 ? (value / max) * 100 : 0
  const severity = getUtilizationSeverity(percent)
  const reading = splitValueUnit(`${value.toFixed(1)}%`)
  const [whole, fraction = ''] = reading.value.split('.')

  const progressSweep = max > 0 ? (value / max) * SWEEP : 0
  const trackEnd = START_ANGLE + SWEEP
  const progressEnd = START_ANGLE + progressSweep

  const trackPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, trackEnd)
  const progressPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, progressEnd)

  const showProgress = value > 0

  const svg = (
    <svg
      aria-hidden="true"
      className="absolute inset-0 h-full w-full overflow-visible"
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
        stroke="var(--metric-ring-track)"
        strokeLinecap="round"
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
      <DashboardCard
        className="@container/gauge relative aspect-square overflow-hidden bg-card"
        style={{ width: 'min(100cqi, 100cqb)', height: 'min(100cqi, 100cqb)' }}
      >
        <p
          className="absolute inset-x-4 top-3.5 z-10 truncate font-medium @[14rem]:text-[13px] text-[12px] text-foreground/75"
          data-testid="gauge-label"
        >
          {label}
        </p>
        <div className="absolute top-[17%] left-1/2 aspect-square w-[70%] -translate-x-1/2">
          {svg}
          <div className="absolute inset-0 flex items-center justify-center">
            <p
              className="font-semibold @[14rem]:text-4xl @[20rem]:text-5xl @[26rem]:text-6xl text-3xl tabular-nums leading-none tracking-[-0.04em]"
              data-testid="gauge-value"
            >
              {whole}
              <span className="font-medium text-[0.5em] text-muted-foreground tracking-[-0.01em]">
                {fraction ? `.${fraction}` : ''}
                {reading.unit}
              </span>
            </p>
          </div>
        </div>
        {/* Server name sits inside the arc's bottom gap; a word on the same line backs
            up the ring color, so a warning never pushes the name into the ring. */}
        <div className="absolute inset-x-3 bottom-[7%] z-10 flex items-baseline justify-center gap-1.5 text-center">
          <p
            className="min-w-0 truncate @[14rem]:text-[13px] text-muted-foreground text-xs"
            data-testid="gauge-subtitle"
          >
            {server.name}
          </p>
          {severity === 'healthy' ? null : (
            <p
              className={cn(
                'shrink-0 font-medium @[14rem]:text-[13px] text-xs',
                severity === 'danger' ? 'text-status-danger-text' : 'text-status-warning-text'
              )}
              data-testid="gauge-severity"
            >
              {t(`widgets.gauge.severity.${severity}`)}
            </p>
          )}
        </div>
      </DashboardCard>
    </div>
  )
}
