import { TrendingDown, TrendingUp } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { DeltaTone, DeltaUnit } from './metric-card-config'

interface MetricCardValueProps {
  delta: number | null
  deltaTone: DeltaTone
  deltaUnit: DeltaUnit
  formattedValue: string
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

function deltaColor(delta: number, tone: DeltaTone): string {
  if (tone === 'neutral') {
    return 'text-muted-foreground'
  }
  if (delta === 0) {
    return 'text-muted-foreground'
  }
  return delta > 0 ? 'text-destructive' : 'text-emerald-500'
}

export function MetricCardValue({ formattedValue, delta, deltaUnit, deltaTone, pastLabel }: MetricCardValueProps) {
  return (
    <div className="space-y-0.5">
      <p
        className="truncate font-bold text-3xl tabular-nums leading-tight tracking-tight"
        data-testid="metric-card-value"
      >
        {formattedValue}
      </p>
      <p
        className={cn(
          'flex items-center gap-1 text-xs',
          delta === null ? 'text-muted-foreground' : deltaColor(delta, deltaTone)
        )}
        data-testid="metric-card-delta"
      >
        {delta === null ? (
          <span>—</span>
        ) : (
          <>
            {delta < 0 ? <TrendingDown className="size-3" /> : <TrendingUp className="size-3" />}
            <span className="font-medium tabular-nums">{formatDelta(delta, deltaUnit)}</span>
          </>
        )}
        <span className="text-muted-foreground">· {pastLabel}</span>
      </p>
    </div>
  )
}
