import { useTranslation } from 'react-i18next'
import { isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import { AGGREGATE_TARGET_ID, type ServerCardTooltipTarget } from './server-card-network-data'

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '-'
  }
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '-'
  }
  return `${(lossRatio * 100).toFixed(1)}%`
}

function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < 0.01) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (lossRatio < 0.05) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

export function NetworkTargetBreakdown({ targets }: { targets: readonly ServerCardTooltipTarget[] }) {
  const { t } = useTranslation(['servers'])
  if (targets.length === 0) {
    return null
  }
  return (
    <div className="grid gap-1.5">
      {targets.map((target) => {
        const failed = isLatencyFailure(target.lossRatio)
        return (
          <div className="flex items-center justify-between gap-3" key={target.targetId}>
            <span className="truncate text-muted-foreground">
              {target.targetId === AGGREGATE_TARGET_ID ? t('card_network_avg') : target.targetName}
            </span>
            <div className="flex gap-2 font-medium font-mono tabular-nums">
              <span className={latencyColorClass(target.latency, { failed })}>{formatLatency(target.latency)}</span>
              <span className={getLossTextClassName(target.lossRatio)}>{formatPacketLoss(target.lossRatio)}</span>
            </div>
          </div>
        )
      })}
    </div>
  )
}
