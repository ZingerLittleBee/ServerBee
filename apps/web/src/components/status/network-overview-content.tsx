import { Link } from '@tanstack/react-router'
import { AlertTriangle, Server, Wifi } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Input } from '@/components/ui/input'
import { getCombinedSeverity } from '@/lib/network-latency-constants'
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

// 24h is the window the server uses to compute anomaly_count (see
// network_probe.rs `count_anomalies`); surfaced here for the card footnote.
const ANOMALY_WINDOW_HOURS = 24

type Health = 'healthy' | 'warning' | 'severe' | 'unknown' | 'offline'

function avgLatencyFromTargets(targets: NetworkOverviewSummary['targets']): number | null {
  const valid = targets.filter((t) => t.avg_latency != null)
  if (valid.length === 0) {
    return null
  }
  return valid.reduce((sum, t) => sum + (t.avg_latency ?? 0), 0) / valid.length
}

function avgLossFromTargets(targets: NetworkOverviewSummary['targets']): number | null {
  if (targets.length === 0) {
    return null
  }
  return targets.reduce((sum, t) => sum + t.packet_loss, 0) / targets.length
}

function worstTarget(targets: NetworkOverviewSummary['targets']): NetworkOverviewSummary['targets'][number] | null {
  const valid = targets.filter((t) => t.avg_latency != null)
  if (valid.length === 0) {
    return null
  }
  return valid.reduce((worst, t) => ((t.avg_latency ?? 0) > (worst.avg_latency ?? 0) ? t : worst))
}

// Whole-server health verdict from avg latency + avg loss, reusing the shared
// severity thresholds so it stays consistent with the rest of the app.
export function serverHealth(summary: NetworkOverviewSummary): Health {
  if (!summary.online) {
    return 'offline'
  }
  const latency = avgLatencyFromTargets(summary.targets)
  // Online but no latency reading yet (empty targets or all probing) — keep the
  // verdict consistent with the card's `hasData` check, which also keys on latency.
  if (latency == null) {
    return 'unknown'
  }
  const sev = getCombinedSeverity({ latencyMs: latency, lossRatio: avgLossFromTargets(summary.targets) })
  if (sev === 'failed' || sev === 'severe') {
    return 'severe'
  }
  if (sev === 'warning') {
    return 'warning'
  }
  return 'healthy'
}

const HEALTH_DOT: Record<Health, string> = {
  healthy: 'bg-emerald-500',
  warning: 'bg-amber-500',
  severe: 'bg-red-500',
  unknown: 'bg-muted-foreground',
  offline: 'bg-muted-foreground/50'
}

const HEALTH_ACCENT: Record<Health, string> = {
  healthy: 'text-emerald-600 dark:text-emerald-400',
  warning: 'text-amber-600 dark:text-amber-400',
  severe: 'text-red-600 dark:text-red-400',
  unknown: 'text-muted-foreground',
  offline: 'text-muted-foreground'
}

function healthLabel(health: Health, t: (key: string) => string): string {
  switch (health) {
    case 'healthy':
      return t('health_healthy')
    case 'warning':
      return t('health_warning')
    case 'severe':
      return t('health_severe')
    case 'offline':
      return t('offline')
    default:
      return t('no_data')
  }
}

function formatLatencyInt(ms: number | null): string {
  return ms == null ? '—' : `${Math.round(ms)}`
}

function StatusPill({ health, label }: { health: Health; label: string }) {
  const muted = health === 'offline' || health === 'unknown'
  return (
    <span
      className={cn(
        'inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-0.5 font-medium text-xs',
        HEALTH_ACCENT[health],
        muted ? 'border-border' : 'border-current/25'
      )}
    >
      <span className={cn('size-1.5 rounded-full', HEALTH_DOT[health])} />
      {label}
    </span>
  )
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
  const health = serverHealth(summary)
  const latency = avgLatencyFromTargets(summary.targets)
  const loss = avgLossFromTargets(summary.targets)
  const worst = worstTarget(summary.targets)
  const hasData = summary.online && latency != null
  const isProblem = health === 'warning' || health === 'severe'

  const body = (
    <div
      className={cn(
        'flex h-full flex-col rounded-xl border bg-card p-5 transition-all',
        'hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-black/5 hover:shadow-lg',
        !summary.online && 'opacity-60'
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate font-medium text-sm tracking-tight">{summary.server_name}</span>
        <StatusPill health={health} label={healthLabel(health, t)} />
      </div>

      <div className="mt-5 flex items-end gap-1.5">
        <span
          className={cn('font-semibold text-4xl tabular-nums leading-none tracking-tighter', HEALTH_ACCENT[health])}
        >
          {formatLatencyInt(latency)}
        </span>
        {hasData && <span className="pb-0.5 text-muted-foreground text-sm">ms</span>}
        <span className="pb-0.5 text-muted-foreground text-xs">{t('latency')}</span>
      </div>

      <div className="mt-auto pt-4">
        {hasData ? (
          <div className="flex items-center gap-3 border-t pt-3 text-xs">
            <span className="text-muted-foreground">
              {t('loss')}{' '}
              <span className="font-medium text-foreground tabular-nums">
                {loss == null ? '—' : `${(loss * 100).toFixed(1)}%`}
              </span>
            </span>
            <span className="text-border">·</span>
            <span className="text-muted-foreground">
              <span className="font-medium text-foreground tabular-nums">{summary.targets.length}</span> {t('lines')}
            </span>
            {isProblem && worst != null && (
              <span className={cn('ml-auto truncate', HEALTH_ACCENT[health])}>
                {worst.target_name} {formatLatencyInt(worst.avg_latency)}ms
              </span>
            )}
          </div>
        ) : (
          <div className="border-t pt-3 text-muted-foreground text-xs">
            {summary.online ? t('no_data') : t('offline_hint')}
          </div>
        )}

        {summary.anomaly_count > 0 && (
          <p className="pt-2 text-muted-foreground text-xs">
            {t('anomaly_recent', { count: summary.anomaly_count, hours: ANOMALY_WINDOW_HOURS })}
          </p>
        )}
      </div>
    </div>
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
