import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  type CombinedSeverity,
  getLatencyStatus,
  getLossSeverity,
  getSeverityMarkerColor,
  isLatencyFailure
} from '@/lib/network-latency-constants'
import { NetworkTargetBreakdown } from './network-target-breakdown'
import type { ServerCardMetricPoint } from './server-card-network-data'

// Uniform narrow markers in the style of status.openai.com fill the history row's height.
// Their shared silhouette keeps every severity at the same visual height; the 1px radius
// only softens the corners without turning the markers into pills.
const MARKER_WIDTH = 5
const MARKER_HEIGHT = 14
const MARKER_GAP = 4

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

// Severity is the single source of truth for each square: color, data-severity and the
// summary's severity counts all derive from it, so the grid can never disagree with itself.
function getPointSeverity(point: ServerCardMetricPoint, kind: 'latency' | 'loss'): CombinedSeverity {
  if (kind === 'latency') {
    return getLatencyStatus({ latencyMs: point.value, failed: isLatencyFailure(averageLossRatio(point)) })
  }
  return getLossSeverity(point.value)
}

function formatSummaryValue(point: ServerCardMetricPoint | undefined, kind: 'latency' | 'loss'): string {
  if (point?.value == null) {
    return '-'
  }
  // Loss point values are ratios (0..1), so the summary formats them as percentages.
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
  // With identical marker geometry, color is the only visual severity channel, so the
  // role="img" summary carries the per-severity breakdown for screen-reader users.
  const severityCounts: Record<Exclude<CombinedSeverity, 'healthy'>, number> = {
    warning: 0,
    severe: 0,
    failed: 0,
    unknown: 0
  }
  for (const point of points) {
    const severity = getPointSeverity(point, kind)
    if (severity !== 'healthy') {
      severityCounts[severity] += 1
    }
  }
  const summary = t(kind === 'latency' ? 'card_latency_history_summary' : 'card_loss_history_summary', {
    // The abnormal total is derived from the same counts (warning + severe + failed),
    // so the summary can never disagree with the breakdown.
    abnormal: severityCounts.warning + severityCounts.severe + severityCounts.failed,
    failed: severityCounts.failed,
    latest: formatSummaryValue(points.at(-1), kind),
    samples: points.length,
    severe: severityCounts.severe,
    unknown: severityCounts.unknown,
    warning: severityCounts.warning
  })

  // Every card renders ~30 markers per grid, so per-marker tab stops would drown the page's
  // Tab order. Instead the grid is a single labelled image: assistive tech gets the summary,
  // pointer users still get the per-marker tooltip. `role="img"` makes the markers
  // presentational, so they need no individual labels.
  return (
    <div
      aria-label={summary}
      className="flex h-3.5 w-full flex-row-reverse overflow-hidden"
      role="img"
      style={{ gap: `${MARKER_GAP}px` }}
    >
      {visible.map((point) => {
        const severity = getPointSeverity(point, kind)
        return (
          <Tooltip key={point.timestamp}>
            <TooltipTrigger
              render={
                <div
                  className="flex-none rounded-[1px]"
                  data-severity={severity}
                  data-testid="square"
                  style={{
                    backgroundColor: getSeverityMarkerColor(severity),
                    height: `${MARKER_HEIGHT}px`,
                    width: `${MARKER_WIDTH}px`
                  }}
                />
              }
            />
            <TooltipContent className="grid min-w-48 gap-1.5" sideOffset={4}>
              <PointTooltip point={point} t={t} />
            </TooltipContent>
          </Tooltip>
        )
      })}
    </div>
  )
}
