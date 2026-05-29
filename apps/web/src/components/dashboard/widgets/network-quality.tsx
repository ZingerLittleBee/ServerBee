import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CHART_COLORS } from '@/lib/chart-colors'
import { formatLatency, formatPacketLoss, getLossTextClassName } from '@/lib/network-types'
import type { NetworkQualityConfig } from '@/lib/widget-types'

interface NetworkQualityWidgetProps {
  config: NetworkQualityConfig
  servers: ServerMetrics[]
}

export function NetworkQualityWidget({ config }: NetworkQualityWidgetProps) {
  const { t } = useTranslation('dashboard')
  const serverId = config.server_id ?? ''
  const { data: summary, isLoading } = useNetworkServerSummary(serverId)

  if (isLoading) {
    return (
      <div className="flex h-full flex-col gap-2 rounded-lg border bg-card p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  const targets = summary?.targets ?? []

  if (targets.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkQuality.title', 'Network Quality')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkQuality.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-2">
        <h3 className="font-semibold text-sm">{t('widgets.networkQuality.title', 'Network Quality')}</h3>
        <p className="text-muted-foreground text-xs">{summary?.server_name}</p>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <ul className="space-y-1.5 pr-2">
          {targets.map((target, i) => (
            <li className="flex items-center gap-3 rounded-md border px-3 py-2" key={target.target_id}>
              <span
                aria-hidden="true"
                className="size-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: CHART_COLORS[i % CHART_COLORS.length] }}
              />
              <span className="min-w-0 flex-1 truncate font-medium text-sm">{target.target_name}</span>
              <span className="font-mono text-sm tabular-nums">{formatLatency(target.avg_latency)}</span>
              <span className={`font-mono text-xs tabular-nums ${getLossTextClassName(target.packet_loss)}`}>
                {formatPacketLoss(target.packet_loss)}
              </span>
            </li>
          ))}
        </ul>
      </ScrollArea>
    </div>
  )
}
