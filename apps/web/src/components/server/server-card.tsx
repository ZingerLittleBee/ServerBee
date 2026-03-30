import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { SparklineChart } from '@/components/ui/sparkline'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn, countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function ProgressBar({ value, label, color }: { color: string; label: string; value: number }) {
  const pct = Math.min(100, Math.max(0, value))
  return (
    <div className="flex flex-col gap-1">
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

function getBarColor(pct: number): string {
  if (pct > 90) {
    return 'bg-red-500'
  }
  if (pct > 70) {
    return 'bg-amber-500'
  }
  return 'bg-emerald-500'
}

function formatLoad(load: number): string {
  return load.toFixed(2)
}

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '-'
  }
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(loss: number): string {
  return `${loss.toFixed(1)}%`
}

function getLatencyColorClass(ms: number | null): string {
  if (ms == null) {
    return 'text-muted-foreground'
  }
  if (ms < 50) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (ms < 100) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function getLossColorClass(loss: number): string {
  if (loss < 1) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (loss < 5) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation(['servers'])
  const { data: networkData } = useNetworkRealtime(server.id)

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const allResults = Object.values(networkData).flat()
  const latencyData = allResults
    .map((r) => r.avg_latency)
    .filter((v): v is number => v != null)
    .slice(-20)
  const lossData = allResults.map((r) => r.packet_loss * 100).slice(-20)

  const avgLatency = latencyData.length > 0 ? latencyData.reduce((a, b) => a + b, 0) / latencyData.length : null
  const avgLoss = lossData.length > 0 ? lossData.reduce((a, b) => a + b, 0) / lossData.length : 0

  return (
    <Link
      className="group flex h-full flex-col rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
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

      <div className="mb-3 flex flex-col gap-2">
        <ProgressBar color="bg-chart-1" label={t('col_cpu')} value={server.cpu} />
        <ProgressBar color="bg-chart-2" label={t('col_memory')} value={memoryPct} />
        <ProgressBar color="bg-chart-3" label={t('col_disk')} value={diskPct} />
      </div>

      <div className="mb-3 grid grid-cols-4 gap-2">
        <CompactMetric label={t('card_load')} value={formatLoad(server.load1)} />
        <CompactMetric label={t('card_processes')} value={server.process_count} />
        <CompactMetric label={t('card_tcp')} value={server.tcp_conn} />
        <CompactMetric label={t('card_udp')} value={server.udp_conn} />
      </div>

      <div className="mb-3 flex items-center gap-4">
        {server.swap_total > 0 && (
          <div className="flex flex-1 items-center gap-2">
            <span className="text-[10px] text-muted-foreground">{t('card_swap')}</span>
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-muted">
              <div
                className={cn('h-full rounded-full transition-all', getBarColor(swapPct))}
                style={{ width: `${swapPct}%` }}
              />
            </div>
            <span className="text-[10px] tabular-nums">{swapPct.toFixed(0)}%</span>
          </div>
        )}
        <span className="text-muted-foreground text-xs">{formatUptime(server.uptime)}</span>
      </div>

      <div className="mt-auto flex items-end justify-between border-t pt-3">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-3 text-xs">
            <span className="text-muted-foreground" title={t('chart_net_in')}>
              ↓{formatSpeed(server.net_in_speed)}
            </span>
            <span className="text-muted-foreground" title={t('chart_net_out')}>
              ↑{formatSpeed(server.net_out_speed)}
            </span>
          </div>
          <span className="text-[10px] text-muted-foreground">
            {t('card_net_total')}: {formatBytes(server.net_in_transfer + server.net_out_transfer)}
          </span>
        </div>

        {latencyData.length > 0 && (
          <div className="flex flex-col items-end gap-1">
            <div className="flex items-center gap-2">
              <SparklineChart color="var(--color-chart-4)" data={latencyData} height={20} width={50} />
              <span className={cn('font-medium text-xs', getLatencyColorClass(avgLatency))}>
                {formatLatency(avgLatency)}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <SparklineChart color="var(--color-chart-5)" data={lossData} height={20} width={50} />
              <span className={cn('font-medium text-xs', getLossColorClass(avgLoss))}>{formatPacketLoss(avgLoss)}</span>
            </div>
          </div>
        )}
      </div>
    </Link>
  )
}
