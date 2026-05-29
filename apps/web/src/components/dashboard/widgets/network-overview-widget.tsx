import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkOverview } from '@/hooks/use-network-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { formatLatency, type NetworkServerSummary } from '@/lib/network-types'
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
      <div className="flex h-full flex-col gap-2 rounded-lg border bg-card p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (rows.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkOverview.title', 'Network Overview')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkOverview.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-2 font-semibold text-sm">{t('widgets.networkOverview.title', 'Network Overview')}</h3>
      <ScrollArea className="min-h-0 flex-1">
        <ul className="space-y-1 pr-2">
          {rows.map((summary) => {
            const latency = avgLatency(summary)
            return (
              <li key={summary.server_id}>
                <Link
                  className="flex items-center gap-3 rounded-md border px-3 py-2 transition-colors hover:bg-muted/50"
                  params={{ serverId: summary.server_id }}
                  search={{ range: 'realtime' }}
                  to="/network/$serverId"
                >
                  <span
                    aria-hidden="true"
                    className={`size-2 shrink-0 rounded-full ${summary.online ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`}
                  />
                  <span className="min-w-0 flex-1 truncate font-medium text-sm">{summary.server_name}</span>
                  <span className="font-mono text-sm tabular-nums">{formatLatency(latency)}</span>
                  {summary.anomaly_count > 0 && (
                    <span className="rounded-full bg-amber-100 px-2 py-0.5 text-amber-700 text-xs dark:bg-amber-900/30 dark:text-amber-400">
                      {summary.anomaly_count}
                    </span>
                  )}
                </Link>
              </li>
            )
          })}
        </ul>
      </ScrollArea>
    </div>
  )
}
