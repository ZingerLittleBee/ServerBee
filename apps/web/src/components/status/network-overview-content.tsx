import { Link } from '@tanstack/react-router'
import { AlertTriangle, Server, Wifi } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'

// Both the admin (`NetworkServerSummary`) and public
// (`PublicNetworkServerOverview`) shapes share the same field set used by this
// component, so we only type the structural subset here and accept both.
export interface NetworkOverviewSummary {
  anomaly_count: number
  last_probe_at: string | null
  online: boolean
  server_id: string
  server_name: string
  targets: {
    avg_latency: number | null
    packet_loss: number
    target_id: string
    target_name: string
  }[]
}

export interface NetworkOverviewContentProps {
  data: NetworkOverviewSummary[]
  isLoading?: boolean
  onSearchChange: (q: string) => void
  search: string
  variant: 'admin' | 'public'
}

function formatLatencyDisplay(ms: number | null | undefined): string {
  if (ms == null) {
    return 'N/A'
  }
  return `${ms.toFixed(1)} ms`
}

function formatAvailability(targets: NetworkOverviewSummary['targets']): string {
  if (targets.length === 0) {
    return 'N/A'
  }
  const avgLoss = targets.reduce((sum, t) => sum + t.packet_loss, 0) / targets.length
  return `${((1 - avgLoss) * 100).toFixed(1)}%`
}

function avgLatencyFromTargets(targets: NetworkOverviewSummary['targets']): number | null {
  const valid = targets.filter((t) => t.avg_latency != null)
  if (valid.length === 0) {
    return null
  }
  return valid.reduce((sum, t) => sum + (t.avg_latency ?? 0), 0) / valid.length
}

function worstTarget(targets: NetworkOverviewSummary['targets']): NetworkOverviewSummary['targets'][number] | null {
  const valid = targets.filter((t) => t.avg_latency != null)
  if (valid.length === 0) {
    return null
  }
  return valid.reduce((worst, t) => ((t.avg_latency ?? 0) > (worst.avg_latency ?? 0) ? t : worst))
}

function StatCard({
  icon: Icon,
  label,
  value,
  tone = 'default'
}: {
  icon: typeof Server
  label: string
  value: string | number
  tone?: 'default' | 'warning'
}) {
  const isWarning = tone === 'warning'
  return (
    <div
      className={cn(
        'flex items-center gap-3 rounded-lg border bg-card p-4',
        isWarning && 'border-amber-300/60 dark:border-amber-700/50'
      )}
    >
      <div className={cn('rounded-md p-2', isWarning ? 'bg-amber-100 dark:bg-amber-900/30' : 'bg-muted')}>
        <Icon aria-hidden="true" className={cn('size-5', isWarning ? 'text-amber-500' : 'text-muted-foreground')} />
      </div>
      <div>
        <p className="font-semibold text-lg leading-tight">{value}</p>
        <p className="text-muted-foreground text-xs">{label}</p>
      </div>
    </div>
  )
}

