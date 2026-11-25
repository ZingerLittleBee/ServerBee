import { cn } from '@/lib/utils'
import type { DeltaTone, DeltaUnit } from './metric-card-config'

interface MetricCardDeltaProps {
  delta: number | null
  deltaTone: DeltaTone
  deltaUnit: DeltaUnit
  pastLabel: string
}

function formatDelta(delta: number, unit: DeltaUnit): string {
  const sign = delta >= 0 ? '+' : '−'
  const magnitude = Math.abs(delta)
  if (unit === 'pp') {
    return `${sign}${magnitude.toFixed(1)}pp`
  }
  return `${sign}${magnitude.toFixed(0)}%`
}

/** Rising utilization reads as a warning; falling reads as relief. Throughput stays neutral. */
function deltaPillClass(delta: number, tone: DeltaTone): string {
  if (tone === 'neutral' || delta === 0) {
    return 'bg-muted text-muted-foreground'
  }
  return delta > 0 ? 'bg-status-warning/15 text-status-warning-text' : 'bg-status-healthy/15 text-status-healthy-text'
}

export function MetricCardDelta({ delta, deltaUnit, deltaTone, pastLabel }: MetricCardDeltaProps) {
  return (
    <p className="flex min-w-0 items-center gap-2 text-muted-foreground text-xs" data-testid="metric-card-delta">
      {delta === null ? (
        <span>—</span>
      ) : (
        <span className={cn('rounded-full px-2 py-0.5 font-medium tabular-nums', deltaPillClass(delta, deltaTone))}>
          {delta > 0 ? '▲ ' : ''}
          {delta < 0 ? '▼ ' : ''}
          {formatDelta(delta, deltaUnit)}
        </span>
      )}
      <span className="truncate">{pastLabel}</span>
    </p>
  )
}
