import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, countryCodeToFlag, formatSpeed, formatUptime } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function ProgressBar({ value, label, color }: { color: string; label: string; value: number }) {
  const pct = Math.min(100, Math.max(0, value))
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium">{pct.toFixed(1)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div className={cn('h-full rounded-full transition-all', color)} style={{ width: `${pct}%` }} />
      </div>
    </div>
  )
}

function osIcon(os: string | null): string {
  if (!os) {
    return ''
  }
  const lower = os.toLowerCase()
  if (lower.includes('ubuntu') || lower.includes('debian') || lower.includes('linux')) {
    return '🐧'
  }
  if (lower.includes('windows')) {
    return '🪟'
  }
  if (lower.includes('macos') || lower.includes('darwin')) {
    return '🍎'
  }
  if (lower.includes('freebsd') || lower.includes('openbsd')) {
    return '😈'
  }
  return ''
}

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation('servers')
  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  return (
    <Link
      className="group block rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
      params={{ id: server.id }}
      search={{ range: 'realtime' }}
      to="/servers/$id"
    >
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-1.5 truncate">
          {flag && (
            <span className="shrink-0 text-sm" title={server.country_code ?? ''}>
              {flag}
            </span>
          )}
          {osEmoji && (
            <span className="shrink-0 text-sm" title={server.os ?? ''}>
              {osEmoji}
            </span>
          )}
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
        </div>
        <StatusBadge online={server.online} />
      </div>

      <div className="space-y-2.5">
        <ProgressBar color="bg-chart-1" label={t('col_cpu')} value={server.cpu} />
        <ProgressBar color="bg-chart-2" label={t('col_memory')} value={memoryPct} />
        <ProgressBar color="bg-chart-3" label={t('col_disk')} value={diskPct} />
      </div>

      <div className="mt-3 flex items-center justify-between text-muted-foreground text-xs">
        <div className="flex gap-3">
          <span title={t('chart_net_in')}>{formatSpeed(server.net_in_speed)}</span>
          <span title={t('chart_net_out')}>{formatSpeed(server.net_out_speed)}</span>
        </div>
        <span title={t('col_uptime')}>{formatUptime(server.uptime)}</span>
      </div>
    </Link>
  )
}
