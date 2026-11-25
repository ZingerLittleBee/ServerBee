import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetHeader, WidgetMessage } from '@/components/dashboard/widget-chrome'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { formatLatency, type NetworkServerSummary } from '@/lib/network-types'
import type { ServerMetrics } from '@/lib/server-catalog'
import { cn } from '@/lib/utils'
import type { NetworkOverviewConfig } from '@/lib/widget-types'

interface NetworkOverviewWidgetProps {
  config: NetworkOverviewConfig
  servers: ServerMetrics[]
}

// Average latency across a server's targets, ignoring targets with no reading.
function avgLatency(summary: NetworkServerSummary): number | null {
  const values = summary.targets.map((target) => target.avg_latency).filter((v): v is number => v != null)
  if (values.length === 0) {
    return null
  }
  return values.reduce((a, b) => a + b, 0) / values.length
}

export function NetworkOverviewWidget({ config }: NetworkOverviewWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { data: overview = [], isLoading } = useNetworkOverview()

  const rows = useMemo(() => {
    const ids = config.server_ids
    if (!ids || ids.length === 0) {
      return overview
    }
    const allow = new Set(ids)
    return overview.filter((summary) => allow.has(summary.server_id))
  }, [overview, config.server_ids])

  if (isLoading) {
    return (
      <DashboardCard className="flex h-full flex-col gap-3 bg-card p-5">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </DashboardCard>
    )
  }

  const title = t('widgets.networkOverview.title', 'Network Overview')

  if (rows.length === 0) {
    return (
      <DashboardCard className="flex h-full flex-col bg-card p-5">
        <WidgetHeader title={title} />
        <WidgetMessage title={t('widgets.networkOverview.empty.noData', 'No network probe data available')} />
      </DashboardCard>
    )
  }

  return (
    <DashboardCard className="flex h-full flex-col bg-card p-5">
      <WidgetHeader title={title} />
      <ScrollArea className="mt-2 min-h-0 flex-1">
        <ul className="flex flex-col">
          {rows.map((summary) => {
            const latency = avgLatency(summary)
            return (
              <li className="border-border/60 border-t" key={summary.server_id}>
                <Link
                  className="-mx-2 flex items-center gap-3 rounded-lg px-2 py-2 transition-colors hover:bg-muted/50"
                  params={{ serverId: summary.server_id }}
                  search={{ range: 'realtime' }}
                  to="/network/$serverId"
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      'size-2 shrink-0 rounded-full',
                      summary.online ? 'bg-status-healthy' : 'bg-muted-foreground/40'
                    )}
                  />
                  <span className="min-w-0 flex-1 truncate font-medium text-sm">{summary.server_name}</span>
                  {summary.anomaly_count > 0 && (
                    <span className="rounded-full bg-status-warning/15 px-2 py-0.5 font-medium text-status-warning-text text-xs tabular-nums">
                      {t('widgets.networkOverview.anomalies', { count: summary.anomaly_count })}
                    </span>
                  )}
                  <span className="w-20 shrink-0 text-right font-semibold text-sm tabular-nums">
                    {formatLatency(latency)}
                  </span>
                </Link>
              </li>
            )
          })}
        </ul>
      </ScrollArea>
    </DashboardCard>
  )
}
