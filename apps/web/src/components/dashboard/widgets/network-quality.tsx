import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage } from '@/components/dashboard/widget-chrome'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import { getLossTextClass } from '@/lib/network-latency-constants'
import { formatLatency, formatPacketLoss } from '@/lib/network-types'
import type { ServerMetrics } from '@/lib/server-catalog'
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
      <DashboardCard className="flex h-full flex-col gap-3 bg-card p-5">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </DashboardCard>
    )
  }

  const targets = summary?.targets ?? []
  const title = t('widgets.networkQuality.title', 'Network Quality')

  if (targets.length === 0) {
    return (
      <DashboardCard className="flex h-full flex-col bg-card p-5">
        <WidgetHeader title={title} />
        <WidgetMessage title={t('widgets.networkQuality.empty.noData', 'No network probe data available')} />
      </DashboardCard>
    )
  }

  return (
    <DashboardCard className="flex h-full flex-col bg-card p-5">
      <WidgetHeader
        aside={t('widgets.networkQuality.lossColumn', 'Loss')}
        context={summary?.server_name}
        title={title}
      />
      <ScrollArea className="mt-2 min-h-0 flex-1">
        <ul className="flex flex-col">
          {targets.map((target) => (
            <li className="flex items-center gap-3 border-border/60 border-t py-2.5" key={target.target_id}>
              <span className="min-w-0 flex-1 truncate font-medium text-sm">{target.target_name}</span>
              <span className="font-semibold text-sm tabular-nums">{formatLatency(target.avg_latency)}</span>
              <span className={`w-12 shrink-0 text-right text-xs tabular-nums ${getLossTextClass(target.packet_loss)}`}>
                {formatPacketLoss(target.packet_loss)}
              </span>
            </li>
          ))}
        </ul>
      </ScrollArea>
    </DashboardCard>
  )
}
