import { useTranslation } from 'react-i18next'
import { DashboardCard } from '@/components/dashboard/dashboard-card'
import { WidgetValue } from '@/components/dashboard/widget-chrome'
import type { ServerMetrics } from '@/lib/server-catalog'
import { getUtilizationTextColor } from '@/lib/utilization-colors'
import { cn, formatBytes } from '@/lib/utils'
import type { StatNumberConfig } from '@/lib/widget-types'

interface StatNumberWidgetProps {
  config: StatNumberConfig
  servers: ServerMetrics[]
  title?: string | null
}

type StatTone = 'healthy' | 'danger' | null

interface StatResult {
  supporting: string
  /** Status dot before the supporting line; null when the metric has no status. */
  tone: StatTone
  value: string
  /** Text color for the value; only set when the reading crosses a threshold. */
  valueClassName?: string
}

const TONE_DOT_CLASS: Record<Exclude<StatTone, null>, string> = {
  healthy: 'bg-status-healthy',
  danger: 'bg-status-danger'
}

function computeMetric(
  metric: string,
  servers: ServerMetrics[],
  t: (key: string, options?: Record<string, number | string>) => string
): StatResult {
  const online = servers.filter((server) => server.online)
  const onlineCount = online.length
  const onlineSummary = t('servers_online', { online: onlineCount, total: servers.length })
  const fleetTone: StatTone = onlineCount === servers.length ? 'healthy' : 'danger'

  switch (metric) {
    case 'server_count':
      return {
        value: `${onlineCount} / ${servers.length}`,
        supporting: t('offline_count', { count: servers.length - onlineCount }),
        tone: servers.length === 0 ? null : fleetTone
      }
    case 'avg_cpu': {
      if (onlineCount === 0) {
        return { value: '0.0%', supporting: onlineSummary, tone: null }
      }
      const average = online.reduce((sum, server) => sum + server.cpu, 0) / onlineCount
      return {
        value: `${average.toFixed(1)}%`,
        supporting: onlineSummary,
        tone: null,
        valueClassName: getUtilizationTextColor(average)
      }
    }
    case 'avg_memory': {
      if (onlineCount === 0) {
        return { value: '0.0%', supporting: onlineSummary, tone: null }
      }
      const average =
        online.reduce((sum, server) => {
          return sum + (server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0)
        }, 0) / onlineCount
      return {
        value: `${average.toFixed(1)}%`,
        supporting: onlineSummary,
        tone: null,
        valueClassName: getUtilizationTextColor(average)
      }
    }
    case 'total_bandwidth': {
      const inbound = online.reduce((sum, server) => sum + server.net_in_speed, 0)
      const outbound = online.reduce((sum, server) => sum + server.net_out_speed, 0)
      return {
        value: `${formatBytes(inbound + outbound)}${t('per_second')}`,
        supporting: `↓ ${formatBytes(inbound)} · ↑ ${formatBytes(outbound)}`,
        tone: null
      }
    }
    case 'health': {
      if (servers.length === 0) {
        return { value: t('no_data'), supporting: onlineSummary, tone: null }
      }
      const offlineCount = servers.length - onlineCount
      return {
        value: offlineCount === 0 ? t('healthy') : t('offline_count', { count: offlineCount }),
        supporting: onlineSummary,
        tone: fleetTone,
        valueClassName: offlineCount === 0 ? undefined : 'text-status-danger-text'
      }
    }
    default:
      return { value: '--', supporting: onlineSummary, tone: null }
  }
}

const METRIC_LABELS: Record<string, string> = {
  server_count: 'stat_servers',
  avg_cpu: 'avg_cpu',
  avg_memory: 'avg_memory',
  total_bandwidth: 'total_bandwidth',
  health: 'stat_status'
}

export function StatNumberWidget({ config, servers, title }: StatNumberWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric } = config
  const result = computeMetric(metric, servers, t)
  const label = title ?? config.label ?? t(METRIC_LABELS[metric] ?? metric)

  return (
    <DashboardCard
      className="flex h-full flex-col justify-center gap-0.5 overflow-hidden bg-card px-5 py-2"
      data-metric={metric}
      data-testid="stat-number-widget"
    >
      <p className="shrink-0 truncate font-medium text-foreground/75 text-xs leading-4" data-testid="stat-number-label">
        {label}
      </p>
      <WidgetValue
        className={cn('shrink-0 py-px text-2xl', result.valueClassName)}
        data-testid="stat-number-value"
        formatted={result.value}
      />
      <p
        className="flex min-w-0 shrink-0 items-center gap-1.5 text-muted-foreground text-xs leading-4"
        data-testid="stat-number-supporting"
      >
        {result.tone ? (
          <span aria-hidden="true" className={cn('size-1.5 shrink-0 rounded-full', TONE_DOT_CLASS[result.tone])} />
        ) : null}
        <span className="truncate">{result.supporting}</span>
      </p>
    </DashboardCard>
  )
}
