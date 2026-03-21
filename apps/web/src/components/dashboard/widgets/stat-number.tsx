import { Activity, Cpu, MemoryStick, Server, Wifi } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { formatBytes } from '@/lib/utils'
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

function computeMetric(metric: string, servers: ServerMetrics[]): { sub?: string; value: string } {
  const online = servers.filter((s) => s.online)
  const onlineCount = online.length

  switch (metric) {
    case 'server_count':
      return {
        value: `${onlineCount} / ${servers.length}`,
        sub: `${servers.length - onlineCount} offline`
      }
    case 'avg_cpu': {
      if (onlineCount === 0) {
        return { value: '0.0%' }
      }
      const avg = online.reduce((sum, s) => sum + s.cpu, 0) / onlineCount
      return { value: `${avg.toFixed(1)}%` }
    }
    case 'avg_memory': {
      if (onlineCount === 0) {
        return { value: '0.0%' }
      }
      const avg =
        online.reduce((sum, s) => {
          return sum + (s.mem_total > 0 ? (s.mem_used / s.mem_total) * 100 : 0)
        }, 0) / onlineCount
      return { value: `${avg.toFixed(1)}%` }
    }
    case 'total_bandwidth': {
      const total = online.reduce((sum, s) => sum + s.net_in_speed + s.net_out_speed, 0)
      return { value: formatBytes(total), sub: '/s' }
    }
    case 'health':
      return { value: onlineCount > 0 ? 'Healthy' : 'No Data' }
    default:
      return { value: '--' }
  }
}

const METRIC_LABELS: Record<string, string> = {
  server_count: 'stat_servers',
  avg_cpu: 'avg_cpu',
  avg_memory: 'avg_memory',
  total_bandwidth: 'total_bandwidth',
  health: 'online'
}

export function StatNumberWidget({ config, servers }: StatNumberWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { metric } = config
  const Icon = METRIC_ICONS[metric] ?? Server
  const result = useMemo(() => computeMetric(metric, servers), [metric, servers])
  const label = config.label ?? t(METRIC_LABELS[metric] ?? metric)

  return (
    <div className="flex h-full items-center gap-3 rounded-lg border bg-card p-4">
      <div className="rounded-md bg-muted p-2">
        <Icon className="size-5 text-muted-foreground" />
      </div>
      <div className="min-w-0">
        <p className="truncate font-semibold text-lg leading-tight">{result.value}</p>
        <p className="truncate text-muted-foreground text-xs">{label}</p>
        {result.sub && <p className="truncate text-muted-foreground text-xs">{result.sub}</p>}
      </div>
    </div>
  )
}
