import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { RingChart } from '@/components/ui/ring-chart'
import { UptimeBar } from '@/components/ui/uptime-bar'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
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

function getRingColor(pct: number, brandColor: string): string {
  if (pct > 90) {
    return '#ef4444'
  }
  if (pct > 70) {
    return '#f59e0b'
  }
  return brandColor
}

function getLatencyColor(ms: number | null): string {
  if (ms == null) {
    return '#ef4444'
  }
  if (ms < 50) {
    return '#10b981'
  }
  if (ms < 100) {
    return '#f59e0b'
  }
  return '#ef4444'
}

function getLossColor(loss: number | null): string {
  if (loss == null) {
    return '#ef4444'
  }
  if (loss < 1) {
    return '#10b981'
  }
  if (loss < 5) {
    return '#f59e0b'
  }
  return '#ef4444'
}

function getLatencyTextClass(ms: number | null): string {
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

function getLossTextClass(loss: number): string {
  if (loss < 1) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (loss < 5) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
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

function formatLoad(load: number): string {
  return load.toFixed(2)
}

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation(['servers'])
  const { data: networkData } = useNetworkRealtime(server.id)

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const { latencyData, lossData, avgLatency, avgLoss } = useMemo(() => {
    const allResults = Object.values(networkData)
      .flat()
      .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
      .slice(-20)

    const latency = allResults.map((r) => r.avg_latency)
    const loss = allResults.map((r) => r.packet_loss * 100)

    const validLatencies = latency.filter((v): v is number => v != null)
    const avg = validLatencies.length > 0 ? validLatencies.reduce((a, b) => a + b, 0) / validLatencies.length : null
    const avgL = loss.length > 0 ? loss.reduce((a, b) => a + b, 0) / loss.length : 0

    return { latencyData: latency, lossData: loss, avgLatency: avg, avgLoss: avgL }
  }, [networkData])

  return (
    <Link
      className="group flex h-full flex-col rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
      params={{ id: server.id }}
      search={{ range: 'realtime' }}
      to="/servers/$id"
    >
      {/* Header */}
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

      {/* Ring Charts */}
      <div className="mb-3 flex justify-around">
        <RingChart color={getRingColor(server.cpu, 'var(--color-chart-1)')} label={t('col_cpu')} value={server.cpu} />
        <RingChart color={getRingColor(memoryPct, 'var(--color-chart-2)')} label={t('col_memory')} value={memoryPct} />
        <RingChart color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct} />
      </div>

      {/* System Metrics Row */}
      <div className="mb-1.5 grid grid-cols-5 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric className="items-center" label={t('card_load')} value={formatLoad(server.load1)} />
        <CompactMetric className="items-center" label={t('card_processes')} value={server.process_count} />
        <CompactMetric className="items-center" label={t('card_tcp')} value={server.tcp_conn} />
        <CompactMetric className="items-center" label={t('card_udp')} value={server.udp_conn} />
        <CompactMetric className="items-center" label={t('card_swap')} value={`${swapPct.toFixed(0)}%`} />
      </div>

      {/* Network Metrics Row */}
      <div className="mb-3 grid grid-cols-4 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric
          className="items-center"
          label={t('card_net_in_speed')}
          value={formatSpeed(server.net_in_speed)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_net_out_speed')}
          value={formatSpeed(server.net_out_speed)}
        />
        <CompactMetric
          className="items-center"
          label={t('card_net_total')}
          value={formatBytes(server.net_in_transfer + server.net_out_transfer)}
        />
        <CompactMetric className="items-center" label={t('col_uptime')} value={formatUptime(server.uptime)} />
      </div>

      {/* Network Quality */}
      {latencyData.length > 0 && (
        <div className="mt-auto border-t pt-3">
          <div className="mb-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_latency')}</span>
              <span className={`font-medium text-xs ${getLatencyTextClass(avgLatency)}`}>
                {formatLatency(avgLatency)}
              </span>
            </div>
            <UptimeBar ariaLabel="Latency trend" data={latencyData} getColor={getLatencyColor} />
          </div>
          <div>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_packet_loss')}</span>
              <span className={`font-medium text-xs ${getLossTextClass(avgLoss)}`}>{formatPacketLoss(avgLoss)}</span>
            </div>
            <UptimeBar ariaLabel="Packet loss trend" data={lossData} getColor={getLossColor} />
          </div>
        </div>
      )}
    </Link>
  )
}
