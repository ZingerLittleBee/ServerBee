import { Activity, Cpu, MemoryStick, Server, Wifi } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, formatBytes } from '@/lib/utils'
import type { StatNumberConfig } from '@/lib/widget-types'

interface StatNumberWidgetProps {
  config: StatNumberConfig
  servers: ServerMetrics[]
}

const METRIC_ICONS: Record<string, typeof Server> = {
  server_count: Server,
  avg_cpu: Cpu,
  avg_memory: MemoryStick,
  total_bandwidth: Wifi,
  health: Activity
}

const METRIC_STYLES: Record<string, { icon: string; iconBg: string; surface: string }> = {
  server_count: {
    icon: 'text-primary',
    iconBg: 'bg-primary/10',
    surface: 'border-border/70 bg-gradient-to-br from-primary/10 via-card to-card'
  },
  avg_cpu: {
    icon: 'text-chart-4',
    iconBg: 'bg-chart-4/10',
    surface: 'border-border/70 bg-gradient-to-br from-chart-4/10 via-card to-card'
  },
  avg_memory: {
    icon: 'text-chart-3',
    iconBg: 'bg-chart-3/10',
    surface: 'border-border/70 bg-gradient-to-br from-chart-3/12 via-card to-card'
  },
  total_bandwidth: {
    icon: 'text-chart-1',
    iconBg: 'bg-chart-1/10',
    surface: 'border-border/70 bg-gradient-to-br from-chart-1/10 via-card to-card'
  },
  health: {
    icon: 'text-chart-2',
    iconBg: 'bg-chart-2/10',
    surface: 'border-border/70 bg-gradient-to-br from-chart-2/12 via-card to-card'
  }
}

function computeMetric(
  metric: string,
  servers: ServerMetrics[],
  t: (key: string, options?: Record<string, number | string>) => string
): { supporting: string; value: string } {
  const online = servers.filter((server) => server.online)
  const onlineCount = online.length
  const onlineSummary = t('servers_online', { online: onlineCount, total: servers.length })

  switch (metric) {
    case 'server_count':
      return {
        value: `${onlineCount} / ${servers.length}`,
        supporting: t('offline_count', { count: servers.length - onlineCount })
      }
    case 'avg_cpu': {
      if (onlineCount === 0) {
        return { value: '0.0%', supporting: onlineSummary }
      }
      const average = online.reduce((sum, server) => sum + server.cpu, 0) / onlineCount
      return { value: `${average.toFixed(1)}%`, supporting: onlineSummary }
    }
    case 'avg_memory': {
      if (onlineCount === 0) {
        return { value: '0.0%', supporting: onlineSummary }
      }
      const average =
        online.reduce((sum, server) => {
          return sum + (server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0)
        }, 0) / onlineCount
      return { value: `${average.toFixed(1)}%`, supporting: onlineSummary }
    }
    case 'total_bandwidth': {
      const total = online.reduce((sum, server) => sum + server.net_in_speed + server.net_out_speed, 0)
      return { value: formatBytes(total), supporting: t('per_second') }
    }
    case 'health':
      return {
        value: t(onlineCount > 0 ? 'healthy' : 'no_data'),
        supporting: onlineSummary
      }
    default:
      return { value: '--', supporting: onlineSummary }
  }
}

const METRIC_LABELS: Record<string, string> = {
  server_count: 'stat_servers',
  avg_cpu: 'avg_cpu',
  avg_memory: 'avg_memory',
  total_bandwidth: 'total_bandwidth',
  health: 'healthy'
}

export function StatNumberWidget({ config, servers }: StatNumberWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric } = config
  const Icon = METRIC_ICONS[metric] ?? Server
  const metricStyles = METRIC_STYLES[metric] ?? METRIC_STYLES.server_count
  const result = computeMetric(metric, servers, t)
  const label = config.label ?? t(METRIC_LABELS[metric] ?? metric)

  return (
    <div
      className={cn(
        'flex h-full min-w-0 items-center gap-3 overflow-hidden rounded-xl border px-3.5 shadow-sm',
        metricStyles.surface
      )}
      data-metric={metric}
      data-testid="stat-number-widget"
    >
      <div
        className={cn('flex size-9 shrink-0 items-center justify-center rounded-[10px]', metricStyles.iconBg)}
        data-testid="stat-number-icon-shell"
      >
        <Icon className={cn('size-[18px]', metricStyles.icon)} />
      </div>

      <div className="min-w-0 flex-1">
        <p
          className="truncate font-medium text-[0.625rem] text-muted-foreground uppercase leading-tight tracking-[0.12em]"
          data-testid="stat-number-label"
        >
          {label}
        </p>
        <p
          className="truncate font-bold text-[1.375rem] leading-tight tracking-[-0.03em]"
          data-testid="stat-number-value"
        >
          {result.value}
        </p>
        <p
          className="truncate text-[0.6875rem] text-muted-foreground leading-tight"
          data-testid="stat-number-supporting"
        >
          {result.supporting}
        </p>
      </div>
    </div>
  )
}