function ServerNetworkCard({ summary, variant }: { summary: NetworkOverviewSummary; variant: 'admin' | 'public' }) {
  const { t } = useTranslation('network')
  const avgLatency = avgLatencyFromTargets(summary.targets)
  const worst = worstTarget(summary.targets)
  const availability = formatAvailability(summary.targets)

  const body = (
    <Card
      className={cn('h-full cursor-pointer transition-colors hover:border-primary/50', !summary.online && 'opacity-70')}
    >
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between gap-2">
          <CardTitle className="truncate text-base">{summary.server_name}</CardTitle>
          <Badge
            className={cn(
              'shrink-0 gap-1 text-xs',
              summary.online ? 'bg-emerald-500/15 text-emerald-600' : 'bg-red-500/15 text-red-600'
            )}
            variant="outline"
          >
            <span className={cn('size-1.5 rounded-full', summary.online ? 'bg-emerald-500' : 'bg-red-500')} />
            {summary.online ? t('online') : t('offline')}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="grid grid-cols-3 gap-2 text-center">
          <div className="rounded-md bg-muted/50 p-2">
            <p className="font-mono font-semibold text-sm">{formatLatencyDisplay(avgLatency)}</p>
            <p className="text-muted-foreground text-xs">{t('avg_latency')}</p>
          </div>
          <div className="rounded-md bg-muted/50 p-2">
            <p className="font-semibold text-sm">{availability}</p>
            <p className="text-muted-foreground text-xs">{t('availability')}</p>
          </div>
          <div className="rounded-md bg-muted/50 p-2">
            <p className="font-semibold text-sm">{summary.targets.length}</p>
            <p className="text-muted-foreground text-xs">{t('targets')}</p>
          </div>
        </div>

        {worst != null && (
          <div className="flex items-center gap-1.5 rounded-md border border-amber-200/60 bg-amber-50/50 px-2.5 py-1.5 dark:border-amber-800/40 dark:bg-amber-900/20">
            <AlertTriangle aria-hidden="true" className="size-3 shrink-0 text-amber-500" />
            <p className="truncate text-amber-700 text-xs dark:text-amber-400">
              <span className="font-medium">{t('worst_line')}:</span> {worst.target_name}{' '}
              <span className="font-mono">{formatLatencyDisplay(worst.avg_latency)}</span>
            </p>
          </div>
        )}

        {summary.anomaly_count > 0 && (
          <div className="flex items-center gap-1.5">
            <span className="size-1.5 rounded-full bg-red-500" />
            <p className="text-muted-foreground text-xs">
              {summary.anomaly_count} {summary.anomaly_count === 1 ? 'anomaly' : 'anomalies'}
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  )

  if (variant === 'public') {
    return (
      <Link params={{ serverId: summary.server_id }} to="/status/network/$serverId">
        {body}
      </Link>
    )
  }

  return (
    <Link params={{ serverId: summary.server_id }} search={{ range: '1' }} to="/network/$serverId">
      {body}
    </Link>
  )
}

export function NetworkOverviewContent({
  data,
  isLoading,
  onSearchChange,
  search,
  variant
}: NetworkOverviewContentProps) {
  const { t } = useTranslation('network')

  const totalServers = data.length
  const onlineServers = data.filter((s) => s.online).length
  const anomalyServers = data.filter((s) => s.anomaly_count > 0).length

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim()
    if (!q) {
      return data
    }
    return data.filter((s) => s.server_name.toLowerCase().includes(q))
  }, [data, search])

  return (
    <div>
      <div className="mb-6">
        <h1 className="font-bold text-2xl">{t('overview_title')}</h1>
        <p className="text-muted-foreground text-sm">
          {onlineServers} / {totalServers} {t('online_servers').toLowerCase()}
        </p>
      </div>

      {totalServers > 0 && (
        <div className="mb-6 grid gap-4 sm:grid-cols-3">
          <StatCard icon={Server} label={t('total_servers')} value={totalServers} />
          <StatCard icon={Wifi} label={t('online_servers')} value={onlineServers} />
          <StatCard
            icon={AlertTriangle}
            label={t('anomaly_count')}
            tone={anomalyServers > 0 ? 'warning' : 'default'}
            value={anomalyServers}
          />
        </div>
      )}

      <div className="mb-4">
        <div className="relative">
          <svg
            aria-hidden="true"
            className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
            fill="none"
            focusable="false"
            stroke="currentColor"
            strokeWidth={2}
            viewBox="0 0 24 24"
          >
            <circle cx={11} cy={11} r={8} />
            <path d="m21 21-4.35-4.35" />
          </svg>
          <Input
            aria-label={t('servers:search_placeholder')}
            autoComplete="off"
            className="pl-9"
            name="search"
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t('servers:search_placeholder')}
            type="text"
            value={search}
          />
        </div>
      </div>

      {isLoading && (
        <div className="flex min-h-[300px] items-center justify-center">
          <div className="size-8 animate-spin rounded-full border-4 border-muted border-t-primary" />
        </div>
      )}

      {!isLoading && totalServers === 0 && (
        <div className="flex min-h-[300px] items-center justify-center rounded-lg border border-dashed">
          <div className="text-center">
            <p className="text-muted-foreground text-sm">{t('no_data')}</p>
          </div>
        </div>
      )}

      {!isLoading && totalServers > 0 && filtered.length === 0 && (
        <div className="flex min-h-[200px] items-center justify-center rounded-lg border border-dashed">
          <p className="text-muted-foreground text-sm">{t('no_search_results')}</p>
        </div>
      )}

      {!isLoading && filtered.length > 0 && (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          {filtered.map((summary) => (
            <ServerNetworkCard key={summary.server_id} summary={summary} variant={variant} />
          ))}
        </div>
      )}
    </div>
  )
}
