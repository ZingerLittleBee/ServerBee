import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { getLatencySquareColor, getLossSquareColor, LATENCY_UNKNOWN_BAR_COLOR } from '@/lib/network-latency-constants'
import { NetworkTargetBreakdown } from './network-target-breakdown'
import type { ServerCardMetricPoint } from './server-card-network-data'

const SQUARE_SIZE = 6
const SQUARE_GAP = 2

interface NetworkSquareGridProps {
  kind: 'latency' | 'loss'
  points: readonly ServerCardMetricPoint[]
}

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }
  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
}

function getSquareColor(point: ServerCardMetricPoint, kind: 'latency' | 'loss'): string {
  if (point.value == null) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (kind === 'latency') {
    return getLatencySquareColor({ latencyMs: point.value, lossRatio: averageLossRatio(point) })
  }
  return getLossSquareColor(point.value)
}

// The healthy square color, resolved through the public color helper so the summary label
// can never drift from the thresholds the squares are actually painted with.
const HEALTHY_SQUARE_COLOR = getLossSquareColor(0)

function isAbnormalSquare(point: ServerCardMetricPoint, kind: 'latency' | 'loss'): boolean {
  const color = getSquareColor(point, kind)
  return color !== HEALTHY_SQUARE_COLOR && color !== LATENCY_UNKNOWN_BAR_COLOR
}

function formatSummaryValue(point: ServerCardMetricPoint | undefined, kind: 'latency' | 'loss'): string {
  if (point?.value == null) {
    return '-'
  }
  return kind === 'latency' ? `${point.value.toFixed(0)}ms` : `${(point.value * 100).toFixed(1)}%`
}

function formatTooltipLabel(point: ServerCardMetricPoint, t: (key: string) => string): string {
  const parsed = Date.parse(point.timestamp)
  if (Number.isNaN(parsed)) {
    return t('current_targets')
  }
  return new Date(parsed).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function PointTooltip({ point, t }: { point: ServerCardMetricPoint; t: (key: string) => string }) {
  if (point.targets.length === 0) {
    return null
  }
  return (
    <>
      <div className="font-medium">{formatTooltipLabel(point, t)}</div>
      <NetworkTargetBreakdown targets={point.targets} />
    </>
  )
}

export function NetworkSquareGrid({ points, kind }: NetworkSquareGridProps) {
  const { t } = useTranslation(['servers'])
  const visible = points.toReversed()
  const summary = t(kind === 'latency' ? 'card_latency_history_summary' : 'card_loss_history_summary', {
    abnormal: points.filter((point) => isAbnormalSquare(point, kind)).length,
    latest: formatSummaryValue(points.at(-1), kind),
    samples: points.length
  })

  // Every card renders ~30 squares per grid, so per-square tab stops would drown the page's
  // Tab order. Instead the grid is a single labelled image: assistive tech gets the summary,
  // pointer users still get the per-square tooltip. `role="img"` makes the squares
  // presentational, so they need no individual labels.
  return (
    <div
      aria-label={summary}
      className="flex h-3 w-full flex-row-reverse items-end overflow-hidden"
      role="img"
      style={{ gap: `${SQUARE_GAP}px` }}
    >
      {visible.map((point) => (
        <Tooltip key={point.timestamp}>
          <TooltipTrigger
            render={
              <div
                className="flex-none rounded-[1px]"
                data-testid="square"
                style={{
                  backgroundColor: getSquareColor(point, kind),
                  height: `${SQUARE_SIZE}px`,
                  width: `${SQUARE_SIZE}px`
                }}
              />
            }
          />
          <TooltipContent className="grid min-w-48 gap-1.5" sideOffset={4}>
            <PointTooltip point={point} t={t} />
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  )
}
